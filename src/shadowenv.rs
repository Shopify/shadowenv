use anyhow::Error;

use crate::{features::Feature, undo};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env,
};

#[derive(Debug)]
pub struct Shadowenv {
    /// the mutated/modified env: the final state we want to be in after eval'ing exports.
    env: HashMap<String, String>,
    /// the outer env, reconstructed by undoing $__shadowenv_data
    unshadowed_env: HashMap<String, String>,
    /// the env inherited from the calling process, untouched.
    initial_env: HashMap<String, String>,
    /// names of variables which are treated as pathlists by the program
    lists: HashSet<String>,
    /// list of features provided by all plugins
    features: HashSet<Feature>,
    target_hash: u64,
}

impl Shadowenv {
    pub fn load_shadowenv_data_or_legacy_fallback(fallback_data: Option<String>) -> String {
        match env::var("__shadowenv_data") {
            Ok(priority_data) => priority_data,
            Err(_) => fallback_data.unwrap_or_else(|| "".to_string()),
        }
    }

    pub fn new(
        env: HashMap<String, String>,
        shadowenv_data: undo::Data,
        target_hash: u64,
    ) -> Shadowenv {
        let unshadowed_env = Shadowenv::unshadow(&env, shadowenv_data);

        Shadowenv {
            env: unshadowed_env.clone(),
            unshadowed_env,
            initial_env: env,
            lists: HashSet::new(),
            features: HashSet::new(),
            target_hash,
        }
    }

    fn unshadow(
        env: &HashMap<String, String>,
        shadowenv_data: undo::Data,
    ) -> HashMap<String, String> {
        let mut result = env.clone();
        for scalar in shadowenv_data.scalars {
            if env_get(&result, scalar.name.clone()) == scalar.current {
                env_set(&mut result, scalar.name, scalar.original);
            }
        }
        for list in shadowenv_data.lists {
            for addition in list.additions {
                env_remove_from_pathlist(&mut result, list.name.clone(), addition);
            }
            // TODO(burke): figure out a way to preserve approximate ordering
            for deletion in list.deletions {
                env_prepend_to_pathlist(&mut result, list.name.clone(), deletion);
            }
        }
        result
    }

    pub fn shadowenv_data(&self) -> undo::Data {
        let mut changes: BTreeMap<String, Option<String>> = BTreeMap::new();
        let varnames = self.all_relevant_varnames();

        for varname in varnames {
            let a = self.env.get(&varname);
            let b = self.unshadowed_env.get(&varname);
            if a != b {
                changes.insert(varname, a.cloned());
            }
        }

        let mut data = undo::Data::new();

        for (varname, final_value) in changes {
            if self.lists.contains(&varname) {
                let unshadowed_parts: Vec<&str> = match self.unshadowed_env.get(&varname) {
                    Some(s) => s.split(':').collect(),
                    None => vec![],
                };
                let final_parts: Vec<&str> = match self.env.get(&varname) {
                    Some(s) => s.split(':').collect(),
                    None => vec![],
                };
                let (additions, deletions) = diff_vecs(unshadowed_parts, final_parts);
                data.add_list(varname, additions, deletions);
            } else {
                let unshadowed_value = self.unshadowed_env.get(&varname).map(|s| s.to_string());
                data.add_scalar(varname, unshadowed_value, final_value);
            }
        }
        data
    }

    fn format_shadowenv_data(&self) -> Result<String, Error> {
        let d = self.shadowenv_data();
        Ok(format!("{:016x}:", self.target_hash) + &serde_json::to_string(&d)?)
    }

    pub fn exports(&self) -> Result<HashMap<String, Option<String>>, Error> {
        let mut changes: HashMap<String, Option<String>> = HashMap::new();
        let varnames = self.all_relevant_varnames();

        changes.insert(
            "__shadowenv_data".to_string(),
            Some(self.format_shadowenv_data()?),
        );

        for varname in varnames {
            let a = self.env.get(&varname);
            let b = self.initial_env.get(&varname);
            if a != b {
                changes.insert(varname, a.cloned());
            }
        }
        Ok(changes)
    }

    pub fn set(&mut self, a: &str, b: Option<&str>) {
        env_set(&mut self.env, a.to_string(), b.map(|s| s.to_string()))
    }

    pub fn get(&self, a: &str) -> Option<String> {
        env_get(&self.env, a.to_string())
    }

    pub fn remove_from_pathlist(&mut self, a: &str, b: &str) {
        self.inform_list(a);
        env_remove_from_pathlist(&mut self.env, a.to_string(), b.to_string())
    }

    pub fn remove_from_pathlist_containing(&mut self, a: &str, b: &str) {
        self.inform_list(a);
        env_remove_from_pathlist_containing(&mut self.env, a.to_string(), b.to_string())
    }

    pub fn append_to_pathlist(&mut self, a: &str, b: &str) {
        self.inform_list(a);
        env_append_to_pathlist(&mut self.env, a.to_string(), b.to_string())
    }

    pub fn prepend_to_pathlist(&mut self, a: &str, b: &str) {
        self.inform_list(a);
        env_prepend_to_pathlist(&mut self.env, a.to_string(), b.to_string())
    }

    pub fn add_feature(&mut self, name: &str, version: Option<&str>) {
        let feature = Feature::new(name.to_string(), version.map(|s| s.to_string()));
        self.features.insert(feature);
    }

    pub fn features(&self) -> HashSet<Feature> {
        self.features.iter().cloned().collect()
    }

    fn inform_list(&mut self, a: &str) {
        self.lists.insert(a.to_string());
    }

    fn all_relevant_varnames(&self) -> HashSet<String> {
        let mut keys: HashSet<String> = self.env.keys().map(String::from).collect();
        keys.extend(self.initial_env.keys().map(String::from));
        keys
    }
}

fn env_set(env: &mut HashMap<String, String>, a: String, b: Option<String>) {
    match b {
        Some(string) => {
            env.insert(a, string);
        }
        None => {
            env.remove(&a);
        }
    }
}

fn env_get(env: &HashMap<String, String>, a: String) -> Option<String> {
    env.get(&a).cloned()
}

fn env_remove_from_pathlist(env: &mut HashMap<String, String>, a: String, b: String) {
    let curr = env.get(&a);
    let mut items = match curr {
        Some(existing) => existing.split(":").collect::<Vec<&str>>(),
        None => vec![],
    };

    if let Some(index) = items.iter().position(|x| *x == b) {
        items.remove(index);
        if items.is_empty() {
            env.remove(&a);
        } else {
            let next = items.join(":");
            env.insert(a, next);
        }
    }
}

fn env_remove_from_pathlist_containing(env: &mut HashMap<String, String>, a: String, b: String) {
    let curr = env.get(&a);
    let items = match curr {
        Some(existing) => existing.split(':').collect::<Vec<&str>>(),
        None => vec![],
    };

    let items = items.into_iter().skip_while(|x| (*x).contains(&b));
    let items: Vec<&str> = items.collect();
    if items.is_empty() {
        env.remove(&a);
    } else {
        let next = items.join(":");
        env.insert(a, next);
    }
}

fn env_append_to_pathlist(env: &mut HashMap<String, String>, a: String, b: String) {
    let curr = env.get(&a);
    let mut items = match curr {
        Some(existing) => existing.split(':').collect::<Vec<&str>>(),
        None => vec![],
    };
    items.push(&b);
    let next = items.join(":");
    env.insert(a, next);
}

fn env_prepend_to_pathlist(env: &mut HashMap<String, String>, a: String, b: String) {
    let curr = env.get(&a);
    let mut items = match curr {
        Some(existing) => existing.split(':').collect::<Vec<&str>>(),
        None => vec![],
    };
    items.insert(0, &b);
    let next = items.join(":");
    env.insert(a, next);
}

fn diff_vecs(oldvec: Vec<&str>, newvec: Vec<&str>) -> (Vec<String>, Vec<String>) {
    let mut additions: Vec<String> = vec![];
    let mut deletions: Vec<String> = vec![];

    let mut oldset: HashSet<String> = HashSet::new();
    for oldval in &oldvec {
        oldset.insert(oldval.to_string());
    }

    let mut newset: HashSet<String> = HashSet::new();
    for newval in &newvec {
        newset.insert(newval.to_string());
    }

    for oldval in oldvec {
        if !newset.contains(&oldval.to_string()) {
            deletions.push(oldval.to_string());
        }
    }

    for newval in newvec {
        if !oldset.contains(&newval.to_string()) {
            additions.push(newval.to_string());
        }
    }

    (additions, deletions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::undo::{Data, List, Scalar};
    use std::collections::HashMap;

    fn build_shadow_env(env_variables: Vec<(&str, &str)>, data: Data) -> Shadowenv {
        let env = env_variables
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>();
        Shadowenv::new(env, data, 123456789)
    }

    #[test]
    fn test_get_set() {
        let mut shadowenv = build_shadow_env(vec![], Default::default());
        shadowenv.set("toto", Some("tata"));
        assert_eq!(shadowenv.get("toto"), Some("tata".to_string()))
    }

    #[test]
    fn test_path_manipulation() {
        let mut shadowenv = build_shadow_env(vec![], Default::default());
        shadowenv.append_to_pathlist("field1", "v1");
        shadowenv.prepend_to_pathlist("field1", "v0");

        assert_eq!(shadowenv.get("field1"), Some("v0:v1".to_string()));
        shadowenv.remove_from_pathlist("field1", "v0");
        assert_eq!(shadowenv.get("field1"), Some("v1".to_string()))
    }

    #[test]
    fn test_shadowenv_data() {
        let mut shadowenv = build_shadow_env(
            vec![("VAR_A", "v0"), ("VAR_B", "v0"), ("PATH", "/path1:/path2")],
            Default::default(),
        );
        shadowenv.append_to_pathlist("PATH", "/path3");
        shadowenv.prepend_to_pathlist("PATH", "/path4");
        shadowenv.remove_from_pathlist("PATH", "/path1");

        shadowenv.set("VAR_A", Some("v2"));
        shadowenv.set("VAR_B", None);
        shadowenv.set("VAR_C", Some("v3"));

        let expected = Data {
            scalars: vec![
                Scalar {
                    name: "VAR_A".to_string(),
                    original: Some("v0".to_string()),
                    current: Some("v2".to_string()),
                },
                Scalar {
                    name: "VAR_B".to_string(),
                    original: Some("v0".to_string()),
                    current: None,
                },
                Scalar {
                    name: "VAR_C".to_string(),
                    original: None,
                    current: Some("v3".to_string()),
                },
            ],
            lists: vec![List {
                name: "PATH".to_string(),
                additions: vec!["/path4".to_string(), "/path3".to_string()],
                deletions: vec!["/path1".to_string()],
            }],
        };

        let expected_formatted_data = r#"00000000075bcd15:{"scalars":[{"name":"VAR_A","original":"v0","current":"v2"},{"name":"VAR_B","original":"v0","current":null},{"name":"VAR_C","original":null,"current":"v3"}],"lists":[{"name":"PATH","additions":["/path4","/path3"],"deletions":["/path1"]}]}"#;

        assert_eq!(shadowenv.shadowenv_data(), expected);

        assert_eq!(
            shadowenv.format_shadowenv_data().unwrap(),
            expected_formatted_data
        );

        let expected_export: HashMap<_, _> = vec![
            ("VAR_A".to_string(), Some("v2".to_string())),
            (
                "__shadowenv_data".to_string(),
                Some(expected_formatted_data.to_string()),
            ),
            ("PATH".to_string(), Some("/path4:/path2:/path3".to_string())),
            ("VAR_B".to_string(), None),
            ("VAR_C".to_string(), Some("v3".to_string())),
        ]
        .into_iter()
        .collect();

        assert_eq!(shadowenv.exports().unwrap(), expected_export);
    }
}

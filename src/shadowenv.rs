use crate::{features::Feature, undo};
use anyhow::Error;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env,
    path::PathBuf,
};

/// Characters that may never appear in an environment variable name, with the
/// reason reported to whoever wrote the offending shadowlisp.
///
/// 0x1E and 0x1F are the record and field separators of the `--porcelain`
/// output format. That format is parsed positionally, so a name containing
/// either one truncates its own record and adds spurious ones. `=` and NUL
/// cannot occur in a real environment entry at all (`execve` encodes entries as
/// `NAME=VALUE`, and `std::env::set_var` panics on both), and a newline breaks
/// every line-oriented consumer.
const FORBIDDEN_NAME_CHARS: [(char, &str); 6] = [
    ('\0', "names may not contain a NUL byte"),
    ('\n', "names may not contain a newline"),
    ('\r', "names may not contain a carriage return"),
    ('=', "names may not contain '='"),
    (
        '\x1e',
        "names may not contain 0x1E, the porcelain record separator",
    ),
    (
        '\x1f',
        "names may not contain 0x1F, the porcelain field separator",
    ),
];

#[derive(Debug, thiserror::Error)]
#[error("invalid environment variable name {name:?}: {reason}")]
pub struct InvalidVariableName {
    pub name: String,
    pub reason: &'static str,
}

/// Reject environment variable names that cannot be represented faithfully to
/// downstream consumers. Deliberately narrower than a full POSIX identifier
/// check: names like `foo.bar` are unusual but round-trip safely through every
/// output mode, and rejecting them would break existing shadowlisp.
pub fn validate_var_name(name: &str) -> Result<(), InvalidVariableName> {
    if name.is_empty() {
        return Err(InvalidVariableName {
            name: name.to_string(),
            reason: "names may not be empty",
        });
    }

    for (forbidden, reason) in FORBIDDEN_NAME_CHARS {
        if name.contains(forbidden) {
            return Err(InvalidVariableName {
                name: name.to_string(),
                reason,
            });
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct Shadowenv {
    /// the mutated/modified env: the final state we want to be in after eval'ing exports.
    env: HashMap<String, String>,
    /// the outer env, reconstructed by undoing $__shadowenv_data
    unshadowed_env: HashMap<String, String>,
    // vars that if attempted to be set will be ignored
    no_clobber: HashSet<String>,
    /// the env inherited from the calling process, untouched.
    initial_env: HashMap<String, String>,
    /// names of variables which are treated as pathlists by the program
    lists: HashSet<String>,
    /// list of features provided by all plugins
    features: HashSet<Feature>,
    target_hash: u64,
    prev_dirs: HashSet<PathBuf>,
    current_dirs: HashSet<PathBuf>,
}

impl Shadowenv {
    pub fn from_env() -> String {
        env::var("__shadowenv_data").unwrap_or_else(|_| "".to_owned())
    }

    pub fn new(
        env: HashMap<String, String>,
        shadowenv_data: undo::Data,
        target_hash: u64,
        clobber: bool,
    ) -> Shadowenv {
        Shadowenv::build(env, shadowenv_data, target_hash, clobber, false)
    }

    /// Like `new` without clobbering, except that a variable removed since activation is restored
    /// rather than preserved as an override.
    pub fn repairing(
        env: HashMap<String, String>,
        shadowenv_data: undo::Data,
        target_hash: u64,
    ) -> Shadowenv {
        Shadowenv::build(env, shadowenv_data, target_hash, false, true)
    }

    fn build(
        env: HashMap<String, String>,
        shadowenv_data: undo::Data,
        target_hash: u64,
        clobber: bool,
        restore_removed: bool,
    ) -> Shadowenv {
        let (unshadowed_env, no_clobber, prev_dirs) =
            Shadowenv::unshadow(&env, shadowenv_data, clobber, restore_removed);

        Shadowenv {
            env: unshadowed_env.clone(),
            unshadowed_env,
            no_clobber,
            initial_env: env,
            lists: HashSet::new(),
            features: HashSet::new(),
            target_hash,
            prev_dirs,
            current_dirs: HashSet::new(),
        }
    }

    fn unshadow(
        env: &HashMap<String, String>,
        shadowenv_data: undo::Data,
        clobber: bool,
        restore_removed: bool,
    ) -> (HashMap<String, String>, HashSet<String>, HashSet<PathBuf>) {
        let mut result = env.clone();
        let mut no_clobber = HashSet::new();
        for scalar in shadowenv_data.scalars {
            let current_value = env_get(&result, scalar.name.clone());
            let removed = restore_removed && current_value.is_none() && scalar.current.is_some();

            if !clobber && scalar.no_clobber && !removed {
                no_clobber.insert(scalar.name);
                continue;
            }

            // A no_clobber original is the user's override, not the outer value, and they have
            // since removed it: leave the variable unset outside the shadowenv.
            if removed && scalar.no_clobber {
                continue;
            }

            if current_value == scalar.current || removed {
                env_set(&mut result, scalar.name, scalar.original);
            } else if !clobber {
                no_clobber.insert(scalar.name);
            }
        }
        // TODO: no_clobber for lists
        for list in shadowenv_data.lists {
            for addition in list.additions {
                env_remove_from_pathlist(&mut result, list.name.clone(), addition);
            }
            // TODO(burke): figure out a way to preserve approximate ordering
            for deletion in list.deletions {
                env_prepend_to_pathlist(&mut result, list.name.clone(), deletion);
            }
        }
        (result, no_clobber, shadowenv_data.prev_dirs)
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
                let mut no_clobber = false;
                if self.no_clobber.contains(&varname) {
                    no_clobber = true;
                }

                data.add_scalar(varname, unshadowed_value, final_value, no_clobber);
            }
        }
        data.prev_dirs = self.current_dirs.clone();
        data.fingerprint = Some(data.fingerprint_of(&self.exported_env()));

        data
    }

    /// The environment the shell ends up with once the exports are applied.
    pub fn exported_env(&self) -> HashMap<String, String> {
        let mut env = self.env.clone();
        for varname in &self.no_clobber {
            match self.initial_env.get(varname) {
                Some(value) => env.insert(varname.clone(), value.clone()),
                None => env.remove(varname),
            };
        }
        env
    }

    fn format_shadowenv_data(&self) -> Result<String, Error> {
        let d = self.shadowenv_data();
        Ok(format!("{:016x}:", self.target_hash,) + &serde_json::to_string(&d)?)
    }

    pub fn exports(&self) -> Result<HashMap<String, Option<String>>, Error> {
        let mut changes: HashMap<String, Option<String>> = HashMap::new();
        let varnames = self.all_relevant_varnames();

        changes.insert(
            "__shadowenv_data".to_string(),
            Some(self.format_shadowenv_data()?),
        );

        for varname in varnames {
            if self.should_not_clobber(&varname) {
                continue;
            }

            let a = self.env.get(&varname);
            let b = self.initial_env.get(&varname);
            if a != b {
                changes.insert(varname, a.cloned());
            }
        }
        Ok(changes)
    }

    pub fn set(&mut self, a: &str, b: Option<&str>) -> Result<(), InvalidVariableName> {
        validate_var_name(a)?;
        env_set(&mut self.env, a.to_string(), b.map(|s| s.to_string()));
        Ok(())
    }

    pub fn get(&self, a: &str) -> Option<String> {
        env_get(&self.env, a.to_string())
    }

    pub fn remove_from_pathlist(&mut self, a: &str, b: &str) -> Result<(), InvalidVariableName> {
        validate_var_name(a)?;
        self.inform_list(a);
        env_remove_from_pathlist(&mut self.env, a.to_string(), b.to_string());
        Ok(())
    }

    pub fn remove_from_pathlist_containing(
        &mut self,
        a: &str,
        b: &str,
    ) -> Result<(), InvalidVariableName> {
        validate_var_name(a)?;
        self.inform_list(a);
        env_remove_from_pathlist_containing(&mut self.env, a.to_string(), b.to_string());
        Ok(())
    }

    pub fn append_to_pathlist(&mut self, a: &str, b: &str) -> Result<(), InvalidVariableName> {
        validate_var_name(a)?;
        self.inform_list(a);
        env_append_to_pathlist(&mut self.env, a.to_string(), b.to_string());
        Ok(())
    }

    pub fn prepend_to_pathlist(&mut self, a: &str, b: &str) -> Result<(), InvalidVariableName> {
        validate_var_name(a)?;
        self.inform_list(a);
        env_prepend_to_pathlist(&mut self.env, a.to_string(), b.to_string());
        Ok(())
    }

    pub fn add_feature(&mut self, name: &str, version: Option<&str>) {
        let feature = Feature::new(name.to_string(), version.map(|s| s.to_string()));
        self.features.insert(feature);
    }

    pub fn features(&self) -> HashSet<Feature> {
        self.features.iter().cloned().collect()
    }

    pub fn current_dirs(&self) -> HashSet<PathBuf> {
        self.current_dirs.iter().cloned().collect()
    }

    pub fn prev_dirs(&self) -> HashSet<PathBuf> {
        self.prev_dirs.iter().cloned().collect()
    }

    pub fn add_dirs(&mut self, dirs: Vec<PathBuf>) {
        for dir in dirs {
            self.current_dirs.insert(dir);
        }
    }

    pub fn should_not_clobber(&self, varname: &str) -> bool {
        self.no_clobber.contains(varname)
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
    let mut delta: HashMap<&str, i32> = HashMap::new();

    for &item in &oldvec {
        *delta.entry(item).or_insert(0) -= 1;
    }
    for &item in &newvec {
        *delta.entry(item).or_insert(0) += 1;
    }

    let mut additions = Vec::new();
    let mut deletions = Vec::new();

    for (&item, &count) in &delta {
        if count > 0 {
            for _ in 0..count {
                additions.push(item.to_string());
            }
        } else if count < 0 {
            for _ in 0..(-count) {
                deletions.push(item.to_string());
            }
        }
    }

    additions.sort();
    deletions.sort();

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
        Shadowenv::new(env, data, 123456789, false)
    }

    #[test]
    fn test_get_set() {
        let mut shadowenv = build_shadow_env(vec![], Default::default());
        shadowenv.set("toto", Some("tata")).unwrap();
        assert_eq!(shadowenv.get("toto"), Some("tata".to_string()))
    }

    #[test]
    fn test_path_manipulation() {
        let mut shadowenv = build_shadow_env(vec![], Default::default());
        shadowenv.append_to_pathlist("field1", "v1").unwrap();
        shadowenv.prepend_to_pathlist("field1", "v0").unwrap();

        assert_eq!(shadowenv.get("field1"), Some("v0:v1".to_string()));
        shadowenv.remove_from_pathlist("field1", "v0").unwrap();
        assert_eq!(shadowenv.get("field1"), Some("v1".to_string()))
    }

    #[test]
    fn test_shadowenv_data() {
        let mut shadowenv = build_shadow_env(
            vec![("VAR_A", "v0"), ("VAR_B", "v0"), ("PATH", "/path1:/path2")],
            Default::default(),
        );
        shadowenv.append_to_pathlist("PATH", "/path3").unwrap();
        shadowenv.prepend_to_pathlist("PATH", "/path4").unwrap();
        shadowenv.remove_from_pathlist("PATH", "/path1").unwrap();

        shadowenv.set("VAR_A", Some("v2")).unwrap();
        shadowenv.set("VAR_B", None).unwrap();
        shadowenv.set("VAR_C", Some("v3")).unwrap();

        let expected = Data {
            scalars: vec![
                Scalar {
                    name: "VAR_A".to_string(),
                    original: Some("v0".to_string()),
                    current: Some("v2".to_string()),
                    no_clobber: false,
                },
                Scalar {
                    name: "VAR_B".to_string(),
                    original: Some("v0".to_string()),
                    current: None,
                    no_clobber: false,
                },
                Scalar {
                    name: "VAR_C".to_string(),
                    original: None,
                    current: Some("v3".to_string()),
                    no_clobber: false,
                },
            ],
            lists: vec![List {
                name: "PATH".to_string(),
                additions: vec!["/path3".to_string(), "/path4".to_string()],
                deletions: vec!["/path1".to_string()],
            }],
            prev_dirs: Default::default(),
            fingerprint: None,
        };

        let mut actual = shadowenv.shadowenv_data();
        let fingerprint = actual.fingerprint.take().expect("fingerprint recorded");
        assert_eq!(actual, expected);

        let expected_formatted_data = format!(
            r#"00000000075bcd15:{{"scalars":[{{"name":"VAR_A","original":"v0","current":"v2","no_clobber":false}},{{"name":"VAR_B","original":"v0","current":null,"no_clobber":false}},{{"name":"VAR_C","original":null,"current":"v3","no_clobber":false}}],"lists":[{{"name":"PATH","additions":["/path3","/path4"],"deletions":["/path1"]}}],"prev_dirs":[],"fingerprint":"{}"}}"#,
            fingerprint
        );

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

    fn env_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn apply_exports(
        shadowenv: &Shadowenv,
        env: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut shell = env.clone();
        for (name, value) in shadowenv.exports().unwrap() {
            match value {
                Some(value) => shell.insert(name, value),
                None => shell.remove(&name),
            };
        }
        shell
    }

    fn zone(mut shadowenv: Shadowenv) -> Shadowenv {
        shadowenv.prepend_to_pathlist("PATH", "/zone/bin").unwrap();
        shadowenv.set("GEM_HOME", Some("/zone/gem")).unwrap();
        shadowenv
    }

    fn activate(env: &HashMap<String, String>) -> (Data, HashMap<String, String>) {
        let shadowenv = zone(Shadowenv::new(env.clone(), Data::new(), 1, false));
        (shadowenv.shadowenv_data(), apply_exports(&shadowenv, env))
    }

    #[test]
    fn test_exported_env_is_what_the_shell_has_after_applying_exports() {
        let (data, mut shell) = activate(&env_of(&[("PATH", "/usr/bin"), ("KEEP", "1")]));
        shell.insert("GEM_HOME".to_string(), "/mine".to_string());
        shell.insert(
            "PATH".to_string(),
            "/elsewhere:/zone/bin:/usr/bin".to_string(),
        );

        let reapplied = zone(Shadowenv::new(shell.clone(), data, 1, false));
        let mut applied = apply_exports(&reapplied, &shell);
        let mut exported = reapplied.exported_env();
        applied.remove("__shadowenv_data");
        exported.remove("__shadowenv_data");

        assert_eq!(applied, exported);
        assert_eq!(applied.get("GEM_HOME"), Some(&"/mine".to_string()));
    }

    #[test]
    fn test_fingerprint_matches_the_shell_after_activation() {
        let (data, shell) = activate(&env_of(&[("PATH", "/usr/bin")]));

        assert_eq!(data.fingerprint.as_ref().map(String::len), Some(16));
        assert!(!data.drifted(&shell));
    }

    #[test]
    fn test_fingerprint_detects_a_changed_list_and_a_removed_scalar() {
        let (data, shell) = activate(&env_of(&[("PATH", "/usr/bin")]));

        let mut prepended = shell.clone();
        prepended.insert(
            "PATH".to_string(),
            "/elsewhere:/zone/bin:/usr/bin".to_string(),
        );
        assert!(data.drifted(&prepended));

        let mut removed = shell.clone();
        removed.remove("GEM_HOME");
        assert!(data.drifted(&removed));
    }

    #[test]
    fn test_new_keeps_a_removed_variable_unset() {
        let (data, mut shell) = activate(&env_of(&[("PATH", "/usr/bin")]));
        shell.remove("GEM_HOME");

        let reapplied = zone(Shadowenv::new(shell, data, 1, false));

        assert!(!reapplied.exports().unwrap().contains_key("GEM_HOME"));
    }

    #[test]
    fn test_repairing_restores_a_removed_variable_and_the_list_order() {
        let (data, mut shell) = activate(&env_of(&[("PATH", "/usr/bin")]));
        shell.remove("GEM_HOME");
        shell.insert(
            "PATH".to_string(),
            "/elsewhere:/zone/bin:/usr/bin".to_string(),
        );

        let repaired = zone(Shadowenv::repairing(shell.clone(), data, 1));
        let applied = apply_exports(&repaired, &shell);

        assert_eq!(applied.get("GEM_HOME"), Some(&"/zone/gem".to_string()));
        assert_eq!(
            applied.get("PATH"),
            Some(&"/zone/bin:/elsewhere:/usr/bin".to_string())
        );
        assert!(!repaired.shadowenv_data().drifted(&applied));
    }

    #[test]
    fn test_repairing_restores_a_variable_already_marked_no_clobber() {
        let (mut data, mut shell) = activate(&env_of(&[("PATH", "/usr/bin")]));
        for scalar in data.scalars.iter_mut() {
            scalar.no_clobber = scalar.name == "GEM_HOME";
        }
        shell.remove("GEM_HOME");

        let repaired = zone(Shadowenv::repairing(shell.clone(), data, 1));

        assert_eq!(
            apply_exports(&repaired, &shell).get("GEM_HOME"),
            Some(&"/zone/gem".to_string())
        );
    }

    #[test]
    fn test_repairing_keeps_a_deliberately_overridden_variable() {
        let (data, mut shell) = activate(&env_of(&[("PATH", "/usr/bin")]));
        shell.insert("GEM_HOME".to_string(), "/mine".to_string());

        let repaired = zone(Shadowenv::repairing(shell.clone(), data, 1));
        let applied = apply_exports(&repaired, &shell);

        assert_eq!(applied.get("GEM_HOME"), Some(&"/mine".to_string()));
        assert!(!repaired.shadowenv_data().drifted(&applied));
    }
}

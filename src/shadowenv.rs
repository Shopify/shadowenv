use std::cell::{Ref, RefCell, RefMut};
use std::collections::{HashMap, HashSet};

use crate::features::Feature;
use crate::undo;

#[derive(Debug, ForeignValue, FromValueRef)]
pub struct Shadowenv {
    /// the mutated/modified env: the final state we want to be in after eval'ing exports.
    env: RefCell<HashMap<String, String>>,
    /// the outer env, reconstructed by undoing $__shadowenv_data
    unshadowed_env: HashMap<String, String>,
    /// the env inherited from the calling process, untouched.
    initial_env: HashMap<String, String>,
    /// names of variables which are treated as pathlists by the program
    lists: RefCell<HashSet<String>>,
    /// list of features provided by all plugins
    features: RefCell<HashSet<Feature>>,
}

impl Shadowenv {
    pub fn new(env: HashMap<String, String>, shadowenv_data: undo::Data) -> Shadowenv {
        let unshadowed_env = Shadowenv::unshadow(&env, shadowenv_data);

        Shadowenv {
            env: RefCell::new(unshadowed_env.clone()),
            unshadowed_env: unshadowed_env,
            initial_env: env.clone(),
            lists: RefCell::new(HashSet::new()),
            features: RefCell::new(HashSet::new()),
        }
    }

    fn unshadow(
        env: &HashMap<String, String>,
        shadowenv_data: undo::Data,
    ) -> HashMap<String, String> {
        let cell = RefCell::new(env.clone());
        for scalar in shadowenv_data.scalars {
            if env_get(cell.borrow(), scalar.name.clone()) == scalar.current {
                env_set(&mut cell.borrow_mut(), scalar.name, scalar.original);
            }
        }
        for list in shadowenv_data.lists {
            for addition in list.additions {
                env_remove_from_pathlist(&mut cell.borrow_mut(), list.name.clone(), addition);
            }
            // TODO(burke): figure out a way to preserve approximate ordering
            for deletion in list.deletions {
                env_prepend_to_pathlist(&mut cell.borrow_mut(), list.name.clone(), deletion);
            }
        }
        cell.into_inner()
    }

    pub fn shadowenv_data(&self) -> undo::Data {
        let mut changes: HashMap<String, Option<String>> = HashMap::new();
        let varnames = self.all_relevant_varnames();

        let env = self.env.borrow();
        for varname in varnames {
            let a = env.get(&varname);
            let b = self.unshadowed_env.get(&varname);
            if a != b {
                changes.insert(varname, a.cloned());
            }
        }

        let mut data = undo::Data::new();

        let lists = self.lists.borrow();
        for (varname, final_value) in changes {
            if lists.contains(&varname) {
                let unshadowed_parts: Vec<&str> = match self.unshadowed_env.get(&varname) {
                    Some(s) => s.split(":").collect(),
                    None => vec![],
                };
                let final_parts: Vec<&str> = match env.get(&varname) {
                    Some(s) => s.split(":").collect(),
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

    pub fn exports(&self) -> HashMap<String, Option<String>> {
        let mut changes: HashMap<String, Option<String>> = HashMap::new();
        let varnames = self.all_relevant_varnames();

        let env = self.env.borrow();
        for varname in varnames {
            let a = env.get(&varname);
            let b = self.initial_env.get(&varname);
            if a != b {
                changes.insert(varname, a.cloned());
            }
        }
        changes
    }

    pub fn set(&self, a: &str, b: Option<&str>) -> () {
        env_set(
            &mut self.env.borrow_mut(),
            a.to_string(),
            b.map(|s| s.to_string()),
        )
    }

    pub fn get(&self, a: &str) -> Option<String> {
        env_get(self.env.borrow(), a.to_string())
    }

    pub fn remove_from_pathlist(&self, a: &str, b: &str) -> () {
        self.inform_list(a);
        env_remove_from_pathlist(&mut self.env.borrow_mut(), a.to_string(), b.to_string())
    }

    pub fn remove_from_pathlist_containing(&self, a: &str, b: &str) -> () {
        self.inform_list(a);
        env_remove_from_pathlist_containing(
            &mut self.env.borrow_mut(),
            a.to_string(),
            b.to_string(),
        )
    }

    pub fn append_to_pathlist(&self, a: &str, b: &str) -> () {
        self.inform_list(a);
        env_append_to_pathlist(&mut self.env.borrow_mut(), a.to_string(), b.to_string())
    }

    pub fn prepend_to_pathlist(&self, a: &str, b: &str) -> () {
        self.inform_list(a);
        env_prepend_to_pathlist(&mut self.env.borrow_mut(), a.to_string(), b.to_string())
    }

    pub fn add_feature(&self, name: &str, version: Option<&str>) -> () {
        let feature: Feature = Feature::new(name.to_string(), version.map(|s| s.to_string()));
        self.features.borrow_mut().insert(feature);
    }

    pub fn features(&self) -> (HashSet<Feature>) {
        // This is terribly innefficent, but it's a small data set
        self.features.borrow().iter().cloned().collect()
    }

    fn inform_list(&self, a: &str) {
        self.lists.borrow_mut().insert(a.to_string());
    }

    fn all_relevant_varnames(&self) -> HashSet<String> {
        let mut keys: HashSet<String> = HashSet::new();

        let env = self.env.borrow();
        for key in env.keys() {
            keys.insert(key.to_string());
        }
        for key in self.initial_env.keys() {
            keys.insert(key.to_string());
        }
        keys
    }
}

fn env_set(env: &mut RefMut<HashMap<String, String>>, a: String, b: Option<String>) -> () {
    match b {
        Some(string) => {
            env.insert(a, string);
        }
        None => {
            env.remove(&a);
        }
    }
}

fn env_get(env: Ref<HashMap<String, String>>, a: String) -> Option<String> {
    env.get(&a).map(|a| a.clone())
}

fn env_remove_from_pathlist(env: &mut RefMut<HashMap<String, String>>, a: String, b: String) -> () {
    let curr = env.get(&a).cloned().unwrap_or("".to_string());
    let mut items = curr.split(":").collect::<Vec<&str>>();

    if let Some(index) = items.iter().position(|x| *x == b) {
        items.remove(index);
        let next = items.join(":");
        env.insert(a, next.to_string());
    }
    ()
}

fn env_remove_from_pathlist_containing(
    env: &mut RefMut<HashMap<String, String>>,
    a: String,
    b: String,
) -> () {
    let curr = env.get(&a).cloned().unwrap_or("".to_string());
    let items = curr.split(":").collect::<Vec<&str>>();

    let items = items.into_iter().skip_while(|x| (*x).contains(&b));
    let items: Vec<&str> = items.collect();
    let next = items.join(":");
    env.insert(a, next.to_string());
    ()
}

fn env_append_to_pathlist(env: &mut RefMut<HashMap<String, String>>, a: String, b: String) -> () {
    let curr = env.get(&a).cloned().unwrap_or("".to_string());
    let mut items = curr.split(":").collect::<Vec<&str>>();
    items.insert(items.len(), &b);
    let next = items.join(":");
    env.insert(a, next.to_string());
    ()
}

fn env_prepend_to_pathlist(env: &mut RefMut<HashMap<String, String>>, a: String, b: String) -> () {
    let curr = env.get(&a).cloned().unwrap_or("".to_string());
    let mut items = curr.split(":").collect::<Vec<&str>>();
    if items.len() == 1 && items[0] == "" {
        items = vec![];
    }
    items.insert(0, &b);
    let next = items.join(":");
    env.insert(a, next.to_string());
    ()
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

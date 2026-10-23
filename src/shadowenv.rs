use crate::{features::Feature, undo};
use anyhow::Error;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
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

/// Characters that may never appear in a value a program assigns. NUL cannot occur in a real
/// environment entry (and `std::env::set_var` panics on it); 0x1E and 0x1F would let a value
/// forge records in the `--porcelain` output.
const FORBIDDEN_VALUE_CHARS: [(char, &str); 3] = [
    ('\0', "values may not contain a NUL byte"),
    (
        '\x1e',
        "values may not contain 0x1E, the porcelain record separator",
    ),
    (
        '\x1f',
        "values may not contain 0x1F, the porcelain field separator",
    ),
];

/// The longest `NAME=value` entry, terminating NUL included, that Linux passes to a new process
/// (`MAX_ARG_STRLEN`). A longer variable makes every later `exec` in the shell fail.
const MAX_ENTRY_BYTES: usize = 128 * 1024;

/// Most bytes of whole pathlist values (`undo::ListValues`) kept in `__shadowenv_data`. They
/// let deactivation restore pathlists in order, but `__shadowenv_data` is itself an environment
/// variable under `MAX_ENTRY_BYTES`; past this budget a pathlist is undone by its additions and
/// deletions alone, as older versions did.
const MAX_EXACT_BYTES: usize = 32 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("invalid environment variable name {name:?}: {reason}")]
pub struct InvalidVariableName {
    pub name: String,
    pub reason: &'static str,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid value for environment variable {name:?}: {reason}")]
pub struct InvalidVariableValue {
    pub name: String,
    pub reason: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum VariableError {
    #[error(transparent)]
    Name(#[from] InvalidVariableName),
    #[error(transparent)]
    Value(#[from] InvalidVariableValue),
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

/// Shadowenv keeps its own state in this variable. A program that set it could rewrite the
/// record of what to undo, and with it what deactivation restores, unvalidated.
pub const SHADOWENV_DATA: &str = "__shadowenv_data";

/// `validate_var_name`, and also refuse the names programs may not assign.
fn validate_assignable_name(name: &str) -> Result<(), InvalidVariableName> {
    validate_var_name(name)?;
    if name == SHADOWENV_DATA {
        return Err(InvalidVariableName {
            name: name.to_string(),
            reason: "shadowenv keeps its own state in this variable",
        });
    }
    Ok(())
}

/// Reject values that cannot be represented faithfully in the environment or in every output
/// mode. `name` is only used for reporting and for the combined length limit.
pub fn validate_var_value(name: &str, value: &str) -> Result<(), InvalidVariableValue> {
    let invalid = |reason| InvalidVariableValue {
        name: name.to_string(),
        reason,
    };
    for (forbidden, reason) in FORBIDDEN_VALUE_CHARS {
        if value.contains(forbidden) {
            return Err(invalid(reason));
        }
    }
    if name.len() + value.len() + 2 > MAX_ENTRY_BYTES {
        return Err(invalid(
            "a variable's name and value together may not exceed 128 KiB",
        ));
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
    /// per pathlist, entries the program prepended or appended and hasn't removed since
    placed: HashMap<String, BTreeSet<String>>,
    /// list of features provided by all plugins
    features: HashSet<Feature>,
    target_hash: u64,
    prev_dirs: HashSet<PathBuf>,
    current_dirs: HashSet<PathBuf>,
}

impl Shadowenv {
    pub fn from_env() -> String {
        env::var(SHADOWENV_DATA).unwrap_or_else(|_| "".to_owned())
    }

    pub fn new(
        env: HashMap<String, String>,
        shadowenv_data: undo::Data,
        target_hash: u64,
        clobber: bool,
    ) -> Shadowenv {
        let (unshadowed_env, no_clobber, prev_dirs) =
            Shadowenv::unshadow(&env, shadowenv_data, clobber);

        Shadowenv {
            env: unshadowed_env.clone(),
            unshadowed_env,
            no_clobber,
            initial_env: env,
            lists: HashSet::new(),
            placed: HashMap::new(),
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
    ) -> (HashMap<String, String>, HashSet<String>, HashSet<PathBuf>) {
        let mut result = env.clone();
        let mut no_clobber = HashSet::new();
        for scalar in shadowenv_data.scalars {
            if !clobber && scalar.no_clobber {
                no_clobber.insert(scalar.name);
                continue;
            }

            let current_value = env_get(&result, scalar.name.clone());
            if current_value == scalar.current {
                env_set(&mut result, scalar.name, scalar.original);
            } else if !clobber {
                no_clobber.insert(scalar.name);
            }
        }
        // TODO: no_clobber for lists
        for list in shadowenv_data.lists {
            let current = env_get(&result, list.name.clone());
            match list.exact {
                // Nothing has touched the variable since activation: restore it exactly.
                Some(exact) if current == exact.current => {
                    env_set(&mut result, list.name, exact.original);
                }
                // It changed since: undo shadowenv's changes, keep the later ones, and keep the
                // order of both.
                Some(exact) => {
                    let parts = |v: &Option<String>| -> Vec<String> {
                        v.as_deref()
                            .map_or_else(Vec::new, |s| s.split(':').map(String::from).collect())
                    };
                    let (original, activated, now) = (
                        parts(&exact.original),
                        parts(&exact.current),
                        parts(&current),
                    );
                    let rebased = rebase_pathlist(&original, &activated, &now, &exact.placed)
                        .unwrap_or_else(|| {
                            undo_pathlist_edits(&now, &list.additions, &list.deletions)
                        });
                    let value = (!rebased.is_empty()).then(|| rebased.join(":"));
                    env_set(&mut result, list.name, value);
                }
                // The data predates `exact`: undo shadowenv's additions and deletions as sets.
                None => {
                    for addition in list.additions {
                        env_remove_from_pathlist(&mut result, list.name.clone(), addition);
                    }
                    for deletion in list.deletions {
                        env_insert_at_front_of_pathlist(&mut result, list.name.clone(), deletion);
                    }
                }
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
        let mut exact_budget = MAX_EXACT_BYTES;

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
                let present: HashSet<&str> = final_parts.iter().copied().collect();
                let placed = self.placed.get(&varname).map_or_else(Vec::new, |placed| {
                    placed
                        .iter()
                        .filter(|entry| present.contains(entry.as_str()))
                        .cloned()
                        .collect()
                });
                let (additions, deletions) = diff_vecs(unshadowed_parts, final_parts);
                let exact = undo::ListValues {
                    original: self.unshadowed_env.get(&varname).cloned(),
                    current: final_value.clone(),
                    placed,
                };
                let cost = exact.original.as_ref().map_or(0, String::len)
                    + exact.current.as_ref().map_or(0, String::len)
                    + exact.placed.iter().map(String::len).sum::<usize>();
                let exact = (cost <= exact_budget).then(|| {
                    exact_budget -= cost;
                    exact
                });
                data.add_list(varname, additions, deletions, exact);
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

        data
    }

    fn format_shadowenv_data(&self) -> Result<String, Error> {
        let d = self.shadowenv_data();
        Ok(format!("{:016x}:", self.target_hash,) + &serde_json::to_string(&d)?)
    }

    pub fn exports(&self) -> Result<HashMap<String, Option<String>>, Error> {
        let mut changes: HashMap<String, Option<String>> = HashMap::new();
        let varnames = self.all_relevant_varnames();

        changes.insert(
            SHADOWENV_DATA.to_string(),
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

    pub fn set(&mut self, a: &str, b: Option<&str>) -> Result<(), VariableError> {
        validate_assignable_name(a)?;
        if let Some(value) = b {
            validate_var_value(a, value)?;
        }
        env_set(&mut self.env, a.to_string(), b.map(|s| s.to_string()));
        // The whole value is the program's now; nothing in it was prepended or appended.
        self.placed.remove(a);
        Ok(())
    }

    pub fn get(&self, a: &str) -> Option<String> {
        env_get(&self.env, a.to_string())
    }

    /// Length of the variable's current value, or 0 if it is unset.
    pub fn value_len(&self, a: &str) -> usize {
        self.env.get(a).map_or(0, String::len)
    }

    pub fn remove_from_pathlist(&mut self, a: &str, b: &str) -> Result<(), VariableError> {
        self.edit_pathlist(a, b, env_remove_from_pathlist)
    }

    pub fn remove_from_pathlist_containing(
        &mut self,
        a: &str,
        b: &str,
    ) -> Result<(), VariableError> {
        self.edit_pathlist(a, b, env_remove_from_pathlist_containing)
    }

    pub fn append_to_pathlist(&mut self, a: &str, b: &str) -> Result<(), VariableError> {
        self.edit_pathlist(a, b, env_append_to_pathlist)?;
        self.place(a, b);
        Ok(())
    }

    pub fn prepend_to_pathlist(&mut self, a: &str, b: &str) -> Result<(), VariableError> {
        self.edit_pathlist(a, b, env_prepend_to_pathlist)?;
        self.place(a, b);
        Ok(())
    }

    fn place(&mut self, a: &str, b: &str) {
        self.placed
            .entry(a.to_string())
            .or_default()
            .insert(b.to_string());
    }

    /// Applies `edit` to pathlist `a`, leaving the variable unchanged if the entry or the
    /// resulting value is invalid.
    fn edit_pathlist(
        &mut self,
        a: &str,
        b: &str,
        edit: fn(&mut HashMap<String, String>, String, String),
    ) -> Result<(), VariableError> {
        validate_assignable_name(a)?;
        validate_var_value(a, b)?;
        self.inform_list(a);
        let before = self.env.get(a).cloned();
        edit(&mut self.env, a.to_string(), b.to_string());
        if let Some(after) = self.env.get(a) {
            if let Err(err) = validate_var_value(a, after) {
                env_set(&mut self.env, a.to_string(), before);
                return Err(err.into());
            }
        }
        // Forget placed entries the edit removed.
        if let Some(placed) = self.placed.get_mut(a) {
            let now: HashSet<&str> = self
                .env
                .get(a)
                .map_or_else(HashSet::new, |v| v.split(':').collect());
            placed.retain(|entry| now.contains(entry.as_str()));
        }
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

    /// Every variable that is set now, was set when shadowenv started, or is set in the
    /// environment with the previous activation undone. Without the last, a variable that a
    /// program unsets would drop out of the undo data on the next activation and never be
    /// restored.
    fn all_relevant_varnames(&self) -> HashSet<String> {
        let mut keys: HashSet<String> = self.env.keys().map(String::from).collect();
        keys.extend(self.initial_env.keys().map(String::from));
        keys.extend(self.unshadowed_env.keys().map(String::from));
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

/// `env/append-to-pathlist`: `b` becomes the last entry of `a`, and its only copy.
fn env_append_to_pathlist(env: &mut HashMap<String, String>, a: String, b: String) {
    env_place_in_pathlist(env, a, b, false);
}

/// `env/prepend-to-pathlist`: `b` becomes the first entry of `a`, and its only copy.
fn env_prepend_to_pathlist(env: &mut HashMap<String, String>, a: String, b: String) {
    env_place_in_pathlist(env, a, b, true);
}

/// Removes every copy of `b` from pathlist `a`, then adds it at the front or the end, as the
/// docs describe for `env/prepend-to-pathlist` and `env/append-to-pathlist`.
fn env_place_in_pathlist(env: &mut HashMap<String, String>, a: String, b: String, front: bool) {
    let mut items = match env.get(&a) {
        Some(existing) => existing
            .split(':')
            .filter(|x| *x != b)
            .collect::<Vec<&str>>(),
        None => vec![],
    };
    if front {
        items.insert(0, &b);
    } else {
        items.push(&b);
    }
    let next = items.join(":");
    env.insert(a, next);
}

/// Adds `b` at the front of pathlist `a` even if it's already present. Only for undoing data
/// written by versions that recorded pathlist changes as bare additions and deletions.
fn env_insert_at_front_of_pathlist(env: &mut HashMap<String, String>, a: String, b: String) {
    let mut items = match env.get(&a) {
        Some(existing) => existing.split(':').collect::<Vec<&str>>(),
        None => vec![],
    };
    items.insert(0, &b);
    let next = items.join(":");
    env.insert(a, next);
}

/// The pathlist edits shadowenv made, undone without position information: additions removed
/// at their first occurrence, deletions put back at the front. Only used when the lists are too
/// long to rebase.
fn undo_pathlist_edits(now: &[String], additions: &[String], deletions: &[String]) -> Vec<String> {
    let mut out = now.to_vec();
    for addition in additions {
        if let Some(i) = out.iter().position(|x| x == addition) {
            out.remove(i);
        }
    }
    for deletion in deletions {
        out.insert(0, deletion.clone());
    }
    out
}

/// Most entries compared when rebasing a pathlist; past this, rebasing would cost too much time
/// per prompt and `undo_pathlist_edits` is used instead.
const MAX_REBASE_COMPARISONS: usize = 1 << 20;

/// Undoes an activation's changes to a pathlist that has also changed since, keeping the later
/// changes and everyone's order. `original` and `activated` are the list before and right after
/// activation, `now` its value since, and `placed` the entries the activation prepended or
/// appended. Entries the activation added or placed are dropped from wherever they still are;
/// entries it removed or placed are put back just before the first entry that followed them
/// originally and is still there, or at the end if none is. People and tools prepend, so
/// whatever they added since belongs ahead of the restored entry, not behind it; everything
/// else stays as it is now.
///
/// Returns `None` if the lists are too long to compare cheaply.
fn rebase_pathlist(
    original: &[String],
    activated: &[String],
    now: &[String],
    placed: &[String],
) -> Option<Vec<String>> {
    if original.len() * activated.len() > MAX_REBASE_COMPARISONS
        || activated.len() * now.len() > MAX_REBASE_COMPARISONS
    {
        return None;
    }
    // Which original entry each activated entry is. A placed entry is the activation's own, even
    // if the same entry was there before: its old copy counts as removed. Only the rest align.
    let placed: HashSet<&str> = placed.iter().map(String::as_str).collect();
    let unplaced: Vec<usize> = (0..activated.len())
        .filter(|&a| !placed.contains(activated[a].as_str()))
        .collect();
    let unplaced_values: Vec<String> = unplaced.iter().map(|&a| activated[a].clone()).collect();
    let mut original_of = vec![None; activated.len()];
    for (k, o) in lcs_alignment(&unplaced_values, original)
        .into_iter()
        .enumerate()
    {
        original_of[unplaced[k]] = o;
    }
    // Which current entry each activated one is.
    let now_of = lcs_alignment(activated, now);
    let mut activated_of_now = vec![None; now.len()];
    for (a, n) in now_of.iter().enumerate() {
        if let Some(n) = n {
            activated_of_now[*n] = Some(a);
        }
    }
    let mut now_of_original = vec![None; original.len()];
    for (a, o) in original_of.iter().enumerate() {
        if let (Some(o), Some(n)) = (o, now_of[a]) {
            now_of_original[*o] = Some(n);
        }
    }

    // The activation's additions that are gone now: something removed them since.
    let mut removed_since: Vec<&String> = (0..activated.len())
        .filter(|&a| original_of[a].is_none() && now_of[a].is_none())
        .map(|a| &activated[a])
        .collect();

    // Deleted entries, keyed by the current entry they go after (None: the very front).
    let mut after: HashMap<Option<usize>, Vec<&String>> = HashMap::new();
    let removed = (0..original.len()).filter(|&o| !original_of.contains(&Some(o)));
    for o in removed {
        // The activation moved this entry (removed it here, added it elsewhere), and the move's
        // new copy was removed since: the entry is meant to be gone, not back where it was.
        if let Some(i) = removed_since.iter().position(|v| **v == original[o]) {
            removed_since.swap_remove(i);
            continue;
        }
        let following = (o + 1..original.len()).find_map(|f| now_of_original[f]);
        let anchor = match following {
            Some(n) => n.checked_sub(1),
            None => now.len().checked_sub(1),
        };
        after.entry(anchor).or_default().push(&original[o]);
    }

    let mut out: Vec<String> = after
        .remove(&None)
        .unwrap_or_default()
        .into_iter()
        .cloned()
        .collect();
    for (n, entry) in now.iter().enumerate() {
        // Drop entries the activation added, wherever they ended up.
        let added = activated_of_now[n].is_some_and(|a| original_of[a].is_none());
        if !added {
            out.push(entry.clone());
        }
        out.extend(
            after
                .remove(&Some(n))
                .unwrap_or_default()
                .into_iter()
                .cloned(),
        );
    }
    Some(out)
}

/// For each element of `a`, the index of the element of `b` it is paired with in a longest
/// common subsequence of the two, if any. Where an entry repeats, it pairs with the latest
/// occurrence it can: people and programs mostly prepend, so the earlier copy is the newer one.
fn lcs_alignment(a: &[String], b: &[String]) -> Vec<Option<usize>> {
    let (n, m) = (a.len(), b.len());
    // len[at(i, j)]: length of the longest common subsequence of a[..i] and b[..j].
    let mut len = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in 1..=n {
        for j in 1..=m {
            len[at(i, j)] = if a[i - 1] == b[j - 1] {
                len[at(i - 1, j - 1)] + 1
            } else {
                len[at(i - 1, j)].max(len[at(i, j - 1)])
            };
        }
    }
    let mut pairs = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            pairs[i - 1] = Some(j - 1);
            i -= 1;
            j -= 1;
        } else if len[at(i - 1, j)] >= len[at(i, j - 1)] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs
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
    use crate::undo::{Data, List, ListValues, Scalar};
    use std::collections::HashMap;

    fn build_shadow_env(env_variables: Vec<(&str, &str)>, data: Data) -> Shadowenv {
        let env = env_variables
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>();
        Shadowenv::new(env, data, 123456789, false)
    }

    #[test]
    fn test_prepend_and_append_move_an_entry_that_is_already_present() {
        let mut shadowenv = build_shadow_env(
            vec![("PATH", "/a:/b:/c:/b"), ("EMPTY", "")],
            Default::default(),
        );
        shadowenv.prepend_to_pathlist("PATH", "/c").unwrap();
        assert_eq!(shadowenv.get("PATH").as_deref(), Some("/c:/a:/b:/b"));
        // Every existing copy goes: the entry ends up exactly once.
        shadowenv.append_to_pathlist("PATH", "/b").unwrap();
        assert_eq!(shadowenv.get("PATH").as_deref(), Some("/c:/a:/b"));
        shadowenv.prepend_to_pathlist("PATH", "/new").unwrap();
        assert_eq!(shadowenv.get("PATH").as_deref(), Some("/new:/c:/a:/b"));
        shadowenv.append_to_pathlist("UNSET", "/x").unwrap();
        assert_eq!(shadowenv.get("UNSET").as_deref(), Some("/x"));
    }

    #[test]
    fn test_undo_data_stays_small_enough_to_export() {
        // `exact` copies whole pathlists into __shadowenv_data, itself an environment variable
        // that every exec must fit under MAX_ENTRY_BYTES. A huge PATH keeps the old, smaller
        // record instead; a normal one keeps `exact`.
        let huge = (0..2000)
            .map(|i| format!("/nix/store/{i:032}-pkg/bin"))
            .collect::<Vec<_>>()
            .join(":");
        let mut shadowenv = build_shadow_env(
            vec![("PATH", &huge), ("MANPATH", "/usr/share/man")],
            Default::default(),
        );
        shadowenv
            .prepend_to_pathlist("PATH", "/opt/project/bin")
            .unwrap();
        shadowenv
            .prepend_to_pathlist("MANPATH", "/opt/project/man")
            .unwrap();
        let data = shadowenv.shadowenv_data();
        let exact = |name: &str| {
            data.lists
                .iter()
                .find(|l| l.name == name)
                .unwrap()
                .exact
                .is_some()
        };
        assert!(!exact("PATH"));
        assert!(exact("MANPATH"));
        assert!(shadowenv.format_shadowenv_data().unwrap().len() < MAX_EXACT_BYTES);
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

    fn list(s: &str) -> Vec<String> {
        s.split(':')
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect()
    }

    /// Each case: the list before activation, right after it, the entries the activation
    /// prepended or appended, and the list after the user edited it while active; then what
    /// deactivating must leave. All came from the shell-session simulation (src/simulation.rs),
    /// minimized.
    #[test]
    fn test_rebase_pathlist_undoes_activation_but_keeps_later_edits_in_order() {
        for (original, activated, placed, now, expected) in [
            // Activation removed /usr/bin; the user prepended. /usr/bin returns to its old place,
            // not the front, where it would shadow /usr/local/bin.
            (
                "/usr/local/bin:/usr/bin:/bin",
                "/usr/local/bin:/bin",
                "",
                "/opt/b:/usr/local/bin:/bin",
                "/opt/b:/usr/local/bin:/usr/bin:/bin",
            ),
            // Activation removed the first entry; it returns before its old successor.
            (
                "/usr/local/bin:/usr/bin:/bin",
                "/usr/bin:/bin",
                "",
                "/opt/c:/usr/bin:/bin",
                "/opt/c:/usr/local/bin:/usr/bin:/bin",
            ),
            // The user prepended an entry the activation had also added: the user's copy stays.
            (
                "",
                "/opt/c",
                "/opt/c",
                "/opt/c:/opt/a:/opt/c",
                "/opt/c:/opt/a",
            ),
            (
                "/man",
                "/cargo:/man",
                "/cargo",
                "/cargo:/local:/cargo:/man",
                "/cargo:/local:/man",
            ),
            // The user duplicated an entry around one the activation removed.
            ("/l:/u:/b", "/l:/b", "", "/l:/l:/b", "/l:/l:/u:/b"),
            // The user removed something the activation added: nothing to undo for it.
            ("/a:/b", "/x:/a:/b", "/x", "/a:/b", "/a:/b"),
            // The user removed an original entry: it stays removed.
            ("/a:/b:/c", "/x:/a:/b:/c", "/x", "/x:/a:/c", "/a:/c"),
            // Nothing of the original list remains to anchor to: the removed entry goes behind
            // what the user prepended since.
            ("/nix", "", "", "/opt/c", "/opt/c:/nix"),
            // The activation moved an entry (by appending it where it already was); the user
            // removed it since. It stays removed.
            ("/usr/bin:/man", "/man:/usr/bin", "/usr/bin", "/man", "/man"),
            ("/l:/u:/b", "/b:/u", "/u", "/b", "/l:/b"),
            // The activation moved an entry and the user left it alone: it goes back.
            (
                "/usr/bin:/man",
                "/man:/usr/bin",
                "/usr/bin",
                "/x:/man:/usr/bin",
                "/x:/usr/bin:/man",
            ),
            // Moving /u to the front looks the same as moving /l after it; knowing which entry
            // the activation placed tells them apart.
            ("/l:/u:/b", "/u:/l:/b", "/u", "/o:/u:/l:/b", "/o:/l:/u:/b"),
            ("/l:/u:/b", "/l:/b:/u", "/u", "/o:/l:/b:/u", "/o:/l:/u:/b"),
        ] {
            let rebased =
                rebase_pathlist(&list(original), &list(activated), &list(now), &list(placed))
                    .unwrap();
            assert_eq!(
                rebased.join(":"),
                expected,
                "{original} → {activated} (placed {placed}) → {now}"
            );
        }
    }

    #[test]
    fn test_deactivation_restores_a_variable_unset_by_nested_activations() {
        // Activate a program that unsets X, then switch to another activation (e.g. a nested
        // .shadowenv.d) that also unsets it. X must still be restored on leaving both.
        let mut first = build_shadow_env(vec![("X", "x0")], Default::default());
        first.set("X", None).unwrap();
        let env: HashMap<String, String> = HashMap::new();
        let mut second = Shadowenv::new(env.clone(), first.shadowenv_data(), 1, false);
        second.set("X", None).unwrap();
        let left = Shadowenv::new(env, second.shadowenv_data(), 0, false);
        assert_eq!(
            left.exports().unwrap().get("X"),
            Some(&Some("x0".to_string()))
        );
    }

    #[test]
    fn test_rejects_values_the_environment_cannot_hold() {
        let mut shadowenv = build_shadow_env(vec![("P", "/a")], Default::default());
        for bad in ["a\0b", "a\x1eb", "a\x1fb"] {
            assert!(shadowenv.set("A", Some(bad)).is_err(), "{bad:?}");
            assert!(shadowenv.prepend_to_pathlist("P", bad).is_err(), "{bad:?}");
        }
        // A name and value up to the OS's 128 KiB entry limit fit; one byte more doesn't.
        let longest = "x".repeat(MAX_ENTRY_BYTES - "A".len() - 2);
        shadowenv.set("A", Some(&longest)).unwrap();
        assert!(shadowenv.set("A", Some(&format!("{longest}x"))).is_err());
        assert_eq!(shadowenv.get("A").map(|v| v.len()), Some(longest.len()));
        assert_eq!(shadowenv.get("P").as_deref(), Some("/a"));
    }

    #[test]
    fn test_pathlist_edit_that_would_overflow_leaves_the_variable_unchanged() {
        let almost = "x".repeat(MAX_ENTRY_BYTES - "P".len() - 3);
        let mut shadowenv = build_shadow_env(vec![("P", &almost)], Default::default());
        assert!(shadowenv.prepend_to_pathlist("P", "/bin").is_err());
        assert_eq!(shadowenv.get("P").as_deref(), Some(almost.as_str()));
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
                exact: Some(ListValues {
                    original: Some("/path1:/path2".to_string()),
                    current: Some("/path4:/path2:/path3".to_string()),
                    placed: vec!["/path3".to_string(), "/path4".to_string()],
                }),
            }],
            prev_dirs: Default::default(),
        };

        let expected_formatted_data = r#"00000000075bcd15:{"scalars":[{"name":"VAR_A","original":"v0","current":"v2","no_clobber":false},{"name":"VAR_B","original":"v0","current":null,"no_clobber":false},{"name":"VAR_C","original":null,"current":"v3","no_clobber":false}],"lists":[{"name":"PATH","additions":["/path3","/path4"],"deletions":["/path1"],"exact":{"original":"/path1:/path2","current":"/path4:/path2:/path3","placed":["/path3","/path4"]}}],"prev_dirs":[]}"#;

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

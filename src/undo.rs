use anyhow::Error;
use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use serde_derive::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    result::Result,
};

#[derive(Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct Scalar {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub original: Option<String>,
    #[serde(default)]
    pub current: Option<String>,
    #[serde(default)]
    pub no_clobber: bool,
}

#[derive(Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct List {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub additions: Vec<String>,
    #[serde(default)]
    pub deletions: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct Data {
    #[serde(default)]
    pub scalars: Vec<Scalar>,
    #[serde(default)]
    pub lists: Vec<List>,
    #[serde(default)]
    pub prev_dirs: HashSet<PathBuf>,
    /// Hash of the managed variables' values right after activation, as 16 hex digits, so drift
    /// can be detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl Data {
    pub fn from_str(data: &str) -> Result<Data, Error> {
        let d: Data = serde_json::from_str(data)?;
        Ok(d)
    }

    pub fn new() -> Self {
        Data {
            scalars: vec![],
            lists: vec![],
            prev_dirs: HashSet::new(),
            fingerprint: None,
        }
    }

    pub fn fingerprint_of(&self, env: &HashMap<String, String>) -> String {
        let mut names: Vec<&str> = self
            .scalars
            .iter()
            .map(|scalar| scalar.name.as_str())
            .chain(self.lists.iter().map(|list| list.name.as_str()))
            .collect();
        names.sort_unstable();
        names.dedup();

        let mut hasher = Blake2bVar::new(8).expect("bad hasher output size");
        for name in names {
            hasher.update(name.as_bytes());
            match env.get(name) {
                Some(value) => {
                    hasher.update(b"\x01");
                    hasher.update(value.as_bytes());
                }
                None => hasher.update(b"\x02"),
            }
            hasher.update(b"\x00");
        }

        let mut buf = [0u8; 8];
        hasher.finalize_variable(&mut buf).unwrap();
        format!("{:016x}", u64::from_be_bytes(buf))
    }

    /// Whether a managed variable changed since activation, or a variable this data set is now
    /// unset (which a forced reapply records as an override). Data written before fingerprints
    /// existed counts as drifted, so the first repair records one.
    pub fn drifted(&self, env: &HashMap<String, String>) -> bool {
        self.fingerprint.as_deref() != Some(self.fingerprint_of(env).as_str())
            || self
                .scalars
                .iter()
                .any(|scalar| scalar.current.is_some() && !env.contains_key(&scalar.name))
    }

    pub fn add_scalar(
        &mut self,
        name: String,
        original: Option<String>,
        current: Option<String>,
        no_clobber: bool,
    ) {
        self.scalars.push(Scalar {
            name,
            original,
            current,
            no_clobber,
        })
    }

    pub fn add_list(&mut self, name: String, additions: Vec<String>, deletions: Vec<String>) {
        self.lists.push(List {
            name,
            additions,
            deletions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marker_without_a_fingerprint_parses_and_counts_as_drifted() {
        let data = Data::from_str(
            r#"{"scalars":[{"name":"A","original":null,"current":"B","no_clobber":false}],"lists":[],"prev_dirs":[]}"#,
        )
        .unwrap();

        assert_eq!(data.fingerprint, None);
        assert!(data.drifted(&HashMap::from([("A".to_string(), "B".to_string())])));
    }

    #[test]
    fn test_fingerprint_round_trips_as_a_hex_string() {
        let mut data = Data::new();
        data.add_scalar("A".to_string(), None, Some("B".to_string()), false);
        let env = HashMap::from([("A".to_string(), "B".to_string())]);
        data.fingerprint = Some(data.fingerprint_of(&env));

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains(&format!(r#""fingerprint":"{}""#, data.fingerprint_of(&env))));
        assert!(!Data::from_str(&json).unwrap().drifted(&env));
    }
}

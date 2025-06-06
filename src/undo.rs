use anyhow::Error;
use serde_derive::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf, result::Result};

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
        }
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

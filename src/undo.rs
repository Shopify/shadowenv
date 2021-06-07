use std::result::Result;

use failure::Error;
use serde_derive::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct Scalar {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub original: Option<String>,
    #[serde(default)]
    pub current: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct List {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub additions: Vec<String>,
    #[serde(default)]
    pub deletions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    pub scalars: Vec<Scalar>,
    #[serde(default)]
    pub lists: Vec<List>,
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
        }
    }

    pub fn add_scalar(&mut self, name: String, original: Option<String>, current: Option<String>) {
        self.scalars.push(Scalar {
            name: name,
            original: original,
            current: current,
        })
    }

    pub fn add_list(&mut self, name: String, additions: Vec<String>, deletions: Vec<String>) {
        self.lists.push(List {
            name: name,
            additions: additions,
            deletions: deletions,
        })
    }
}

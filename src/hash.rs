use std::error;
use std::error::Error;
use std::convert::TryInto;
use std::result::Result;
use std::str::FromStr;
use std::u64;

use blake2::digest::{Input, VariableOutput};
use blake2::VarBlake2b;

const FILE_SEPARATOR: &'static str = "\x1C";
const RECORD_SEPARATOR: &'static str = "\x1D";

#[derive(Debug)]
pub struct SourceFile {
    pub name: String,
    pub source: String,
}

#[derive(Debug)]
pub struct Source {
    pub files: Vec<SourceFile>,
}

#[derive(Debug, PartialEq)]
pub struct Hash {
    pub hash: u64,
}

impl Source {
    pub fn new() -> Self {
        Source{ files: vec![] }
    }

    pub fn add_file(&mut self, name: String, contents: String) -> Result<(), Box<Error>> {
        self.files.push(
            SourceFile { name: name.to_string(), source: contents.to_string() },
        );
        Ok(())
    }

    pub fn hash(&self) -> Result<u64, Box<Error>> {
        let mut hasher = VarBlake2b::new(8)?;
        if self.files.len() == 0 {
            return Ok(0);
        }
        for file in self.files.iter() {
            hasher.input(&file.name);
            hasher.input(RECORD_SEPARATOR);
            hasher.input(&file.source);
            hasher.input(FILE_SEPARATOR);
        }
        let mut sum: u64 = 0;
        hasher.variable_result(|res| {
            sum = u64::from_ne_bytes(res.try_into().unwrap());
        });
        Ok(sum)
    }
}

impl FromStr for Hash {
    type Err = Box<error::Error>;

    fn from_str(key: &str) -> Result<Self, Box<Error>> {
        if key.len() != 16 {
            return Err("wrong input size".to_string().into());
        }
        let hash = u64::from_str_radix(&key, 16)?;
        Ok(Hash{ hash: hash })
    }
}

impl ToString for Hash {
    fn to_string(&self) -> String {
        format!("{:016x}", self.hash)
    }
}

#[test]
fn test_key_encoding() {
    let key = Hash{ hash: 2, source: None };
    let hex = key.to_string();
    assert_eq!("0000000000000002", hex);
    let key2: Hash = Hash::from_str(&hex).unwrap();
    assert_eq!(key, key2);
}

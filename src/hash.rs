use blake2::digest::{Input, VariableOutput};
use blake2::VarBlake2b;
use failure::{Error, Fail};
use std::cmp::Ord;
use std::cmp::Ordering;
use std::convert::TryInto;
use std::result::Result;
use std::str::FromStr;
use std::u64;

const FILE_SEPARATOR: &str = "\x1C";
const GROUP_SEPARATOR: &str = "\x1D";

#[derive(Debug, Clone)]
pub struct Source {
    pub dir: String,
    pub files: Vec<SourceFile>,
}

#[derive(Debug, Clone, Eq)]
pub struct SourceFile {
    pub name: String,
    pub contents: String,
}

impl Ord for SourceFile {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for SourceFile {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for SourceFile {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.contents == other.contents
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Hash {
    pub hash: u64,
}

#[derive(Fail, Debug)]
#[fail(display = "wrong input size")]
struct WrongInputSize;

impl Source {
    pub fn new(dir: String) -> Self {
        Source { dir, files: vec![] }
    }

    pub fn add_file(&mut self, name: String, contents: String) {
        self.files.push(SourceFile { name, contents })
    }

    pub fn hash(&self) -> Result<u64, Error> {
        if self.files.is_empty() {
            return Ok(0);
        }
        let mut hasher = VarBlake2b::new(8)?;
        hasher.input(&self.dir);
        hasher.input(FILE_SEPARATOR);
        for file in self.files.iter() {
            hasher.input(&file.name);
            hasher.input(GROUP_SEPARATOR);
            hasher.input(&file.contents);
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
    type Err = Error;

    fn from_str(key: &str) -> Result<Self, Error> {
        if key.len() != 16 {
            return Err(WrongInputSize {}.into());
        }
        let hash = u64::from_str_radix(key, 16)?;
        Ok(Hash { hash })
    }
}

impl ToString for Hash {
    fn to_string(&self) -> String {
        format!("{:016x}", self.hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::Arbitrary;
    use quickcheck::Gen;
    use quickcheck_macros::quickcheck;
    #[test]
    fn test_key_encoding() {
        let key = Hash { hash: 2 };
        let hex = key.to_string();
        assert_eq!("0000000000000002", hex);
        let key2: Hash = Hash::from_str(&hex).unwrap();
        assert_eq!(key, key2);
    }

    impl Arbitrary for Source {
        fn arbitrary(g: &mut Gen) -> Source {
            Source {
                dir: Arbitrary::arbitrary(g),
                files: Arbitrary::arbitrary(g),
            }
        }
    }

    impl Arbitrary for SourceFile {
        fn arbitrary(g: &mut Gen) -> SourceFile {
            SourceFile {
                name: Arbitrary::arbitrary(g),
                contents: Arbitrary::arbitrary(g),
            }
        }
    }

    impl Arbitrary for Hash {
        fn arbitrary(g: &mut Gen) -> Hash {
            Hash {
                hash: Arbitrary::arbitrary(g),
            }
        }
    }

    #[quickcheck]
    fn hash_roundtrip(hash: Hash) -> bool {
        hash.hash == Hash::from_str(&hash.to_string()).unwrap().hash
    }

    #[quickcheck]
    fn source_hash_is_stable(source: Source) -> bool {
        let a = source.hash();
        let b = source.hash();

        (a.is_err() && b.is_err()) || (a.is_ok() && b.is_ok() && a.unwrap() == b.unwrap())
    }
}

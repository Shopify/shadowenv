use anyhow::Error;
use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use std::{
    cmp::{Ord, Ordering},
    collections::VecDeque,
    fmt::Display,
    path::PathBuf,
    result::Result,
    str::FromStr,
};

const FILE_SEPARATOR: &str = "\x1C";
const GROUP_SEPARATOR: &str = "\x1D";

#[derive(Debug, Clone)]
pub struct SourceList {
    sources: VecDeque<Source>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub dir: String,
    pub files: Vec<SourceFile>,
}

#[derive(Debug, Clone, Eq, PartialOrd, Ord)]
pub struct SourceFile {
    pub name: String,
    pub contents: String,
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

#[derive(Debug, thiserror::Error)]
#[error("wrong input size")]
struct WrongInputSize;

impl Source {
    pub fn new<S: Into<String>>(dir: S) -> Self {
        Source { dir: dir.into(), files: vec![] }
    }

    pub fn add_file<S: Into<String>>(&mut self, name: S, contents: S) {
        self.files.push(SourceFile { 
            name: name.into(), 
            contents: contents.into() 
        })
    }

    pub fn hash(&self) -> Option<u64> {
        if self.files.is_empty() {
            return None;
        }

        let mut hasher = Blake2bVar::new(8).expect("bad hasher output size");
        hasher.update(self.dir.as_bytes());
        hasher.update(FILE_SEPARATOR.as_bytes());

        for file in self.files.iter() {
            hasher.update(file.name.as_bytes());
            hasher.update(GROUP_SEPARATOR.as_bytes());
            hasher.update(file.contents.as_bytes());
            hasher.update(FILE_SEPARATOR.as_bytes());
        }

        let mut buf = [0u8; 8];
        hasher.finalize_variable(&mut buf).unwrap();

        Some(u64::from_ne_bytes(buf))
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

impl Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016x}", self.hash)
    }
}

impl SourceList {
    pub fn new() -> Self {
        SourceList {
            sources: VecDeque::new(),
        }
    }

    #[cfg(test)]
    pub fn new_with_sources(sources: Vec<Source>) -> Self {
        SourceList {
            sources: sources.into(),
        }
    }

    pub fn prepend_source(&mut self, source: Source) {
        self.sources.push_front(source);
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn hash(&self) -> Option<u64> {
        if self.sources.iter().any(|source| source.hash().is_none()) {
            return None;
        }

        let hashes: Vec<u64> = self
            .sources
            .iter()
            .map(|source| source.hash().unwrap())
            .collect();

        let mut hasher = Blake2bVar::new(8).expect("bad hasher output size");
        for hash in hashes {
            hasher.update(&hash.to_ne_bytes());
            hasher.update(FILE_SEPARATOR.as_bytes());
        }

        let mut buf = [0u8; 8];
        hasher.finalize_variable(&mut buf).unwrap();

        Some(u64::from_ne_bytes(buf))
    }

    pub fn consume(self) -> Vec<Source> {
        self.sources.into()
    }

    pub fn shortened_dirs(&self) -> Vec<PathBuf> {
        let dirs: Vec<PathBuf> = self
            .sources
            .iter()
            .map(|source| source.dir.parse().expect("dir not a valid path"))
            .collect();

        if dirs.is_empty() {
            return dirs;
        }

        let highest_dir = dirs.first().unwrap();
        let depth = highest_dir.components().count() - 1;

        dirs.iter()
            .map(|dir| {
                dir.components()
                    .skip(depth)
                    .map(|comp| comp.as_os_str())
                    .fold(PathBuf::new(), |acc, comp| acc.join(comp))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::Arbitrary;
    use quickcheck::Gen;
    use quickcheck_macros::quickcheck;

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

    #[test]
    fn test_key_encoding() {
        let key = Hash { hash: 2 };
        let hex = key.to_string();
        assert_eq!("0000000000000002", hex);
        let key2: Hash = Hash::from_str(&hex).unwrap();
        assert_eq!(key, key2);
    }

    #[quickcheck]
    fn hash_roundtrip(hash: Hash) -> bool {
        hash.hash == Hash::from_str(&hash.to_string()).unwrap().hash
    }

    #[quickcheck]
    fn source_hash_is_stable(source: Source) -> bool {
        let a = source.hash();
        let b = source.hash();

        (a.is_none() && b.is_none()) || (a.is_some() && b.is_some() && a.unwrap() == b.unwrap())
    }
}

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

#[derive(Debug, thiserror::Error)]
#[error("wrong input size")]
struct WrongInputSize;

impl Source {
    pub fn new(dir: String) -> Self {
        Source { dir, files: vec![] }
    }

    pub fn add_file(&mut self, name: String, contents: String) {
        self.files.push(SourceFile { name, contents })
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
        if self.sources.is_empty() || self.sources.iter().any(|source| source.hash().is_none()) {
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

        (a.is_none() && b.is_none()) || (a.is_some() && b.is_some() && a.unwrap() == b.unwrap())
    }

    #[test]
    fn test_empty_source_list() {
        let list = SourceList::new();
        assert!(list.is_empty());
        assert_eq!(list.hash(), None);
        assert!(list.shortened_dirs().is_empty());
    }

    #[test]
    fn test_source_list_ordering() {
        let mut source1 = Source::new("dir1".to_string());
        source1.add_file("file1.txt".to_string(), "content1".to_string());
        
        let mut source2 = Source::new("dir2".to_string());
        source2.add_file("file2.txt".to_string(), "content2".to_string());

        let list = SourceList::new_with_sources(vec![source1.clone(), source2.clone()]);
        let consumed = list.consume();
        assert_eq!(consumed.len(), 2);
        assert_eq!(consumed[0].dir, "dir1");
        assert_eq!(consumed[1].dir, "dir2");
    }

    #[test]
    fn test_path_shortening() {
        let mut list = SourceList::new();
        let source1 = Source::new("/home/user/project/src/dir1".to_string());
        let source2 = Source::new("/home/user/project/src/dir2".to_string());
        list.prepend_source(source2);
        list.prepend_source(source1);

        let shortened = list.shortened_dirs();
        assert_eq!(shortened.len(), 2);
        assert_eq!(shortened[0].to_str().unwrap(), "dir1");
        assert_eq!(shortened[1].to_str().unwrap(), "dir2");
    }

    #[test]
    fn test_source_file_ordering() {
        let mut source = Source::new("test_dir".to_string());
        source.add_file("b.txt".to_string(), "content b".to_string());
        source.add_file("a.txt".to_string(), "content a".to_string());
        source.add_file("c.txt".to_string(), "content c".to_string());

        // Files should maintain insertion order
        assert_eq!(source.files[0].name, "b.txt");
        assert_eq!(source.files[1].name, "a.txt");
        assert_eq!(source.files[2].name, "c.txt");
    }

    #[test]
    fn test_source_list_prepend() {
        let mut list = SourceList::new();
        
        let mut source1 = Source::new("dir1".to_string());
        source1.add_file("file1.txt".to_string(), "content1".to_string());
        
        let mut source2 = Source::new("dir2".to_string());
        source2.add_file("file2.txt".to_string(), "content2".to_string());
        
        let mut source3 = Source::new("dir3".to_string());
        source3.add_file("file3.txt".to_string(), "content3".to_string());

        list.prepend_source(source1);
        list.prepend_source(source2);
        list.prepend_source(source3);

        let consumed = list.consume();
        assert_eq!(consumed.len(), 3);
        assert_eq!(consumed[0].dir, "dir3"); // Last prepended = first
        assert_eq!(consumed[1].dir, "dir2");
        assert_eq!(consumed[2].dir, "dir1"); // First prepended = last
    }

    #[test]
    fn test_hash_with_empty_files() {
        let mut source = Source::new("test_dir".to_string());
        source.add_file("empty.txt".to_string(), "".to_string());
        
        // A source with empty files should still have a valid hash
        assert!(source.hash().is_some());

        let list = SourceList::new_with_sources(vec![source]);
        assert!(list.hash().is_some());
    }

    #[test]
    fn test_mixed_source_hashing() {
        let empty_source = Source::new("empty_dir".to_string());
        let mut normal_source = Source::new("normal_dir".to_string());
        normal_source.add_file("file.txt".to_string(), "content".to_string());

        let list = SourceList::new_with_sources(vec![empty_source, normal_source]);
        
        // If any source has no files (thus no hash), the entire list should have no hash
        assert_eq!(list.hash(), None);
    }
}

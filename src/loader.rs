use crate::hash::Source;

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use failure::Error;

pub const DEFAULT_RELATIVE_COMPONENT: &str = ".shadowenv.d";

/// Search upwards the filesystem branch starting with `at` and then its ancestors looking
/// for a file or directory named `relative_component`.
pub fn find_root(at: &PathBuf, relative_component: &str) -> Result<Option<PathBuf>, Error> {
    for curr in at.ancestors() {
        let dirpath = curr.join(relative_component);

        match fs::read_dir(&dirpath) {
            Ok(_) => return Ok(Some(fs::canonicalize(dirpath)?)),
            Err(ref e) if e.kind() == ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(None)
}

/// Load all .lisp files in the directory pointed by `dirpath` storing their names and contents as
/// `SourceFiles` inside a `Source` struct.
///
/// Note that this function assumes that the dirpath is trusted.
pub fn load(dirpath: PathBuf) -> Result<Option<Source>, Error> {
    let mut source = Source::new(dirpath.parent().unwrap().to_string_lossy().to_string());

    for entry in fs::read_dir(dirpath)?.flatten() {
        let path = entry.path();
        if path.is_file() {
            // TODO: there HAS to be a better way to do this.
            let basename = path.file_name().unwrap().to_string_lossy().to_string();
            if !basename.ends_with(".lisp") {
                continue;
            }
            let contents = fs::read_to_string(&path)?;
            source.add_file(basename, contents);
        }
    }

    if source.files.is_empty() {
        return Ok(None);
    }
    Ok(Some(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::SourceFile;

    #[test]
    fn test_load() {
        let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", "simple"]
            .iter()
            .collect();
        let res = load(path);
        let source = res.unwrap().unwrap();
        assert_eq!(source.files.len(), 2, "it should contain 2 files");
        let mut files = source.files.clone();
        files.sort_by(|a, b| a.name.cmp(&b.name));

        let expected = vec![
            SourceFile {
                name: "550_dev_ruby.lisp".to_string(),
                contents: r#"(provide "ruby" "3.1.2")
"#
                .to_string(),
            },
            SourceFile {
                name: "585_dev_rust.lisp".to_string(),
                contents: r#"(provide "rust" "stable")
"#
                .to_string(),
            },
        ];

        assert_eq!(files, expected)
    }
}

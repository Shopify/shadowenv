use crate::hash::Source;
use anyhow::Error;
use std::{
    borrow::Cow,
    env, fs, io, iter,
    path::{Path, PathBuf},
};

pub const SHADOWENV_DIR_NAME: &str = ".shadowenv.d";
pub const SHADOWENV_PARENT_LINK_NAME: &str = "parent";

#[derive(thiserror::Error, Debug)]
pub enum TraversalError {
    /// General resolver error.
    #[error("Error for parent file at {}: {}", parent_link_path, error)]
    ResolveError {
        parent_link_path: String,
        error: String,
    },

    #[error(
        "Parent symlink at {} must link to a {} directory, but links to {}",
        path_to_parent_link,
        SHADOWENV_DIR_NAME,
        parent_link_target
    )]
    InvalidLinkTarget {
        path_to_parent_link: String,
        parent_link_target: String,
    },

    #[error(
        "Parent symlink {} must be an ancestor of {}",
        parent_link_target,
        shadowenv_path
    )]
    NotAnAncestor {
        parent_link_target: String,
        shadowenv_path: String,
    },

    #[error("Shadow env at {} references itself", shadowenv_path)]
    SelfReferential { shadowenv_path: String },

    #[error(transparent)]
    IoError(#[from] io::Error),
}

/// Search upwards the filesystem starting at `at` to find the closest shadowenv,
/// then traverse any parent symlinks if they exist (.shadowenv.d/parent).
///
/// This call validates that all parents found satisfy the following:
/// - Each parent must be an ancestor of the previous parent. Parent links can't
///   therefore point to adjacent or sub folders.
/// - Each parent must point to a `.shadowenv.d` basename folder.
///
/// This call does _not_ verify that any of the found shadowenvs is trusted.
/// See [crate::trust::ensure_dir_tree_trusted] for checking trust.
pub fn find_shadowenv_paths(at: &Path) -> Result<Vec<PathBuf>, TraversalError> {
    // First find the closest shadowenv.
    let closest = match closest_shadowenv(at)? {
        Some(closest) => closest,
        None => return Ok(vec![]),
    };

    // Then find all parents recursively. Any validation errors bubble up.
    let parents = resolve_shadowenv_parents(&closest)?;

    Ok(iter::once(closest).chain(parents).collect())
}

/// Attempts to find the closest shadowenv folder to `at`.
/// Returns a canonicalized path to the found shadowenv folder.
fn closest_shadowenv(at: &Path) -> Result<Option<PathBuf>, TraversalError> {
    for ancestor in at.ancestors() {
        let dirpath = ancestor.join(SHADOWENV_DIR_NAME);
        let metadata = match fs::metadata(&dirpath) {
            Ok(metadata) => metadata,
            Err(_) => continue, // Doens't exist or lacking permissions.
        };

        if metadata.is_dir() {
            return Ok(Some(fs::canonicalize(dirpath)?));
        }
    }

    Ok(None)
}

/// Recursively resolves parents.
/// Expects `from_shadowenv` to be canonicalized.
fn resolve_shadowenv_parents(from_shadowenv: &PathBuf) -> Result<Vec<PathBuf>, TraversalError> {
    let parent_link = from_shadowenv.join(SHADOWENV_PARENT_LINK_NAME);

    // `symlink_metadata`, opposed to `metadata`, doesn't resolve symlinks, which means that we can catch
    // the case where symlink points to an non-existant file later on for a more precise error.
    let metadata = match fs::symlink_metadata(&parent_link) {
        Ok(metadata) => metadata,
        // Symlink file itself doesn't exist or process is lacking access permissions.
        Err(_) => return Ok(vec![]),
    };

    let previous_working_dir = env::current_dir().unwrap();
    env::set_current_dir(from_shadowenv).unwrap();

    // Must be a valid symlink.
    // We need to resolve the symlink in context of the .shadowenv.d folder it is in.
    let resolved_parent = fs::read_link(&parent_link)
        .and_then(|resolved| resolved.canonicalize())
        .map_err(|err| {
            env::set_current_dir(&previous_working_dir).unwrap();

            TraversalError::ResolveError {
                parent_link_path: parent_link.to_string_lossy().to_string(),
                error: if metadata.is_symlink() {
                    err.to_string()
                } else {
                    "Not a symlink".to_owned()
                },
            }
        })?;

    // Restore working directory.
    env::set_current_dir(previous_working_dir).unwrap();

    // TODO: Refactor into better structure with the options.
    let base_name = resolved_parent.file_name();
    let base_name_stringified = base_name.map(|f| f.to_string_lossy());

    // Must point to a SHADOWENV_DIR_NAME (e.g. `.shadowenv.d`).
    if base_name_stringified != Some(Cow::Borrowed(SHADOWENV_DIR_NAME)) {
        return Err(TraversalError::InvalidLinkTarget {
            path_to_parent_link: parent_link.to_string_lossy().to_string(),
            parent_link_target: base_name_stringified.unwrap().to_string(),
        });
    }

    // Must not be self-referential.
    if from_shadowenv == &resolved_parent {
        return Err(TraversalError::SelfReferential {
            shadowenv_path: from_shadowenv.to_string_lossy().to_string(),
        });
    }

    // Must be an ancestor of the shadowenv we're coming from.
    // Unwrap is safe, we're always resolving to at least SHADOWENV_DIR_NAME.
    if !from_shadowenv.starts_with(&resolved_parent.parent().unwrap()) {
        return Err(TraversalError::NotAnAncestor {
            parent_link_target: resolved_parent.to_string_lossy().to_string(),
            shadowenv_path: from_shadowenv.to_string_lossy().to_string(),
        });
    }

    // Find parents of parent.
    let parents = resolve_shadowenv_parents(&resolved_parent)?;

    Ok(iter::once(resolved_parent).chain(parents).collect())
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
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

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

    #[test]
    fn closest_shadowenv_from_subfolder() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();
        let dir_one = base_path.join("dir1/.shadowenv.d");
        let dir_two = base_path.join("dir1/dir2/dir3");

        create_all(&[&dir_one, &dir_two]);

        let closest = closest_shadowenv(&dir_two).unwrap().unwrap();
        assert_eq!(closest, dir_one);
    }

    #[test]
    fn closest_shadowenv_from_inside_shadowenv() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();
        let shadowenv_path = base_path.join("dir1/.shadowenv.d");

        fs::create_dir_all(&shadowenv_path).unwrap();

        let closest = closest_shadowenv(&shadowenv_path).unwrap().unwrap();
        assert_eq!(closest, shadowenv_path);
    }

    #[test]
    fn find_shadowenv_paths_success() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();

        let shadowenv_one_path = base_path.join("dir1/.shadowenv.d");
        let shadowenv_two_path = base_path.join("dir1/dir2/dir3/.shadowenv.d");
        let shadowenv_three_path = base_path.join("dir1/dir2/dir3/dir4/.shadowenv.d");

        create_all(&[
            &shadowenv_one_path,
            &shadowenv_two_path,
            &shadowenv_three_path,
        ]);

        // Three -> two
        symlink(
            &shadowenv_two_path,
            &shadowenv_three_path.join(SHADOWENV_PARENT_LINK_NAME),
        )
        .unwrap();

        // Two -> one
        symlink(
            &shadowenv_one_path,
            &shadowenv_two_path.join(SHADOWENV_PARENT_LINK_NAME),
        )
        .unwrap();

        assert_eq!(
            find_shadowenv_paths(shadowenv_three_path.parent().unwrap()).unwrap(),
            [shadowenv_three_path, shadowenv_two_path, shadowenv_one_path]
        );
    }

    #[test]
    fn find_shadowenv_paths_ancestors_only() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();

        let shadowenv_one_path = base_path.join("dir1");
        let shadowenv_one_sibling_path = base_path.join("dir1_sibling/.shadowenv.d");
        let shadowenv_two_path = base_path.join("dir1/dir2/.shadowenv.d");

        create_all(&[
            &shadowenv_one_path,
            &shadowenv_one_sibling_path,
            &shadowenv_two_path,
        ]);

        // Two -> one sibling
        symlink(
            &shadowenv_one_sibling_path,
            &shadowenv_two_path.join(SHADOWENV_PARENT_LINK_NAME),
        )
        .unwrap();

        assert!(matches!(
            find_shadowenv_paths(shadowenv_two_path.parent().unwrap()).unwrap_err(),
            TraversalError::NotAnAncestor { .. }
        ));
    }

    #[test]
    fn find_shadowenv_paths_no_descending_links() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();

        let shadowenv_one_path = base_path.join("dir1/.shadowenv.d");
        let shadowenv_two_path = base_path.join("dir1/dir2/.shadowenv.d");

        create_all(&[&shadowenv_one_path, &shadowenv_two_path]);

        // Two -> one
        symlink(
            &shadowenv_one_path,
            &shadowenv_two_path.join(SHADOWENV_PARENT_LINK_NAME),
        )
        .unwrap();

        // One -> two
        symlink(
            &shadowenv_two_path,
            &shadowenv_one_path.join(SHADOWENV_PARENT_LINK_NAME),
        )
        .unwrap();

        assert!(matches!(
            find_shadowenv_paths(shadowenv_two_path.parent().unwrap()).unwrap_err(),
            TraversalError::NotAnAncestor { .. }
        ));
    }

    #[test]
    fn find_shadowenv_paths_invalid_parent() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();
        let shadowenv = base_path.join("top/sub/.shadowenv.d");

        create_all(&[&shadowenv]);

        // Shadowenv -> not a shadowenv
        symlink(
            &base_path.join("top"),
            &shadowenv.join(SHADOWENV_PARENT_LINK_NAME),
        )
        .unwrap();

        assert!(matches!(
            find_shadowenv_paths(shadowenv.parent().unwrap()).unwrap_err(),
            TraversalError::InvalidLinkTarget { .. }
        ));
    }

    #[test]
    fn find_shadowenv_paths_not_a_symlink() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();
        let shadowenv = base_path.join("dir/.shadowenv.d");
        let shadowenv_invalid_parent_file = base_path
            .join("dir/.shadowenv.d")
            .join(SHADOWENV_PARENT_LINK_NAME);

        create_all(&[&shadowenv, &shadowenv_invalid_parent_file]);
        assert!(
            match find_shadowenv_paths(shadowenv.parent().unwrap()).unwrap_err() {
                TraversalError::ResolveError {
                    parent_link_path,
                    error,
                } =>
                    parent_link_path == shadowenv_invalid_parent_file.to_string_lossy().to_string()
                        && error == "Not a symlink".to_owned(),
                _ => false,
            }
        );
    }

    #[test]
    fn find_shadowenv_paths_nonexistant_link_target() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();
        let shadowenv = base_path.join("dir/.shadowenv.d");
        let link_path = shadowenv.join(SHADOWENV_PARENT_LINK_NAME);

        create_all(&[&shadowenv]);

        // Shadowenv -> doesn't exist
        // Note: Rust doesn't allow creating links to nonexistant files, so we're using `Command`.
        let _ = std::process::Command::new("ln")
            .args([
                "-s",
                base_path.join("nonexistant").to_str().unwrap(),
                link_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();

        assert!(
            match find_shadowenv_paths(shadowenv.parent().unwrap()).unwrap_err() {
                TraversalError::ResolveError {
                    parent_link_path,
                    error,
                } =>
                    parent_link_path == link_path.to_string_lossy().to_string()
                        && error.contains("No such file or directory"),
                _ => false,
            }
        );
    }

    #[test]
    fn find_shadowenv_paths_disallow_self_reference() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();
        let shadowenv = base_path.join("dir/.shadowenv.d");
        let link_path = shadowenv.join(SHADOWENV_PARENT_LINK_NAME);

        create_all(&[&shadowenv]);

        // Shadowenv -> same shadowenv
        symlink(&shadowenv, &link_path).unwrap();

        assert!(matches!(
            find_shadowenv_paths(shadowenv.parent().unwrap()).unwrap_err(),
            TraversalError::SelfReferential { .. }
        ));
    }

    #[test]
    fn find_shadowenv_paths_using_relative_paths() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().canonicalize().unwrap();
        let shadowenv_one_path = base_path.join("dir1/.shadowenv.d");
        let shadowenv_two_path = base_path.join("dir1/dir2/.shadowenv.d");
        let shadowenv_three_path = base_path.join("dir1/dir2/dir3/.shadowenv.d");

        create_all(&[
            &shadowenv_one_path,
            &shadowenv_two_path,
            &shadowenv_three_path,
        ]);

        // Three -> one
        symlink(
            PathBuf::from("../../../.shadowenv.d"),
            shadowenv_three_path.join(SHADOWENV_PARENT_LINK_NAME),
        )
        .unwrap();

        // Two -> one
        symlink(
            PathBuf::from("../../.shadowenv.d"),
            shadowenv_two_path.join(SHADOWENV_PARENT_LINK_NAME),
        )
        .unwrap();

        assert_eq!(
            find_shadowenv_paths(shadowenv_three_path.parent().unwrap()).unwrap(),
            [shadowenv_three_path, shadowenv_one_path.clone()]
        );

        assert_eq!(
            find_shadowenv_paths(shadowenv_two_path.parent().unwrap()).unwrap(),
            [shadowenv_two_path, shadowenv_one_path]
        );
    }

    fn create_all(dirs: &[&PathBuf]) {
        for dir in dirs {
            fs::create_dir_all(dir).unwrap();
        }
    }
}

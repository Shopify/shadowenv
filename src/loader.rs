use crate::hash::Source;
use anyhow::Error;
use std::{
    borrow::Cow,
    fs, iter,
    path::{Path, PathBuf},
};

pub const SHADOWENV_DIR_NAME: &str = ".shadowenv.d";
pub const SHADOWENV_PARENT_LINK_NAME: &str = "parent";

#[derive(thiserror::Error, Debug)]
enum TraversalError {
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
pub fn find_shadowenv_paths(at: &Path) -> Result<Vec<PathBuf>, Error> {
    // First find the closest shadowenv.
    let closest = match closest_shadowenv(at)? {
        Some(closest) => closest,
        None => return Ok(vec![]),
    };

    // Then find all parents recursively. Any validation errors bubble up.
    let parents = resolve_shadowenv_parents(&closest)?;

    Ok(iter::once(closest).chain(parents).collect())
}

fn closest_shadowenv(at: &Path) -> Result<Option<PathBuf>, Error> {
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
    if let Err(_) = fs::metadata(&parent_link) {
        // Symlink doesn't exist or process is lacking permissions.
        return Ok(vec![]);
    };

    // if !metadata.is_symlink() {
    //     return Err(TraversalError::NotASymlink(
    //         parent_pointer.to_string_lossy().to_string(),
    //     ));
    // }

    // Must be a valid symlink.
    let resolved_parent = fs::read_link(&parent_link)
        .and_then(|resolved| resolved.canonicalize())
        .map_err(|err| TraversalError::ResolveError {
            parent_link_path: parent_link.to_string_lossy().to_string(),
            error: err.to_string(),
        })?;

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

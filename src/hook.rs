use crate::{
    hash::{Hash, SourceList},
    lang, loader, output,
    shadowenv::Shadowenv,
    trust::ensure_dir_tree_trusted,
    undo,
};
use anyhow::Error;
use serde_derive::Serialize;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
    result::Result,
    str::FromStr,
};

use crate::lang::ShadowLang;
use shell_escape as shell;

pub enum VariableOutputMode {
    FishMode,
    PorcelainMode,
    PosixMode,
    JsonMode,
    PrettyJsonMode,
}

#[derive(Serialize, Debug)]
struct Modifications {
    schema: String,
    exported: HashMap<String, Option<String>>,
    unexported: HashMap<String, Option<String>>, // Legacy. Not used, just shows up empty in json
}

impl Modifications {
    fn new(exports: HashMap<String, Option<String>>) -> Modifications {
        Modifications {
            schema: "v2".to_string(),
            exported: exports,
            unexported: HashMap::new(),
        }
    }
}

pub fn run(
    pathbuf: PathBuf,
    shadowenv_data: String,
    mode: VariableOutputMode,
    force: bool,
) -> Result<(), Error> {
    match load_env(pathbuf, shadowenv_data, force)? {
        Some(shadowenv) => {
            apply_env(&shadowenv, mode)?;
            Ok(())
        }
        None => Ok(()),
    }
}

pub fn load_env(
    pathbuf: PathBuf,
    shadowenv_data: String,
    force: bool,
) -> Result<Option<Shadowenv>, Error> {
    let mut parts = shadowenv_data.splitn(3, ":");
    let prev_hash = parts.next();
    let prev_dirs = parts.next().unwrap_or("[]");
    let json_data = parts.next().unwrap_or("{}");

    let active: Option<Hash> = match prev_hash {
        None => None,
        Some("") => None,
        Some("0000000000000000") => None,
        Some(x) => Some(Hash::from_str(x)?),
    };

    // "targets" are sources of shadowenv lisp files
    let targets = load_trusted_sources(pathbuf, false)?;

    let targets_hash = targets.as_ref().and_then(|targets| targets.hash());

    // before we had multiple targets, this ensured we only act if we needed to
    match (&active, &targets) {
        // if there is no active shadowenv and we've got no targets, then we have nothing to compute
        (None, None) => {
            return Ok(None);
        }
        // if there is an active shadowenv and some action we've taken leads us to still be in the same one, we do nothing
        // unless the force flag was specified
        // probably need to update whatever sets prev_hash to be a hash of all the targets' hashes (?)
        (Some(a), Some(_)) if a.hash == targets_hash.unwrap() && !force => {
            return Ok(None);
        }
        (_, _) => (),
    }

    // should probably be a hash of all the target's hashes (?, see above comment)
    // let target_hash = match &targets {
    //     Some(t) => t.hash().unwrap_or(0),
    //     None => 0,
    // };

    // "data" is used to undo changes made when activating a shadowenv
    // we will only have "data" if already inside a shadowenv
    let data = undo::Data::from_str(json_data)?;
    let prev_dirs: HashSet<PathBuf> = serde_json::from_str(prev_dirs)?;
    let shadowenv = Shadowenv::new(
        env::vars().collect(),
        data,
        targets_hash.unwrap_or(0),
        prev_dirs,
    );

    match targets {
        Some(targets) => {
            // run_program takes in the shadowenv, evaluates the code we found on it, and returns it
            match ShadowLang::run_programs(shadowenv, targets) {
                // no need to return anything descriptive here since we already
                // had ketos print it to stderr.
                Err(_) => Err(lang::ShadowlispError {}.into()),
                // note the "true" since we ran code to activate/modify the shadowenv
                Ok(shadowenv) => Ok(Some(shadowenv)),
            }
        }
        // note the "false" since we didn't have anything to run
        None => Ok(Some(shadowenv)),
    }
}

/// Load all Sources from the current dir, ensuring that they are all trusted.
fn load_trusted_sources(
    pathbuf: PathBuf,
    skip_trust_check: bool,
) -> Result<Option<SourceList>, Error> {
    let roots = loader::find_roots(&pathbuf, loader::DEFAULT_RELATIVE_COMPONENT)?;
    if roots.is_empty() {
        return Ok(None);
    }

    if !skip_trust_check {
        ensure_dir_tree_trusted(&roots)?;
    }

    let mut source_list = SourceList::new();
    for root in roots {
        let source = loader::load(root)?;
        if let Some(source) = source {
            // stack would be more efficient
            source_list.prepend_source(source);
        }
    }

    if source_list.is_empty() {
        return Ok(None);
    }

    Ok(Some(source_list))
}

pub fn mutate_own_env(shadowenv: &Shadowenv) -> Result<(), Error> {
    for (k, v) in shadowenv.exports()? {
        match v {
            Some(s) => env::set_var(k, &s),
            None => env::remove_var(k),
        }
    }

    Ok(())
}

pub fn apply_env(shadowenv: &Shadowenv, mode: VariableOutputMode) -> Result<(), Error> {
    match mode {
        VariableOutputMode::PosixMode => {
            for (k, v) in shadowenv.exports()? {
                match v {
                    Some(s) => println!("export {}={}", k, shell_escape(&s)),
                    None => println!("unset {}", k),
                }
            }
            output::print_activation_to_tty(
                shadowenv.current_dirs(),
                shadowenv.prev_dirs(),
                shadowenv.features(),
            );
        }
        VariableOutputMode::FishMode => {
            for (k, v) in shadowenv.exports()? {
                match v {
                    Some(s) => {
                        if k == "PATH" {
                            let pathlist = shell_escape(&s).replace(":", "' '");
                            println!("set -gx {} {}", k, pathlist);
                        } else {
                            println!("set -gx {} {}", k, shell_escape(&s));
                        }
                    }
                    None => {
                        println!("set -e {}", k);
                    }
                }
            }
            output::print_activation_to_tty(
                shadowenv.current_dirs(),
                shadowenv.prev_dirs(),
                shadowenv.features(),
            );
        }
        VariableOutputMode::PorcelainMode => {
            // three fields: <operation> : <name> : <value>
            // opcodes: 1: set, unexported (unused)
            //          2: set, exported
            //          3: unset (value is empty)
            // field separator is 0x1F; record separator is 0x1E. There's a trailing record
            // separator because I'm lazy but don't depend on it not going away.
            for (k, v) in shadowenv.exports()? {
                match v {
                    Some(s) => print!("\x02\x1F{}\x1F{}\x1E", k, s),
                    None => print!("\x03\x1F{}\x1F\x1E", k),
                }
            }
        }
        VariableOutputMode::JsonMode => {
            let modifs = Modifications::new(shadowenv.exports()?);
            println!("{}", serde_json::to_string(&modifs).unwrap());
        }
        VariableOutputMode::PrettyJsonMode => {
            let modifs = Modifications::new(shadowenv.exports()?);
            println!("{}", serde_json::to_string_pretty(&modifs).unwrap());
        }
    }
    Ok(())
}

fn shell_escape(s: &str) -> String {
    shell::escape(Cow::from(s)).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    #[test]
    fn load_trusted_source_returns_an_error_for_untrusted_folders() {
        let temp_dir = tempdir().unwrap().into_path();
        let path = temp_dir.to_string_lossy().to_string();
        fs::create_dir(temp_dir.join(".shadowenv.d")).unwrap();
        let result = load_trusted_sources(temp_dir, false);
        assert!(result.is_err());
        assert_eq!(format!("directory: '{}' contains untrusted shadowenv program: `shadowenv help trust` to learn more.", path), result.err().unwrap().to_string())
    }

    #[test]
    fn load_trusted_sources_returns_nearest_sources_last() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create test directories and files
        fs::create_dir_all(base_path.join("dir1/.shadowenv.d")).unwrap();
        fs::create_dir_all(base_path.join("dir1/dir2/.shadowenv.d")).unwrap();
        fs::write(
            base_path.join("dir1/.shadowenv.d/test.lisp"),
            "(env/set \"ORDER\" \"1\")",
        )
        .unwrap();
        fs::write(
            base_path.join("dir1/dir2/.shadowenv.d/test.lisp"),
            "(env/set \"ORDER\" \"2\")",
        )
        .unwrap();

        let result = load_trusted_sources(base_path.join("dir1/dir2"), true)
            .unwrap()
            .unwrap();

        let sources = result.consume();
        assert_eq!(sources.len(), 2);

        // Assert that sources are returned in the correct order
        // The order they are returned is the order they are executed in.
        // So the outermost env must come first, with the innermost dir coming last.
        assert!(sources[0].dir.ends_with("dir1"));
        assert!(sources[1].dir.ends_with("dir1/dir2"));
    }
}

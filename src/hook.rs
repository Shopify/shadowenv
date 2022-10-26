use crate::hash::{Hash, Source};
use crate::lang;
use crate::loader;
use crate::output;
use crate::shadowenv::Shadowenv;
use crate::trust;
use crate::undo;
use serde_derive::Serialize;

use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::result::Result;
use std::str::FromStr;

use crate::lang::ShadowLang;
use failure::Error;
use shell_escape as shell;

pub enum VariableOutputMode {
    FishMode,
    PorcelainMode,
    PosixMode,
    JsonMode,
    PrettyJsonMode,
    XonshMode,
}

#[derive(Serialize, Debug)]
struct Modifications {
    schema: String,
    exported: HashMap<String, Option<String>>,
    unexported: HashMap<String, Option<String>>, // Legacy. Not used, just shows up empty in json
}

impl Modifications {
    fn new(exports: HashMap<String, Option<String>>) -> Modifications {
        return Modifications {
            schema: "v2".to_string(),
            exported: exports,
            unexported: HashMap::new(),
        };
    }
}

pub fn run(
    pathbuf: PathBuf,
    shadowenv_data: String,
    mode: VariableOutputMode,
    force: bool,
) -> Result<(), Error> {
    match load_env(pathbuf, shadowenv_data, force)? {
        Some((shadowenv, activation)) => {
            apply_env(&shadowenv, mode, activation)?;
            Ok(())
        }
        None => Ok(()),
    }
}

pub fn load_env(
    pathbuf: PathBuf,
    shadowenv_data: String,
    force: bool,
) -> Result<Option<(Shadowenv, bool)>, Error> {
    let mut parts = shadowenv_data.splitn(2, ":");
    let prev_hash = parts.next();
    let json_data = parts.next().unwrap_or("{}");

    let active: Option<Hash> = match prev_hash {
        None => None,
        Some("") => None,
        Some("0000000000000000") => None,
        Some(x) => Some(Hash::from_str(x)?),
    };

    let target: Option<Source> = load_trusted_source(pathbuf)?;

    match (&active, &target) {
        (None, None) => {
            return Ok(None);
        }
        (Some(a), Some(t)) if a.hash == t.hash()? && !force => {
            return Ok(None);
        }
        (_, _) => (),
    }

    let target_hash = match &target {
        Some(t) => t.hash().unwrap_or(0),
        None => 0,
    };

    let data = undo::Data::from_str(json_data)?;
    let shadowenv = Shadowenv::new(env::vars().collect(), data, target_hash);

    match target {
        Some(target) => {
            match ShadowLang::run_program(shadowenv, target) {
                // no need to return anything descriptive here since we already
                // had ketos print it to stderr.
                Err(_) => Err(lang::ShadowlispError {}.into()),
                Ok(shadowenv) => Ok(Some((shadowenv, true))),
            }
        }
        None => Ok(Some((shadowenv, false))),
    }
}

/// Load a Source from the current dir, ensuring that it is trusted.
fn load_trusted_source(pathbuf: PathBuf) -> Result<Option<Source>, Error> {
    if let Some(root) = loader::find_root(&pathbuf, loader::DEFAULT_RELATIVE_COMPONENT)? {
        if !trust::is_dir_trusted(&root)? {
            return Err(trust::NotTrusted {
                not_trusted_dir_path: pathbuf.to_string_lossy().to_string(),
            }
            .into());
        }
        return Ok(loader::load(root)?);
    }
    Ok(None)
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

pub fn apply_env(
    shadowenv: &Shadowenv,
    mode: VariableOutputMode,
    activation: bool,
) -> Result<(), Error> {
    match mode {
        VariableOutputMode::PosixMode => {
            for (k, v) in shadowenv.exports()? {
                match v {
                    Some(s) => println!("export {}={}", k, shell_escape(&s)),
                    None => println!("unset {}", k),
                }
            }
            output::print_activation_to_tty(activation, shadowenv.features());
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
            output::print_activation_to_tty(activation, shadowenv.features());
        }
        VariableOutputMode::XonshMode => {
            for (k, v) in shadowenv.exports()? {
                match v {
                    Some(s) => println!(r#"${} = "{}""#, k, s.escape_default()),
                    None => println!("del ${}", k),
                }
            }
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
        let result = load_trusted_source(temp_dir);
        assert!(result.is_err());
        assert_eq!(format!("directory: '{}' contains untrusted shadowenv program: `shadowenv help trust` to learn more.", path), result.err().unwrap().to_string())
    }
}

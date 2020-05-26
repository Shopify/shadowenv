use crate::hash::{Hash, Source};
use crate::lang;
use crate::lang::ShadowLang;
use crate::loader;
use crate::output;
use crate::serde_json;
use crate::shadowenv::Shadowenv;
use crate::trust;
use crate::undo;

use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;
use std::result::Result;
use std::str::FromStr;

use failure::Error;
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
    let shadowenv = Rc::new(Shadowenv::new(env::vars().collect(), data, target_hash));

    let activation = match target {
        Some(target) => {
            if let Err(_) = ShadowLang::run_program(shadowenv.clone(), target) {
                // no need to return anything descriptive here since we already had ketos print it
                // to stderr.
                return Err(lang::ShadowlispError {}.into());
            }
            true
        }
        None => false,
    };

    let shadowenv = Rc::try_unwrap(shadowenv).unwrap();
    Ok(Some((shadowenv, activation)))
}

/// Load a Source from the current dir, ensuring that it is trusted.
fn load_trusted_source(pathbuf: PathBuf) -> Result<Option<Source>, Error> {
    if let Some(root) = loader::find_root(pathbuf, loader::DEFAULT_RELATIVE_COMPONENT)? {
        if !trust::is_dir_trusted(&root)? {
            return Err(trust::NotTrusted {}.into());
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

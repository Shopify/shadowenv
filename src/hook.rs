use crate::hash::Source;
use crate::lang;
use crate::lang::ShadowLang;
use crate::loader;
use crate::output;
use crate::shadowenv::Shadowenv;
use crate::undo;

use std::env;
use std::rc::Rc;
use std::result::Result;

use failure::Error;
use serde_json;

pub enum VariableOutputMode {
    FishMode,
    PorcelainMode,
    PosixMode,
}

pub fn run(shadowenv_data: &str, mode: VariableOutputMode) -> Result<(), Error> {
    let target: Option<Source> =
        loader::load(env::current_dir()?, loader::DEFAULT_RELATIVE_COMPONENT)?;

    let (active, data) = undo::load_shadowenv_data(shadowenv_data)?;

    let skip = match (&active, &target) {
        (None, None) => true,
        (Some(a), Some(t)) if a.hash == t.hash()? => true,
        (_, _) => false,
    };
    if skip {
        return Ok(());
    }

    let shadowenv = Rc::new(Shadowenv::new(env::vars().collect(), data));

    let target_hash = match &target {
        Some(t) => t.hash().unwrap_or(0),
        None => 0,
    };

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
    let final_data = shadowenv.shadowenv_data();
    let shadowenv_data =
        format!("{:016x}:", target_hash).to_string() + &serde_json::to_string(&final_data)?;

    match mode {
        VariableOutputMode::PosixMode => {
            println!("__shadowenv_data={:?}", shadowenv_data);
            for (k, v) in shadowenv.exports() {
                match v {
                    Some(s) => println!("export {}={:?}", k, s),
                    None => println!("unset {}", k),
                }
            }
        }
        VariableOutputMode::FishMode => {
            println!("set -g __shadowenv_data {:?}", shadowenv_data);
            for (k, v) in shadowenv.exports() {
                match v {
                    Some(s) => {
                        if k == "PATH" {
                            let pathlist = format!("{:?}", s);
                            let pathlist = pathlist.replace(":", "\" \"");
                            println!("set -gx {} {}", k, pathlist);
                        } else {
                            println!("set -gx {} {:?}", k, s);
                        }
                    }
                    None => {
                        println!("set -e {}", k);
                    }
                }
            }
        }
        VariableOutputMode::PorcelainMode => {
            // three fields: <operation> : <name> : <value>
            // opcodes: 1: set, unexported
            //          2: set, exported
            //          3: unset (value is empty)
            // field separator is 0x1F; record separator is 0x1E. There's a trailing record
            // separator because I'm lazy but don't depend on it not going away.
            print!("\x01\x1F__shadowenv_data\x1F{}\x1E", shadowenv_data);
            for (k, v) in shadowenv.exports() {
                match v {
                    Some(s) => print!("\x02\x1F{}\x1F{}\x1E", k, s),
                    None => print!("\x03\x1F{}\x1F\x1E", k),
                }
            }
        }
    }

    output::print_activation(activation);
    Ok(())
}

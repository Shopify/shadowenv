use crate::hash::{Hash, Source};
use crate::lang::ShadowLang;
use crate::loader;
use crate::shadowenv::Shadowenv;
use crate::undo;

use std::env;
use std::rc::Rc;
use std::result::Result;
use std::str::FromStr;

use failure::Error;
use serde_json;

pub enum VariableOutputMode {
    FishMode,
    PorcelainMode,
    PosixMode,
}

pub fn run(shadowenv_data: &str, mode: VariableOutputMode) -> Result<(), Error> {
    let mut parts = shadowenv_data.splitn(2, ":");
    let prev_hash = parts.next();
    let json_data = parts.next().unwrap_or("{}");

    let active: Option<Hash> = match prev_hash {
        None => None,
        Some("") => None,
        Some("0000000000000000") => None,
        Some(x) => Some(Hash::from_str(x)?),
    };

    let target: Option<Source> =
        loader::load(env::current_dir()?, loader::DEFAULT_RELATIVE_COMPONENT)?;

    match (&active, &target) {
        (None, None) => {
            return Ok(());
        }
        (Some(a), Some(t)) if a.hash == t.hash()? => {
            return Ok(());
        }
        (_, _) => (),
    }

    let data = undo::Data::from_str(json_data)?;
    let shadowenv = Rc::new(Shadowenv::new(env::vars().collect(), data));

    let target_hash = match &target {
        Some(t) => t.hash().unwrap_or(0),
        None => 0,
    };

    match target {
        Some(target) => {
            print_activation(true);
            if let Err(_err) = ShadowLang::run_program(shadowenv.clone(), target) {
                // TODO: error type
                panic!();
            }
        }
        None => {
            print_activation(false);
        }
    }

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
            println!("set __shadowenv_data {:?}", shadowenv_data);
            for (k, v) in shadowenv.exports() {
                match v {
                    Some(s) => {
                        // TODO(burke): it looks like we had to do some weird shit in dev to
                        // accommodate fish's weird handling of PATH. we'll probably have to re-do
                        // it here.
                        println!("set -gx {} {:?}", k, s);
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

    Ok(())
}

fn print_activation(activated: bool) {
    let word = match activated {
        true => "activated",
        false => "deactivated",
    };
    let shadowenv = String::new()
        + "\x1b[38;5;249ms\x1b[38;5;248mh\x1b[38;5;247ma\x1b[38;5;246md\x1b[38;5;245mo"
        + "\x1b[38;5;244mw\x1b[38;5;243me\x1b[38;5;242mn\x1b[38;5;241mv\x1b[38;5;240m";
    eprintln!("\x1b[1;34m{} {}.\x1b[0m", word, shadowenv);
}

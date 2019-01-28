#![feature(try_from)]

extern crate ketos;
#[macro_use]
extern crate ketos_derive;
extern crate serde;
#[macro_use]
extern crate serde_derive;

mod lang;
mod shadowenv;
mod hash;
mod loader;
mod undo;

use crate::shadowenv::Shadowenv;
use crate::lang::ShadowLang;
use crate::hash::{Hash, Source};

use std::env;
use std::error::Error;
use std::process;
use std::rc::Rc;
use std::result::Result;
use std::str::FromStr;

use serde_json;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: {} <posix|fish> \"$__shadowenv_data\"", args[0]);
        process::exit(1);
    }

    if let Err(err) = run(&args[2]) {
        eprintln!("error: {}", err);
        std::process::exit(1);
    }
}

fn run(shadowenv_data: &str) -> Result<(), Box<Error>> {
    let mut parts = shadowenv_data.splitn(2, ":");
    let prev_hash = parts.next();
    let json_data = parts.next().unwrap_or("{}");

    let active: Option<Hash> = match prev_hash {
        None     => None,
        Some("") => None,
        Some(x)  => Some(Hash::from_str(x)?),
    };

    let target: Option<Source> = loader::load(env::current_dir()?, ".shadowenv.d")?;

    match (&active, &target) {
        (None, None) => { return Ok(()); },
        (Some(a), Some(t)) if a.hash == t.hash()? => { return Ok(()); },
        (_, _) => (),
    }

    let data = undo::Data::from_str(json_data)?;
    let shadowenv = Rc::new(Shadowenv::new(env::vars().collect(), data));

    let target_hash = match &target { Some(t) => t.hash().unwrap_or(0), None => 0 };
    // TODO: on initial load, we print 'deactivated shadowenv.', but shouldn't.
    // I think this has to do with target_hash==0

    if let Some(target) = target {
        if let Err(_err) = ShadowLang::run_program(shadowenv.clone(), target) {
            panic!();
        }
    }

    let shadowenv = Rc::try_unwrap(shadowenv).unwrap();
    let final_data = shadowenv.shadowenv_data();
    println!("__shadowenv_data={:?}", format!("{:016x}:", target_hash).to_string() + &serde_json::to_string(&final_data)?);
    for (k, v) in shadowenv.exports() {
        match v {
            Some(s) => { println!("export {}={:?}", k, s); },
            None => { println!("unset {}", k); },
        }
    }

    print_activation(shadowenv.exports().len() > 0);

    Ok(())
}

fn print_activation(activated: bool) {
    let word = match activated { true => "activated", false => "deactivated" };
    let shadowenv = String::new() +
        "\x1b[38;5;249ms\x1b[38;5;248mh\x1b[38;5;247ma\x1b[38;5;246md\x1b[38;5;245mo" +
        "\x1b[38;5;244mw\x1b[38;5;243me\x1b[38;5;242mn\x1b[38;5;241mv\x1b[38;5;240m";
    eprintln!("\x1b[1;34m{} {}.\x1b[0m", word, shadowenv);
}

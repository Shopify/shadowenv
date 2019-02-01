#![feature(try_from)]

extern crate ketos;
#[macro_use]
extern crate ketos_derive;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;
extern crate clap;

mod lang;
mod shadowenv;
mod hash;
mod loader;
mod undo;
mod tests;
mod hook;
mod init;

use std::env;
use std::process;
use clap::{Arg, App, SubCommand, AppSettings};

use crate::hook::VariableOutputMode;

fn main() {
    let app_matches = App::new("shadowenv")
        .version("0.0.1")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(SubCommand::with_name("hook")
                    .about("Runs the shell hook. You shouldn't need to run this manually.")
                    .arg(Arg::with_name("$__shadowenv_data")
                         .required(true))
                    .arg(Arg::with_name("fish")
                         .long("fish")
                         .help("Format variable assignments for fish shell")))
        .subcommand(SubCommand::with_name("init")
                    .about("Prints a script which can be eval'd to set up shadowenv in various shells")
                    .setting(AppSettings::SubcommandRequiredElseHelp)
                    .subcommand(SubCommand::with_name("bash")
                                .about("Prints a script which can be eval'd by bash to set up shadowenv."))
                    .subcommand(SubCommand::with_name("zsh")
                                .about("Prints a script which can be eval'd by zsh to set up shadowenv."))
                    .subcommand(SubCommand::with_name("fish")
                                .about("Prints a script which can be eval'd by fish to set up shadowenv.")))
        .get_matches();

    match app_matches.subcommand() {
        ("hook", Some(matches)) => {
            let data = matches.value_of("$__shadowenv_data").unwrap();
            let mode = match matches.is_present("fish") {
                true => VariableOutputMode::FishMode,
                false => VariableOutputMode::PosixMode
            };
            if let Err(err) = hook::run(data, mode) {
                eprintln!("error: {}", err);
                std::process::exit(1);
            }
        },
        ("init", Some(matches)) => {
            let argv0: String = env::args().next().unwrap();
            let shellname = matches.subcommand_name().unwrap();
            process::exit(init::run(argv0.as_ref(), shellname));
        },
        _ => { panic!("subcommand was required by config but somehow none was provided"); },
    }
}


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
extern crate hex;
extern crate regex;
extern crate signatory;
extern crate signatory_dalek;

mod hash;
mod hook;
mod init;
mod lang;
mod loader;
mod shadowenv;
mod trust;
mod undo;

use clap::{App, AppSettings, Arg, SubCommand};
use std::env;
use std::process;

use crate::hook::VariableOutputMode;
use crate::trust::NotTrusted;

fn main() {
    let app_matches = App::new("shadowenv")
        .version("0.0.1")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("hook")
                .about("Runs the shell hook. You shouldn't need to run this manually.")
                .arg(Arg::with_name("$__shadowenv_data").required(true))
                .arg(
                    Arg::with_name("fish")
                        .long("fish")
                        .help("Format variable assignments for fish shell"),
                ),
        )
        .subcommand(
            SubCommand::with_name("trust")
                .about("Mark this directory as 'trusted', allowing shadowenv programs to be run"),
        )
        .subcommand(
            SubCommand::with_name("init")
                .about("Prints a script which can be eval'd to set up shadowenv in various shells")
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .subcommand(
                    SubCommand::with_name("bash")
                        .about("Prints a script which can be eval'd by bash to set up shadowenv."),
                )
                .subcommand(
                    SubCommand::with_name("zsh")
                        .about("Prints a script which can be eval'd by zsh to set up shadowenv."),
                )
                .subcommand(
                    SubCommand::with_name("fish")
                        .about("Prints a script which can be eval'd by fish to set up shadowenv."),
                ),
        )
        .get_matches();

    match app_matches.subcommand() {
        ("hook", Some(matches)) => {
            let data = matches.value_of("$__shadowenv_data").unwrap();
            let mode = match matches.is_present("fish") {
                true => VariableOutputMode::FishMode,
                false => VariableOutputMode::PosixMode,
            };
            if let Err(err) = hook::run(data, mode) {
                handle_hook_error(err);
            }
        }
        ("trust", Some(_)) => {
            trust::run();
        }
        ("init", Some(matches)) => {
            let shellname = matches.subcommand_name().unwrap();
            process::exit(init::run(shellname));
        }
        _ => {
            panic!("subcommand was required by config but somehow none was provided");
        }
    }
}

fn handle_hook_error(err: failure::Error) {
    let shadowenv = String::new()
        + "\x1b[38;5;249ms\x1b[38;5;248mh\x1b[38;5;247ma\x1b[38;5;246md\x1b[38;5;245mo"
        + "\x1b[38;5;244mw\x1b[38;5;243me\x1b[38;5;242mn\x1b[38;5;241mv\x1b[38;5;240m";

    let re = regex::Regex::new(r"`(.*?)`").unwrap();
    let before = format!("{}", err);
    let err = re.replace_all(before.as_ref(), "\x1b[1;32m$1\x1b[1;31m");

    eprintln!("{} \x1b[1;31mfailure: {}\x1b[0m", shadowenv, err);

    std::process::exit(1);
}

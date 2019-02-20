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
extern crate dirs;
extern crate hex;
extern crate libc;
extern crate regex;
extern crate signatory;
extern crate signatory_dalek;

mod hash;
mod hook;
mod init;
mod lang;
mod loader;
mod output;
mod shadowenv;
mod trust;
mod undo;

use clap::{App, AppSettings, Arg, SubCommand};
use std::process;

use crate::hook::VariableOutputMode;

fn main() {
    let version = format!(
        "{}.{}.{}{}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
        option_env!("CARGO_PKG_VERSION_PRE").unwrap_or("")
    );

    let app_matches = App::new("shadowenv")
        .version(&version[..])
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("hook")
                .about("Runs the shell hook. You shouldn't need to run this manually.")
                .arg(Arg::with_name("$__shadowenv_data").required(true))
                .arg(
                    Arg::with_name("fish")
                        .long("fish")
                        .help("Format variable assignments for fish shell"),
                )
                .arg(
                    Arg::with_name("silent")
                        .long("silent")
                        .help("Suppress error printing"),
                )
                .arg(
                    Arg::with_name("porcelain")
                        .long("porcelain")
                        .help("Format variable assignments for machine parsing"),
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
            let mode = match matches.is_present("porcelain") {
                true => VariableOutputMode::PorcelainMode,
                false => match matches.is_present("fish") {
                    true => VariableOutputMode::FishMode,
                    false => VariableOutputMode::PosixMode,
                },
            };
            if let Err(err) = hook::run(data, mode) {
                process::exit(output::handle_hook_error(err, matches.is_present("silent")));
            }
        }
        ("trust", Some(_)) => {
            if let Err(err) = trust::run() {
                eprintln!("{}", err); // TODO: better formatting
                process::exit(1);
            }
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

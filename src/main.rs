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
extern crate atty;

mod features;
mod hash;
mod hook;
mod init;
mod diff;
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
                    // this is necessary if shadowenv hook is called from a subshell, as we do in
                    // the bash hook
                    Arg::with_name("shellpid")
                        .long("shellpid")
                        .takes_value(true)
                        .help("rather than looking up the PPID, use this as the shell's pid"),
                )
                .arg(
                    Arg::with_name("porcelain")
                        .long("porcelain")
                        .help("Format variable assignments for machine parsing"),
                ),
        )
        .subcommand(
            SubCommand::with_name("diff")
                .about("Display a diff of changed environment variables")
                .arg(
                    Arg::with_name("verbose")
                    .long("verbose")
                    .short("v")
                    .help("Show all environment variables, not just those that changed"),
                ).arg(
                    Arg::with_name("no-color")
                    .long("no-color")
                    .short("n")
                    .help("Do not use color to highlight the diff"),
                )
                .arg(Arg::with_name("$__shadowenv_data").required(true)),
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
            let shellpid = determine_shellpid_or_crash(matches.value_of("shellpid"));

            let mode = match matches.is_present("porcelain") {
                true => VariableOutputMode::PorcelainMode,
                false => match matches.is_present("fish") {
                    true => VariableOutputMode::FishMode,
                    false => VariableOutputMode::PosixMode,
                },
            };
            if let Err(err) = hook::run(data, mode) {
                process::exit(output::handle_hook_error(
                    err,
                    shellpid,
                    matches.is_present("silent"),
                ));
            }
        }
        ("diff", Some(matches)) => {
            let verbose = matches.is_present("verbose");
            let color = !matches.is_present("no-color");
            let data = matches.value_of("$__shadowenv_data").unwrap();
            process::exit(diff::run(verbose, color, data));
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

fn determine_shellpid_or_crash(arg: Option<&str>) -> u32 {
    match arg {
        Some(arg) => arg
            .parse::<u32>()
            .expect("shadowenv error: invalid non-numeric argument for --shellpid"),
        None => unsafe_getppid().expect("shadowenv bug: unable to get parent pid"),
    }
}

fn unsafe_getppid() -> Result<u32, failure::Error> {
    let ppid;
    unsafe { ppid = libc::getppid() }
    if ppid < 1 {
        return Err(format_err!("somehow failed to get ppid"));
    }
    Ok(ppid as u32)
}

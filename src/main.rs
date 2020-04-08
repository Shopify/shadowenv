extern crate ketos;
#[macro_use]
extern crate ketos_derive;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;
extern crate atty;
extern crate clap;
extern crate dirs;
extern crate exec;
extern crate hex;
extern crate libc;
extern crate regex;
extern crate serde_json;
extern crate shellexpand;
extern crate signatory;
extern crate signatory_dalek;

#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

mod diff;
mod execcmd;
mod features;
mod hash;
mod hook;
mod init;
mod lang;
mod loader;
mod output;
mod shadowenv;
mod trust;
mod undo;

use crate::shadowenv::Shadowenv;
use clap::{App, AppSettings, Arg, ArgGroup, SubCommand};
use std::env;
use std::path::PathBuf;
use std::process;

use crate::hook::VariableOutputMode;

fn main() {
    let current_dir = match env::current_dir() {
        Ok(dir) => dir,
        Err(_) => return, // If the current dir was deleted, there's not much we can do. Just exit silently.
    };

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
                .setting(AppSettings::DisableHelpSubcommand)
                .arg(
                    // Legacy: This is exported now, and in fact this setting is ignored
                    // completely if $__shadowenv_data is present in the environment.
                    Arg::with_name("$__shadowenv_data").required(false)
                )
                .arg(
                    Arg::with_name("fish")
                        .long("fish")
                        .help("Format variable assignments for fish shell"),
                )
                .arg(
                    Arg::with_name("posix")
                        .long("posix")
                        .help("Format variable assignments for posix shells (default)"),
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
                )
                .arg(
                    Arg::with_name("json")
                        .long("json")
                        .help("Format variable assignments as JSON"),
                )
                .arg(
                    Arg::with_name("pretty-json")
                        .long("pretty-json")
                        .help("Format variable assignments as pretty JSON"),
                )
                .group(ArgGroup::with_name("format").args(&["porcelain", "posix", "fish", "json", "pretty-json"])),
        )
        .subcommand(
            SubCommand::with_name("diff")
                .about("Display a diff of changed environment variables.")
                .setting(AppSettings::DisableHelpSubcommand)
                .arg(
                    Arg::with_name("verbose")
                        .long("verbose")
                        .short("v")
                        .help("Show all environment variables, not just those that changed"),
                )
                .arg(
                    Arg::with_name("no-color")
                        .long("no-color")
                        .short("n")
                        .help("Do not use color to highlight the diff"),
                )
                .arg(
                    // Legacy: This is exported now, and in fact this setting is ignored
                    // completely if $__shadowenv_data is present in the environment.
                    Arg::with_name("$__shadowenv_data").required(false)
                ),
        )
        .subcommand(
            SubCommand::with_name("trust")
                .about("Mark this directory as 'trusted', allowing shadowenv programs to be run.")
                .setting(AppSettings::DisableHelpSubcommand)
        )
        .subcommand(
            SubCommand::with_name("exec")
                .about(
                    "Execute a command after loading the environment from the current directory.",
                )
                .setting(AppSettings::DisableHelpSubcommand)
                .arg(
                    // Legacy: This is exported now, and this flag will likely go away soon.
                    Arg::with_name("$__shadowenv_data")
                        .long("shadowenv-data")
                        .takes_value(true)
                        .help("Legacy, will be removed soon: Don't use this; provide $__shadowenv_data in the environment instead"),
                )
                .arg(
                    Arg::with_name("dir")
                        .long("dir")
                        .takes_value(true)
                        .help("Instead of searching from the current directory for a .shadowenv.d, search from this one."),
                )
                .arg(
                    Arg::with_name("child-argv0")
                        .help("If the command doesn't need arguments, it can be passed directly as the last arugment."),
                )
                .arg(
                    Arg::with_name("child-argv")
                        .multiple(true)
                        .last(true)
                        .help("If the command requires arguments, they must all be passed after a '--'."),
                )
                .group(
                    ArgGroup::with_name("argv")
                             .args(&["child-argv0", "child-argv"])
                             .required(true),
                )
        )
        .subcommand(
            SubCommand::with_name("init")
                .about("Prints a script which can be eval'd to set up shadowenv in various shells.")
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .setting(AppSettings::DisableHelpSubcommand)
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
                )
        )
        .get_matches();

    match app_matches.subcommand() {
        ("hook", Some(matches)) => {
            let legacy_fallback_data = matches.value_of("$__shadowenv_data").map(|d| d.to_string());
            let data = Shadowenv::load_shadowenv_data_or_legacy_fallback(legacy_fallback_data);
            let shellpid = determine_shellpid_or_crash(matches.value_of("shellpid"));

            let mode = match true {
                true if matches.is_present("porcelain") => VariableOutputMode::PorcelainMode,
                true if matches.is_present("fish") => VariableOutputMode::FishMode,
                true if matches.is_present("json") => VariableOutputMode::JsonMode,
                true if matches.is_present("pretty-json") => VariableOutputMode::PrettyJsonMode,
                _ => VariableOutputMode::PosixMode,
            };
            if let Err(err) = hook::run(current_dir, data, mode) {
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
            let legacy_fallback_data = matches.value_of("$__shadowenv_data").map(|d| d.to_string());
            let data = Shadowenv::load_shadowenv_data_or_legacy_fallback(legacy_fallback_data);
            process::exit(diff::run(verbose, color, data));
        }
        ("trust", Some(_)) => {
            if let Err(err) = trust::run() {
                eprintln!("{}", err); // TODO: better formatting
                process::exit(1);
            }
        }
        ("exec", Some(matches)) => {
            let legacy_fallback_data = matches.value_of("$__shadowenv_data").map(|d| d.to_string());
            let data = Shadowenv::load_shadowenv_data_or_legacy_fallback(legacy_fallback_data);
            let argv: Vec<&str> = match (
                matches.value_of("child-argv0"),
                matches.values_of("child-argv"),
            ) {
                (_, Some(argv)) => argv.collect(),
                (Some(argv0), _) => vec![argv0],
                (_, _) => unreachable!(),
            };
            let dir = matches.value_of("dir");
            let pathbuf = dir.map(|d| PathBuf::from(d)).unwrap_or(current_dir);
            if let Err(err) = execcmd::run(pathbuf, data, argv) {
                eprintln!("{}", err);
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

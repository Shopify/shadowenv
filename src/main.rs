mod cli;
mod diff;
mod execcmd;
mod features;
mod hash;
mod hook;
mod init;
mod lang;
mod loader;
mod output;
mod prompt_widget;
mod shadowenv;
mod trust;
mod undo;

use crate::{hook::VariableOutputMode, shadowenv::Shadowenv};
use anyhow::{anyhow, Error};
use clap::Parser;
use cli::{DiffCmd, ExecCmd, HookCmd};
use std::{env, iter, path::PathBuf, process};

fn main() {
    use cli::ShadowenvApp::*;

    let result = match cli::ShadowenvApp::parse() {
        Diff(cmd) => Ok(run_diff(cmd)),
        Exec(cmd) => run_exec(cmd),
        Hook(cmd) => run_hook(cmd),
        Init(cmd) => Ok(init::run(cmd)),
        Trust(_) => trust::run(),
        PromptWidget(_) => Ok(prompt_widget::run()),
    };

    if let Err(err) = result {
        if err.to_string() != "" {
            eprintln!("{}", err);
        }

        process::exit(1);
    }
}

fn run_diff(cmd: DiffCmd) {
    let color = !cmd.no_color;
    let data = Shadowenv::from_env();

    diff::run(cmd.verbose, color, data);
}

fn run_hook(cmd: HookCmd) -> Result<(), Error> {
    let mode = if cmd.format.porcelain {
        VariableOutputMode::Porcelain
    } else if cmd.format.fish {
        VariableOutputMode::Fish
    } else if cmd.format.json {
        VariableOutputMode::Json
    } else if cmd.format.pretty_json {
        VariableOutputMode::PrettyJson
    } else {
        VariableOutputMode::Posix
    };

    let data = Shadowenv::from_env();
    if let Err(err) = hook::run(get_current_dir_or_exit(), data, mode, cmd.force) {
        let pid = cmd
            .shellpid
            .unwrap_or_else(|| unsafe_getppid().expect("shadowenv bug: unable to get parent pid"));

        match output::format_hook_error(err, pid, cmd.silent) {
            Some(formatted) => Err(anyhow!(formatted)),
            None => Err(anyhow!("")),
        }
    } else {
        Ok(())
    }
}

fn run_exec(cmd: ExecCmd) -> Result<(), Error> {
    let data = Shadowenv::from_env();
    let pathbuf = cmd
        .dir
        .map(|d| PathBuf::from(d))
        .unwrap_or(get_current_dir_or_exit());

    let argv = iter::once(cmd.cmd).chain(cmd.cmd_argv);
    execcmd::run(pathbuf, data, argv.collect())
}

fn get_current_dir_or_exit() -> PathBuf {
    match env::current_dir() {
        Ok(dir) => dir,
        Err(_) => process::exit(0), // If the current dir was deleted, there's not much we can do. Just exit silently.
    }
}

fn unsafe_getppid() -> Result<u32, Error> {
    let ppid;
    unsafe { ppid = libc::getppid() }
    if ppid < 1 {
        return Err(anyhow!("failed to get ppid"));
    }
    Ok(ppid as u32)
}

mod cli;
mod diff;
mod exec_cmd;
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

use anyhow::{anyhow, Error};
use clap::Parser;
use std::{env, path::PathBuf, process};

fn main() {
    use cli::ShadowenvApp::*;

    let result = match cli::ShadowenvApp::parse() {
        Diff(cmd) => {
            diff::run(cmd);
            Ok(())
        }
        Exec(cmd) => exec_cmd::run(cmd),
        Hook(cmd) => hook::run(cmd),
        Init(cmd) => init::run(cmd),
        Trust(_) => trust::run(),
        PromptWidget(_) => {
            prompt_widget::run();
            Ok(())
        }
    };

    if let Err(err) = result {
        if err.to_string() != "" {
            eprintln!("{}", err);
        }

        process::exit(1);
    }
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

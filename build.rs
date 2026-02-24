extern crate clap;

use clap::{CommandFactory, ValueEnum};
use clap_complete::{generate_to, Shell};

include!("src/cli.rs");

fn main() {
    for shell in Shell::value_variants() {
        let mut cmd = ShadowenvApp::command();
        let name = cmd.get_name().to_string();

        generate_to(*shell, &mut cmd, name, "sh/completions").unwrap();
    }
}

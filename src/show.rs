use crate::shadowenv::Shadowenv;
use crate::undo;

use std::env;
use std::result::Result;

use failure::Error;

pub fn run(shadowenv_data: &str) -> Result<(), Error> {
    let (_, data) = undo::load_shadowenv_data(shadowenv_data)?;
    let shadowenv = Shadowenv::new(env::vars().collect(), data);

    for k in shadowenv.all_relevant_varnames().iter() {
        let curr = shadowenv.initial_env.get(k);
        let root = shadowenv.unshadowed_env.get(k);

        match (curr, root) {
            (Some(v), None) => {
                eprintln!("\x1b[32m+{}={}\x1b[0m", k, v);
            }
            (None, Some(v)) => {
                eprintln!("\x1b[31m-{}={}\x1b[0m", k, v);
            }
            (None, None) => {}
            (Some(c), Some(r)) => {
                if c == r {
                    eprintln!("{}={}", k, c);
                } else {
                    eprintln!("\x1b[32m+{}={}\x1b[0m", k, c);
                    eprintln!("\x1b[31m-{}={}\x1b[0m", k, r);
                }
            }
        };
    }

    Ok(())
}

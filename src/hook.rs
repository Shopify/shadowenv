use crate::{
    cli::HookCmd,
    get_current_dir_or_exit,
    hash::{Hash, SourceList},
    lang::{self, ShadowLang},
    loader, output,
    shadowenv::Shadowenv,
    trust::ensure_dir_tree_trusted,
    undo, unsafe_getppid,
};
use anyhow::{anyhow, Error};
use serde_derive::Serialize;
use shell_escape as shell;
use std::{borrow::Cow, collections::HashMap, env, path::PathBuf, result::Result, str::FromStr};

pub enum VariableOutputMode {
    Fish,
    Porcelain,
    Posix,
    Json,
    PrettyJson,
}

#[derive(Serialize, Debug)]
struct Modifications {
    schema: String,
    exported: HashMap<String, Option<String>>,
    unexported: HashMap<String, Option<String>>, // Legacy. Not used, just shows up empty in json
}

impl Modifications {
    fn new(exports: HashMap<String, Option<String>>) -> Modifications {
        Modifications {
            schema: "v2".to_string(),
            exported: exports,
            unexported: HashMap::new(),
        }
    }
}

pub fn run(cmd: HookCmd) -> Result<(), Error> {
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
    let result =
        load_env(get_current_dir_or_exit(), data, cmd.force, cmd.clobber).and_then(|loaded_env| {
            if let Some(shadowenv) = loaded_env {
                apply_env(&shadowenv, mode)
            } else {
                Ok(())
            }
        });

    // Reformat the error if needed.
    if let Err(err) = result {
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

pub fn load_env(
    pathbuf: PathBuf,
    shadowenv_data: String,
    force: bool,
    clobber: bool,
) -> Result<Option<Shadowenv>, Error> {
    let mut parts = shadowenv_data.splitn(2, ":");
    let prev_hash = parts.next();
    let json_data = parts.next().unwrap_or("{}");

    let active: Option<Hash> = match prev_hash {
        None => None,
        Some("") => None,
        Some("0000000000000000") => None,
        Some(x) => Some(Hash::from_str(x)?),
    };

    // "targets" are sources of shadowenv lisp files
    let targets = load_trusted_sources(pathbuf, false)?;

    let targets_hash = targets.as_ref().and_then(|targets| targets.hash());

    // before we had multiple targets, this ensured we only act if we needed to
    match (&active, &targets) {
        // if there is no active shadowenv and we've got no targets, then we have nothing to compute
        (None, None) => {
            return Ok(None);
        }
        // if there is an active shadowenv and some action we've taken leads us to still be in the same one, we do nothing
        // unless the force flag was specified
        // probably need to update whatever sets prev_hash to be a hash of all the targets' hashes (?)
        (Some(a), Some(_)) if a.hash == targets_hash.unwrap() && !force => {
            return Ok(None);
        }
        (_, _) => (),
    }

    // "data" is used to undo changes made when activating a shadowenv
    // we will only have "data" if already inside a shadowenv
    let data = undo::Data::from_str(json_data)?;
    let shadowenv = Shadowenv::new(
        env::vars().collect(),
        data,
        targets_hash.unwrap_or(0),
        clobber,
    );

    match targets {
        Some(targets) => {
            // run_program takes in the shadowenv, evaluates the code we found on it, and returns it
            match ShadowLang::run_programs(shadowenv, targets) {
                // no need to return anything descriptive here since we already
                // had ketos print it to stderr.
                Err(_) => Err(lang::ShadowlispError {}.into()),
                // note the "true" since we ran code to activate/modify the shadowenv
                Ok(shadowenv) => Ok(Some(shadowenv)),
            }
        }
        // note the "false" since we didn't have anything to run
        None => Ok(Some(shadowenv)),
    }
}

/// Load all Sources from the current dir, ensuring that they are all trusted.
fn load_trusted_sources(
    pathbuf: PathBuf,
    skip_trust_check: bool,
) -> Result<Option<SourceList>, Error> {
    #[cfg(not(test))]
    assert!(!skip_trust_check);

    let roots = loader::find_shadowenv_paths(&pathbuf)?;
    if roots.is_empty() {
        return Ok(None);
    }

    if !skip_trust_check {
        ensure_dir_tree_trusted(&roots)?;
    }

    let mut source_list = SourceList::new();
    for root in roots {
        let source = loader::load(root)?;
        if let Some(source) = source {
            source_list.prepend_source(source);
        }
    }

    if source_list.is_empty() {
        return Ok(None);
    }

    Ok(Some(source_list))
}

pub fn mutate_own_env(shadowenv: &Shadowenv) -> Result<(), Error> {
    for (k, v) in shadowenv.exports()? {
        match v {
            Some(s) => env::set_var(k, &s),
            None => env::remove_var(k),
        }
    }

    Ok(())
}

pub fn apply_env(shadowenv: &Shadowenv, mode: VariableOutputMode) -> Result<(), Error> {
    match mode {
        VariableOutputMode::Posix => {
            for (k, v) in shadowenv.exports()? {
                match v {
                    Some(s) => println!("export {}={}", shell_escape(&k), shell_escape(&s)),
                    None => println!("unset {}", shell_escape(&k)),
                }
            }
            output::print_activation_to_tty(
                shadowenv.current_dirs(),
                shadowenv.prev_dirs(),
                shadowenv.features(),
            );
        }
        VariableOutputMode::Fish => {
            for (k, v) in shadowenv.exports()? {
                match v {
                    Some(s) => {
                        if k == "PATH" {
                            let pathlist = shell_escape(&s).replace(":", "' '");
                            println!("set -gx {} {}", shell_escape(&k), pathlist);
                        } else {
                            println!("set -gx {} {}", shell_escape(&k), shell_escape(&s));
                        }
                    }
                    None => {
                        println!("set -e {}", shell_escape(&k));
                    }
                }
            }
            output::print_activation_to_tty(
                shadowenv.current_dirs(),
                shadowenv.prev_dirs(),
                shadowenv.features(),
            );
        }
        VariableOutputMode::Porcelain => {
            // three fields: <operation> : <name> : <value>
            // opcodes: 1: set, unexported (unused)
            //          2: set, exported
            //          3: unset (value is empty)
            // field separator is 0x1F; record separator is 0x1E. There's a trailing record
            // separator because I'm lazy but don't depend on it not going away.
            for (k, v) in shadowenv.exports()? {
                match v {
                    Some(s) => print!("\x02\x1F{}\x1F{}\x1E", k, s),
                    None => print!("\x03\x1F{}\x1F\x1E", k),
                }
            }
        }
        VariableOutputMode::Json => {
            let modifs = Modifications::new(shadowenv.exports()?);
            println!("{}", serde_json::to_string(&modifs).unwrap());
        }
        VariableOutputMode::PrettyJson => {
            let modifs = Modifications::new(shadowenv.exports()?);
            println!("{}", serde_json::to_string_pretty(&modifs).unwrap());
        }
    }
    Ok(())
}

fn shell_escape(s: &str) -> String {
    shell::escape(Cow::from(s)).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::undo::Data;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_trusted_source_returns_an_error_for_untrusted_folders() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join(".shadowenv.d");
        fs::create_dir(&path).unwrap();
        let result = load_trusted_sources(path.clone(), false);
        assert!(result.is_err());
        assert_eq!(format!("directory: '{}' contains untrusted shadowenv program: `shadowenv help trust` to learn more.", path.canonicalize().unwrap().to_string_lossy()), result.err().unwrap().to_string())
    }

    #[test]
    fn load_trusted_sources_returns_nearest_sources_last() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create test directories and files
        fs::create_dir_all(base_path.join("dir1/.shadowenv.d")).unwrap();
        fs::create_dir_all(base_path.join("dir1/dir2/.shadowenv.d")).unwrap();

        // Link the two shadowenvs.
        std::os::unix::fs::symlink(
            base_path.join("dir1/.shadowenv.d"),
            base_path.join("dir1/dir2/.shadowenv.d/parent"),
        )
        .unwrap();

        fs::write(
            base_path.join("dir1/.shadowenv.d/test.lisp"),
            "(env/set \"ORDER\" \"1\")",
        )
        .unwrap();
        fs::write(
            base_path.join("dir1/dir2/.shadowenv.d/test.lisp"),
            "(env/set \"ORDER\" \"2\")",
        )
        .unwrap();

        let result = load_trusted_sources(base_path.join("dir1/dir2"), true)
            .unwrap()
            .unwrap();

        let sources = result.consume();
        assert_eq!(sources.len(), 2);

        // Assert that sources are returned in the correct order
        // The order they are returned is the order they are executed in.
        // So the outermost env must come first, with the innermost dir coming last.
        assert!(sources[0].dir.ends_with("dir1"));
        assert!(sources[1].dir.ends_with("dir1/dir2"));
    }

    #[test]
    fn test_apply_env_escapes_variable_names() {
        // Test that shell_escape properly escapes dangerous characters
        assert_eq!(shell_escape("normal_var"), "normal_var");
        assert_eq!(
            shell_escape("TEST=AA; touch pwned.txt; #"),
            "'TEST=AA; touch pwned.txt; #'"
        );
        assert_eq!(shell_escape("VAR$(command)"), "'VAR$(command)'");
        assert_eq!(shell_escape("VAR`command`"), "'VAR`command`'");
        assert_eq!(shell_escape("VAR'with'quotes"), "'VAR'\\''with'\\''quotes'");

        // Create a shadowenv with potentially malicious variable names as new variables
        let initial_env = HashMap::new(); // Empty initial env
        let mut env = initial_env.clone();
        env.insert("NORMAL_VAR".to_string(), "normal_value".to_string());
        env.insert(
            "TEST=AA; touch pwned.txt; #".to_string(),
            "value".to_string(),
        );

        let mut shadowenv = Shadowenv::new(initial_env, Data::new(), 0, false);
        // Set the variables using the shadowenv API
        shadowenv.set("NORMAL_VAR", Some("normal_value"));
        shadowenv.set("TEST=AA; touch pwned.txt; #", Some("value"));

        // Test that exports returns the expected data
        let exports = shadowenv.exports().unwrap();
        assert_eq!(
            exports.get("NORMAL_VAR"),
            Some(&Some("normal_value".to_string()))
        );
        assert_eq!(
            exports.get("TEST=AA; touch pwned.txt; #"),
            Some(&Some("value".to_string()))
        );
    }

    #[test]
    fn test_shell_escape_comprehensive() {
        // Test various shell metacharacters that could lead to command injection
        let test_cases = vec![
            ("simple", "simple"),
            ("with spaces", "'with spaces'"),
            ("with;semicolon", "'with;semicolon'"),
            ("with|pipe", "'with|pipe'"),
            ("with&ampersand", "'with&ampersand'"),
            ("with>redirect", "'with>redirect'"),
            ("with<redirect", "'with<redirect'"),
            ("with$variable", "'with$variable'"),
            ("with`backtick`", "'with`backtick`'"),
            ("with$(command)", "'with$(command)'"),
            ("with${variable}", "'with${variable}'"),
            ("with'quote", "'with'\\''quote'"),
            ("with\"doublequote", "'with\"doublequote'"),
            ("with\\backslash", "'with\\backslash'"),
            ("with\nnewline", "'with\nnewline'"),
            ("with\ttab", "'with\ttab'"),
            ("with#comment", "'with#comment'"),
            ("with!history", "'with'\\!'history'"), // shell-escape also escapes !
            ("with*glob", "'with*glob'"),
            ("with?glob", "'with?glob'"),
            ("with[bracket", "'with[bracket'"),
            ("with]bracket", "'with]bracket'"),
            ("with(paren", "'with(paren'"),
            ("with)paren", "'with)paren'"),
            ("with{brace", "'with{brace'"),
            ("with}brace", "'with}brace'"),
            ("with~tilde", "'with~tilde'"),
            (
                "complex; echo 'pwned' > /tmp/pwned.txt #",
                "'complex; echo '\\''pwned'\\'' > /tmp/pwned.txt #'",
            ),
        ];

        for (input, expected) in test_cases {
            assert_eq!(shell_escape(input), expected, "Failed for input: {}", input);
        }
    }
}

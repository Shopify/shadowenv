use crate::{
    cli::HookCmd,
    get_current_dir_or_exit,
    hash::{Hash, SourceList},
    lang::{self, ShadowLang},
    loader, output,
    shadowenv::{validate_var_name, Shadowenv},
    trust::ensure_dir_tree_trusted,
    undo, unsafe_getppid,
};
use anyhow::{anyhow, Error};
use serde_derive::Serialize;
use shell_escape as shell;
use std::{
    borrow::Cow, collections::HashMap, env, io::Write, path::PathBuf, result::Result, str::FromStr,
};

#[derive(Clone, Copy)]
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
        None => Ok(None),
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
        // `env::set_var` panics on a name containing '=' or NUL. Names are
        // rejected when a program assigns them, but a $__shadowenv_data written
        // by an older version can still carry one, so skip rather than abort.
        if let Err(err) = validate_var_name(&k) {
            eprintln!("shadowenv: skipping variable: {}", err);
            continue;
        }
        match v {
            Some(s) => env::set_var(k, &s),
            None => env::remove_var(k),
        }
    }

    Ok(())
}

pub fn apply_env(shadowenv: &Shadowenv, mode: VariableOutputMode) -> Result<(), Error> {
    let exports = shadowenv.exports()?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    write_exports(&mut out, &exports, mode)?;
    out.flush()?;

    match mode {
        VariableOutputMode::Posix | VariableOutputMode::Fish => {
            output::print_activation_to_tty(
                shadowenv.current_dirs(),
                shadowenv.prev_dirs(),
                shadowenv.features(),
            );
        }
        VariableOutputMode::Porcelain
        | VariableOutputMode::Json
        | VariableOutputMode::PrettyJson => {}
    }

    Ok(())
}

/// Render the variable assignments for `mode`. Separate from `apply_env` so
/// that tests can assert on the exact bytes each mode produces: the escaping
/// rules differ per mode and are only correct if checked at this boundary.
fn write_exports<W: Write>(
    out: &mut W,
    exports: &HashMap<String, Option<String>>,
    mode: VariableOutputMode,
) -> Result<(), Error> {
    match mode {
        VariableOutputMode::Posix => {
            for (k, v) in exports {
                match v {
                    Some(s) => writeln!(out, "export {}={}", shell_escape(k), shell_escape(s))?,
                    None => writeln!(out, "unset {}", shell_escape(k))?,
                }
            }
        }
        VariableOutputMode::Fish => {
            for (k, v) in exports {
                match v {
                    Some(s) => {
                        if k == "PATH" {
                            writeln!(
                                out,
                                "set -gx {} (string split : -- {})",
                                shell_escape(k),
                                shell_escape(s)
                            )?;
                        } else {
                            writeln!(out, "set -gx {} {}", shell_escape(k), shell_escape(s))?;
                        }
                    }
                    None => {
                        writeln!(out, "set -e {}", shell_escape(k))?;
                    }
                }
            }
        }
        VariableOutputMode::Porcelain => {
            // three fields: <operation> : <name> : <value>
            // opcodes: 1: set, unexported (unused)
            //          2: set, exported
            //          3: unset (value is empty)
            // field separator is 0x1F; record separator is 0x1E. There's a trailing record
            // separator because I'm lazy but don't depend on it not going away.
            //
            // Names are NOT shell-escaped here: this is a binary protocol, not
            // something a shell evaluates, and quoting a name would make the
            // quotes part of the name a consumer reads back. What consumers do
            // need is the guarantee that a name never contains a separator, so
            // that records stay parseable positionally. Names are rejected at
            // assignment time; this also drops any that survive in a
            // $__shadowenv_data written by an older version.
            for (k, v) in exports {
                if let Err(err) = validate_var_name(k) {
                    eprintln!(
                        "shadowenv: omitting variable from porcelain output: {}",
                        err
                    );
                    continue;
                }
                match v {
                    Some(s) => write!(out, "\x02\x1F{}\x1F{}\x1E", k, s)?,
                    None => write!(out, "\x03\x1F{}\x1F\x1E", k)?,
                }
            }
        }
        VariableOutputMode::Json => {
            let modifs = Modifications::new(exports.clone());
            writeln!(out, "{}", serde_json::to_string(&modifs).unwrap())?;
        }
        VariableOutputMode::PrettyJson => {
            let modifs = Modifications::new(exports.clone());
            writeln!(out, "{}", serde_json::to_string_pretty(&modifs).unwrap())?;
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
    use VariableOutputMode::{Fish, Porcelain, Posix};

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

    fn render(exports: &[(&str, Option<&str>)], mode: VariableOutputMode) -> String {
        let map: HashMap<String, Option<String>> = exports
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(|s| s.to_string())))
            .collect();
        let mut buf: Vec<u8> = Vec::new();
        write_exports(&mut buf, &map, mode).unwrap();
        String::from_utf8(buf).unwrap()
    }

    /// Records are emitted in HashMap order, so compare them as a set.
    fn porcelain_records(out: &str) -> Vec<String> {
        let mut records: Vec<String> = out
            .split('\x1e')
            .filter(|r| !r.is_empty())
            .map(|r| r.to_string())
            .collect();
        records.sort();
        records
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

        // The shell-evaluated modes must quote the *name* as well as the value,
        // or a name carrying shell metacharacters is reinterpreted by the shell
        // that evaluates this output.
        let out = render(&[("TEST=AA; touch pwned.txt; #", Some("value"))], Posix);
        assert_eq!(out, "export 'TEST=AA; touch pwned.txt; #'=value\n");

        let out = render(&[("VAR$(command)", None)], Posix);
        assert_eq!(out, "unset 'VAR$(command)'\n");

        let out = render(&[("VAR`command`", Some("v"))], Fish);
        assert_eq!(out, "set -gx 'VAR`command`' v\n");
    }

    #[test]
    fn test_porcelain_emits_names_verbatim() {
        // Porcelain is delimiter-framed and never shell-evaluated, so a name is
        // emitted as-is. Quoting it here would make the quotes part of the name.
        let out = render(&[("FOO", Some("bar"))], Porcelain);
        assert_eq!(out, "\x02\x1FFOO\x1Fbar\x1E");

        let out = render(&[("FOO", None)], Porcelain);
        assert_eq!(out, "\x03\x1FFOO\x1F\x1E");

        // A name that would need quoting in a shell is still passed through.
        let out = render(&[("VAR$(command)", Some("v"))], Porcelain);
        assert_eq!(out, "\x02\x1FVAR$(command)\x1Fv\x1E");
    }

    #[test]
    fn test_porcelain_never_emits_separators_inside_a_name() {
        // A name containing a separator truncates its own record and adds
        // spurious ones, so it must never reach the stream: consumers parse
        // positionally and cannot recover the framing themselves.
        let unrepresentable = "AA\x1e\x02\x1fSPURIOUS\x1fyes";
        let out = render(
            &[(unrepresentable, Some("v")), ("GOOD", Some("g"))],
            Porcelain,
        );

        assert_eq!(
            porcelain_records(&out),
            vec!["\x02\x1FGOOD\x1Fg".to_string()]
        );
        assert!(!out.contains("SPURIOUS"));

        // Every emitted record has exactly the three fields the protocol defines.
        for record in porcelain_records(&out) {
            assert_eq!(record.split('\x1f').count(), 3, "record: {:?}", record);
        }
    }

    #[test]
    fn test_porcelain_record_count_matches_variable_count() {
        let out = render(
            &[("A", Some("1")), ("B", None), ("C", Some("3"))],
            Porcelain,
        );
        assert_eq!(porcelain_records(&out).len(), 3);
    }

    #[test]
    fn test_set_rejects_names_that_break_the_porcelain_protocol() {
        let mut shadowenv = Shadowenv::new(HashMap::new(), Data::new(), 0, false);

        assert!(shadowenv.set("NORMAL_VAR", Some("normal_value")).is_ok());
        // Unusual but harmless: representable in every output mode.
        assert!(shadowenv.set("TEST; touch pwned.txt; #", Some("v")).is_ok());

        for bad in ["AA\x1eBB", "AA\x1fBB", "TEST=AA", "AA\nBB", "AA\0BB", ""] {
            assert!(
                shadowenv.set(bad, Some("value")).is_err(),
                "expected {:?} to be rejected",
                bad
            );
        }

        // Rejected names must not be left behind in the environment.
        let exports = shadowenv.exports().unwrap();
        assert_eq!(
            exports.get("NORMAL_VAR"),
            Some(&Some("normal_value".to_string()))
        );
        assert!(exports.keys().all(|k| !k.contains('\x1e')));
    }

    #[test]
    fn test_pathlist_helpers_also_reject_unrepresentable_names() {
        let mut shadowenv = Shadowenv::new(HashMap::new(), Data::new(), 0, false);

        assert!(shadowenv.append_to_pathlist("AA\x1eBB", "/x").is_err());
        assert!(shadowenv.prepend_to_pathlist("AA\x1fBB", "/x").is_err());
        assert!(shadowenv.remove_from_pathlist("AA=BB", "/x").is_err());
        assert!(shadowenv
            .remove_from_pathlist_containing("AA\nBB", "/x")
            .is_err());

        assert!(shadowenv.append_to_pathlist("PATH", "/x").is_ok());
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

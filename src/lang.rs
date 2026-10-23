//! Shadowlisp: the small Lisp that `.shadowenv.d/*.lisp` programs are written in.
//!
//! The language is exactly what docs/shadowlisp.md and shadowlisp(5) describe. Programs are read
//! ([`reader`]), resolved against the builtins ([`compile`]), and only then evaluated ([`eval`]),
//! so a program that refers to anything else is rejected before any program runs.

mod compile;
mod eval;
mod reader;

use crate::{
    hash::{SourceFile, SourceList},
    shadowenv::Shadowenv,
};
use std::{fmt, fs, path::PathBuf};

pub struct ShadowLang {}

/// Byte offsets into a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// An error at a position in the file being read, compiled, or run.
#[derive(Debug)]
pub struct Fault {
    pub span: Span,
    pub message: String,
}

impl Fault {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

/// A Shadowlisp error, located in the file that caused it.
#[derive(Debug)]
pub struct Error {
    file: String,
    line: usize,
    column: usize,
    /// The offending line, or the part of it around the error.
    excerpt: String,
    /// Characters of `excerpt` before the error.
    caret: usize,
    underline: usize,
    message: String,
    /// Whether the file failed to parse, as opposed to compile or run.
    syntax: bool,
}

/// Most characters of the offending line quoted in an error; source lines can be up to the
/// loader's whole size limit long.
const EXCERPT_CHARS: usize = 120;

impl Error {
    fn new(file: &SourceFile, fault: Fault, syntax: bool) -> Self {
        let src = &file.contents;
        let line_of = |pos: usize| src[..pos].rfind('\n').map_or(0, |i| i + 1);
        let start = fault.span.start.min(src.len());
        let line_start = line_of(start);
        let line_end = src[start..].find('\n').map_or(src.len(), |i| start + i);
        let source_line = src[line_start..line_end].trim_end_matches('\r');
        // The error's start sits on this line; if it is on a trailing '\r', treat it as the
        // line's end. `end` stays within the line and never before `start`.
        let start = start.min(line_start + source_line.len());
        let end = fault
            .span
            .end
            .max(start)
            .min(line_start + source_line.len());
        let column = src[line_start..start].chars().count();
        let line_len = source_line.chars().count();
        let (skip, take) = if line_len <= EXCERPT_CHARS {
            (0, line_len)
        } else {
            let skip = column
                .saturating_sub(EXCERPT_CHARS / 3)
                .min(line_len - EXCERPT_CHARS);
            (skip, EXCERPT_CHARS)
        };
        let mut excerpt: String = source_line.chars().skip(skip).take(take).collect();
        let mut caret = column - skip;
        let underline = src[start..end]
            .chars()
            .count()
            .min(take.saturating_sub(caret))
            .max(1);
        if skip > 0 {
            excerpt.insert(0, '…');
            caret += 1;
        }
        if skip + take < line_len {
            excerpt.push('…');
        }

        Self {
            file: file.name.clone(),
            line: src[..line_start].matches('\n').count() + 1,
            column: column + 1,
            excerpt,
            caret,
            underline,
            message: fault.message,
            syntax,
        }
    }

    /// The error's location, message and the offending line, without the closing summary.
    ///
    /// A syntax error's first line starts with the file name, `NNN_name.lisp:line:column:`, as
    /// it did under Ketos; other errors start with `error in`. dev matches that prefix on stderr
    /// to recognize a dev-generated file that doesn't parse and move it out of the way, and it
    /// must not do that for a file that parses but fails to run.
    pub fn detail(&self) -> String {
        let indent: String = self
            .excerpt
            .chars()
            .take(self.caret)
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();
        let prefix = if self.syntax { "" } else { "error in " };
        format!(
            "{prefix}{}:{}:{}: {}\n    {}\n    {indent}^{}",
            self.file,
            self.line,
            self.column,
            self.message,
            self.excerpt,
            "~".repeat(self.underline - 1)
        )
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.detail())?;
        f.write_str("error while evaluating shadowlisp")
    }
}

impl ShadowLang {
    pub fn run_programs(mut shadowenv: Shadowenv, sources: SourceList) -> Result<Shadowenv, Error> {
        let dirs = sources.shortened_dirs();

        let mut programs = Vec::new();
        for source in sources.consume() {
            // `expand-path` resolves relative paths against the directory containing the
            // `.shadowenv.d`, as the physical path (matching what `getcwd` reported when Ketos
            // evaluated programs with that directory as the working directory).
            let dir = fs::canonicalize(&source.dir).unwrap_or_else(|_| PathBuf::from(&source.dir));
            let mut files = source.files;
            files.sort();
            for file in files {
                let program = reader::read(&file.contents)
                    .map_err(|fault| Error::new(&file, fault, true))
                    .and_then(|exprs| {
                        compile::compile(&exprs).map_err(|fault| Error::new(&file, fault, false))
                    })?;
                programs.push((dir.clone(), file, program));
            }
        }

        let mut machine = eval::Machine::new(&mut shadowenv);
        for (dir, file, program) in &programs {
            machine
                .run(program, dir)
                .map_err(|fault| Error::new(file, fault, false))?;
        }

        shadowenv.add_dirs(dirs);
        Ok(shadowenv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::Feature;
    use crate::hash::Source;
    use crate::undo::Data;
    use std::collections::{BTreeMap, HashMap, HashSet};

    fn build_source(content: &str) -> Source {
        build_source_named("file.lisp", content)
    }

    fn build_source_named(name: &str, content: &str) -> Source {
        Source {
            dir: "dir".to_string(),
            files: vec![SourceFile {
                name: name.to_string(),
                contents: content.to_string(),
            }],
        }
    }

    fn build_shadow_env(env_variables: Vec<(&str, &str)>) -> Shadowenv {
        let env = env_variables
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>();
        Shadowenv::new(env, Data::new(), 0, false)
    }

    fn run(program: &str) -> Result<Shadowenv, Error> {
        ShadowLang::run_programs(
            build_shadow_env(vec![("PRESET", "p")]),
            SourceList::new_with_sources(vec![build_source(program)]),
        )
    }

    /// Runs `(env/set "RESULT" <expr>)` and returns the resulting value of `RESULT`.
    fn result_of(expr: &str) -> Result<Option<String>, String> {
        run(&format!(r#"(env/set "RESULT" {expr})"#))
            .map(|env| env.get("RESULT"))
            .map_err(|err| err.message)
    }

    #[test]
    fn test_env_manipulation() {
        let shadowenv = build_shadow_env(vec![]);

        let source = build_source(
            r#"
            (env/set "VAL_A" "42")
            "#,
        );

        let result =
            ShadowLang::run_programs(shadowenv, SourceList::new_with_sources(vec![source]));
        let env = result.unwrap().exports().unwrap();

        assert_eq!(env["VAL_A"].as_ref().unwrap(), "42");
    }

    #[test]
    fn test_env_set_rejects_names_containing_porcelain_separators() {
        // Names come from shadowlisp, which can produce any text. 0x1E/0x1F are
        // the porcelain record and field separators, so a name containing one
        // corrupts the framing of the output editor integrations parse.
        for bad in [
            r#"(env/set "AA\x1e\x02\x1fSPURIOUS\x1fyes" "v")"#,
            r#"(env/set "AA\x1fBB" "v")"#,
            r#"(env/set "TEST=AA" "v")"#,
            r#"(env/append-to-pathlist "AA\x1eBB" "/x")"#,
        ] {
            assert!(run(bad).is_err(), "expected {} to be rejected", bad);
        }
    }

    #[test]
    fn test_env_set_still_allows_unusual_but_representable_names() {
        let env = run(r#"(env/set "weird.name-with$stuff" "42")"#).unwrap();
        assert_eq!(env.get("weird.name-with$stuff").as_deref(), Some("42"));
    }

    #[test]
    fn test_pathlist_manipulation() {
        let shadowenv = build_shadow_env(vec![
            ("PATH_A", "/path1:/path2"),
            ("PATH_B", "/path3"),
            ("PATH_C", "/will_be_removed_path:/path5"),
        ]);

        let source = build_source(
            r#"
                (env/prepend-to-pathlist "PATH_A" "/path3")
                (env/prepend-to-pathlist "PATH_B" "/path7")
                (env/remove-from-pathlist-containing "PATH_C" "/will_be_removed_path")
                (env/prepend-to-pathlist "PATH_C" "/will_be_added_path")
            "#,
        );

        let result =
            ShadowLang::run_programs(shadowenv, SourceList::new_with_sources(vec![source]));
        let env = result.unwrap().exports().unwrap();

        assert_eq!(env["PATH_A"].as_ref().unwrap(), "/path3:/path1:/path2");
        assert_eq!(env["PATH_B"].as_ref().unwrap(), "/path7:/path3");
        assert_eq!(
            env["PATH_C"].as_ref().unwrap(),
            "/will_be_added_path:/path5"
        );
    }

    #[test]
    fn test_set_variables() {
        let shadowenv = build_shadow_env(vec![
            ("GEM_HOME", "/gem_home"),
            ("PATH", "/gem_home/bin:/something_else"),
        ]);

        let source = build_source(
            r#"
                (when-let ((gem-home (env/get "GEM_HOME")))
                (env/remove-from-pathlist "PATH" (path-concat gem-home "bin")))
            "#,
        );

        let result =
            ShadowLang::run_programs(shadowenv, SourceList::new_with_sources(vec![source]));
        let env = result.unwrap().exports().unwrap();

        assert_eq!(env["PATH"].as_ref().unwrap(), "/something_else");
    }

    #[test]
    fn test_features() {
        let shadowenv = run(r#"(provide "ruby" "3.1.2") (provide "rust")"#).unwrap();
        let expected = HashSet::from([
            Feature::new("ruby".to_string(), Some("3.1.2".to_string())),
            Feature::new("rust".to_string(), None),
        ]);
        assert_eq!(shadowenv.features(), expected);
    }

    #[test]
    fn test_expand_path() {
        let home = dirs::home_dir().map(|p| p.into_os_string().into_string().unwrap());
        let shadowenv = run(r#"(env/set "EXPANDED" (expand-path "~"))"#).unwrap();
        assert_eq!(shadowenv.get("EXPANDED"), home);
    }

    #[test]
    fn test_expand_path_resolves_relative_to_the_shadowenv_parent_physically() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let source = Source {
            dir: link.to_string_lossy().into_owned(),
            files: vec![SourceFile {
                name: "file.lisp".to_string(),
                contents: r#"(env/set "BIN" (expand-path "./bin/../sbin"))"#.to_string(),
            }],
        };
        let shadowenv = ShadowLang::run_programs(
            build_shadow_env(vec![]),
            SourceList::new_with_sources(vec![source]),
        )
        .unwrap();
        let expected = std::fs::canonicalize(&real).unwrap().join("sbin");
        assert_eq!(
            shadowenv.get("BIN"),
            Some(expected.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn test_source_ordering() {
        let shadowenv = build_shadow_env(vec![]);

        let outer_source = build_source(
            r#"
                (env/set "TEST" "ONE")
            "#,
        );
        let inner_source = build_source(
            r#"
                (env/set "TEST" "TWO")
            "#,
        );

        // Outer source comes first, as shown in test load_trusted_sources_returns_nearest_sources_last
        // The source that comes last in the input list should be executed last
        let shadowenv = ShadowLang::run_programs(
            shadowenv,
            SourceList::new_with_sources(vec![outer_source, inner_source]),
        )
        .unwrap();
        assert_eq!(shadowenv.get("TEST"), Some("TWO".to_string()));
    }

    #[test]
    fn test_every_documented_form_runs() {
        let shadowenv = run(r#"
            (env/set "A" "1")
            (let ((items (append (list "x" "y") "z")))
              (do
                (env/set "JOINED" (join ":" (first items) (elt items 1) (last items)))
                (env/set "TAIL" (first (tail (concat items (list "w")))))))
            (when (and (eq (env/get "A") "1") (ne (env/get "A") "2") (not (null (env/get "A"))))
              (env/set "WHEN" "yes"))
            (when-let ((a (env/get "A")))
              (env/set "WHEN_LET" a))
            (env/set "IF" (if (or (null (env/get "MISSING")) false) "then" "else"))
            (env/set "COND" (cond ((not (null (env/get "MISSING"))) "missing") (else "fallback")))
            (env/prepend-to-pathlist "P" "/b")
            (env/prepend-to-pathlist "P" "/a")
            (env/append-to-pathlist "P" "/c")
            (env/prepend-to-pathlist "P" "/rvm/bin")
            (env/remove-from-pathlist "P" "/b")
            (env/remove-from-pathlist-containing "P" "rvm")
            (env/set "CONCAT" (path-concat "/a" "b"))
            (env/set "EXPAND" (expand-path "/a/../b"))
            (provide "thing" "1.0")
            "#)
        .unwrap();
        for (name, value) in [
            ("JOINED", "x:y:z"),
            ("TAIL", "y"),
            ("WHEN", "yes"),
            ("WHEN_LET", "1"),
            ("IF", "then"),
            ("COND", "fallback"),
            ("P", "/a:/c"),
            ("CONCAT", "/a/b"),
            ("EXPAND", "/b"),
        ] {
            assert_eq!(shadowenv.get(name).as_deref(), Some(value), "{name}");
        }
    }

    #[test]
    fn test_values_match_ketos() {
        for (expr, expected) in [
            // `and`/`or` return their last operand as is.
            (r#"(and true "last")"#, Some("last")),
            (r#"(or false "last")"#, Some("last")),
            (r#"(if (and) "t" "f")"#, Some("t")),
            (r#"(if (or) "t" "f")"#, Some("f")),
            // `eq`/`ne` compare structurally; values of different types are unequal.
            (
                r#"(if (eq (list "a" (list "b")) (list "a" (list "b"))) "t" "f")"#,
                Some("t"),
            ),
            (r#"(if (eq "a" ()) "t" "f")"#, Some("f")),
            (r#"(if (eq "a" "a" "b") "t" "f")"#, Some("f")),
            (r#"(if (ne "a" "b" "c") "t" "f")"#, Some("t")),
            (r#"(if (ne "a" "b" "a") "t" "f")"#, Some("f")),
            // Ketos's own functions compare by identity; shadowenv's compare with nothing.
            (r#"(if (eq list list) "t" "f")"#, Some("t")),
            (r#"(if (eq env/set env/set) "t" "f")"#, Some("f")),
            (r#"(if (ne env/set env/set) "t" "f")"#, Some("t")),
            // `=` is strict: equal only if every value matches the first.
            (r#"(if (= "" "") "t" "f")"#, Some("t")),
            (r#"(if (= "a" "a" "b") "t" "f")"#, Some("f")),
            (r#"(if (= (list 1 "a") (list 1 "a")) "t" "f")"#, Some("t")),
            (r#"(if (= "a" "b" ()) "t" "f")"#, Some("f")),
            // Strings concatenate and join; so do lists.
            (r#"(concat "a" "b" "c")"#, Some("abc")),
            (r#"(join "" "a" "b")"#, Some("ab")),
            (r#"(join "-")"#, Some("")),
            (
                r#"(first (join (list "-") (list "a") (list "b")))"#,
                Some("a"),
            ),
            (
                r#"(elt (join (list "-") (list "a") (list "b")) 1)"#,
                Some("-"),
            ),
            (r#"(first (concat () (list "x")))"#, Some("x")),
            // A single value concatenates to itself.
            (r#"(join "" ())"#, None),
            (r#"(tail (list "only"))"#, None),
            (r#"(path-concat)"#, Some(".")),
            (r#"(path-concat "/a/" "./b" "../c")"#, Some("/a/c")),
            (r#"(cond ((eq 1 2) "no"))"#, None),
            (r#"(when false "no")"#, None),
            (r#"(do)"#, None),
            // Arity is checked when a call runs, so a mistake in an untaken branch is harmless.
            (
                r#"(if false (env/remove-from-pathlist "PATH") "ok")"#,
                Some("ok"),
            ),
            // Locals shadow builtins and special forms; builtins are values.
            (r#"(let ((list "shadowed")) list)"#, Some("shadowed")),
            (r#"(let ((if "shadowed")) if)"#, Some("shadowed")),
            (r#"(let ((f env/get)) (f "PRESET"))"#, Some("p")),
            (r#"(let ((a "1") (b (concat a "2"))) b)"#, Some("12")),
            (
                r#"(let ((a "outer")) (let ((a "inner")) a))"#,
                Some("inner"),
            ),
            // `when-let` runs its body only if the first binding is non-`()`.
            (r#"(when-let ((a (env/get "PRESET")) (b ())) a)"#, Some("p")),
            (r#"(when-let ((a ()) (b "x")) b)"#, None),
            // The first binding's *name* decides, so a later binding of the same name wins.
            (r#"(when-let ((a "x") (a ())) "ran")"#, None),
            (r#"(when-let ((a ()) (a "y")) a)"#, Some("y")),
            // `else` is the catch-all even when bound locally.
            (r#"(let ((else false)) (cond (else "else")))"#, Some("else")),
        ] {
            assert_eq!(result_of(expr), Ok(expected.map(String::from)), "{expr}");
        }
    }

    #[test]
    fn test_errors_match_ketos() {
        for (expr, expected) in [
            // Conditions must be booleans.
            (
                r#"(if "x" "t" "f")"#,
                "type error: expected bool; found string: \"x\"",
            ),
            (
                r#"(when () "t")"#,
                "type error: expected bool; found (): ()",
            ),
            (
                r#"(and "x" true)"#,
                "type error: expected bool; found string: \"x\"",
            ),
            (r#"(not ())"#, "type error: expected bool; found (): ()"),
            // Comparing values of different types with `=` is an error.
            (
                r#"(= "" ())"#,
                "type error: cannot compare string: \"\" with (): ()",
            ),
            (r#"(first ())"#, "type error: expected list; found (): ()"),
            (
                r#"(elt (list "a") 1)"#,
                "index 1 is out of bounds for a list of 1",
            ),
            (
                r#"(elt (list "a") -1)"#,
                "type error: expected non-negative integer; found integer: -1",
            ),
            (
                r#"(concat () "x")"#,
                "type error: expected list; found string: \"x\"",
            ),
            (
                r#"(env/get "A" "B")"#,
                "`env/get` expects 1 argument; found 2",
            ),
            (
                r#"(provide "x" 1)"#,
                "type error: expected string; found integer: 1",
            ),
            (
                r#"(let ((f env/get)) (f))"#,
                "`env/get` expects 1 argument; found 0",
            ),
            (
                r#"(let ((f "not a function")) (f))"#,
                "type error: expected function; found string: \"not a function\"",
            ),
            (
                r#"(expand-path "")"#,
                "cannot expand \"\": cannot make an empty path absolute",
            ),
            (
                r#"(when false ("literal" 1))"#,
                "invalid call: expected a function name or expression",
            ),
        ] {
            assert_eq!(result_of(expr), Err(expected.to_string()), "{expr}");
        }
    }

    #[test]
    fn test_rejects_what_ketos_accepted_but_shadowlisp_does_not_document() {
        for (expr, expected) in [
            // Ketos silently unset the variable for any non-string value.
            ("1", "type error: expected string or (); found integer: 1"),
            // Ketos returned a character, which Shadowlisp has no type for.
            (
                r#"(first "abc")"#,
                "type error: expected list; found string: \"abc\"",
            ),
            // Ketos panicked.
            (
                r#"(path-concat "a" 1)"#,
                "type error: expected string; found integer: 1",
            ),
        ] {
            assert_eq!(result_of(expr), Err(expected.to_string()), "{expr}");
        }
    }

    #[test]
    fn test_accepts_what_ketos_rejected() {
        // Ketos failed on an empty file, on a comment-only final line without a newline, and on
        // `;;` comments before anything but a definition.
        for program in [
            "",
            "; comment",
            ";; comment\n(env/set \"A\" \"1\")",
            "(when true)",
        ] {
            run(program).unwrap_or_else(|err| panic!("{program:?}: {err}"));
        }
    }

    #[test]
    fn test_rejects_undocumented_names_before_running_anything() {
        for (name, program) in [
            ("format", r#"(env/set "A" (format "{}" 1))"#),
            ("define", "(define x 1)"),
            ("lambda", r#"(env/set "A" ((lambda () "x")))"#),
            ("env", r#"(env/set "A" env)"#),
            ("shadowenv", r#"(env/get shadowenv)"#),
        ] {
            assert_eq!(
                result_of(&format!("(do {program})")).unwrap_err(),
                format!("`{name}` is not defined; Shadowlisp only provides the functions documented in shadowlisp(5)"),
            );
        }
        assert_eq!(
            result_of("if").unwrap_err(),
            "`if` is a special form and cannot be used as a value"
        );

        // A later file's undefined name prevents an earlier file from running at all.
        let mut good = build_source_named("100_good.lisp", r#"(env/set "A" "1")"#);
        good.files.push(SourceFile {
            name: "200_bad.lisp".to_string(),
            contents: "(print 1)".to_string(),
        });
        let err = ShadowLang::run_programs(
            build_shadow_env(vec![]),
            SourceList::new_with_sources(vec![good]),
        )
        .unwrap_err();
        assert_eq!(err.file, "200_bad.lisp");
    }

    #[test]
    fn test_rejects_malformed_forms() {
        for (expr, expected) in [
            ("(if true)", "malformed `if`: expected `(if condition then-case [ else-case ])`"),
            ("(let ((a \"1\")) a a)", "malformed `let`: expected `(let ((name expression) ...) body)`; wrap multiple body forms in `do`"),
            ("(let ((\"a\" \"1\")) ())", "malformed `let` binding: expected `(name expression)`"),
            ("(when-let () \"x\")", "malformed `when-let`: expected `(when-let ((name expression) ...) [ body ... ])`"),
            ("(cond (else \"a\") (true \"b\"))", "`else` must be the last `cond` clause"),
            ("(cond (true))", "malformed `cond` clause: expected `(predicate branch)`"),
            ("(do (let ((a \"1\")) ()) a)", "`a` is not defined; Shadowlisp only provides the functions documented in shadowlisp(5)"),
        ] {
            assert_eq!(result_of(expr).unwrap_err(), expected, "{expr}");
        }
    }

    #[test]
    fn test_error_display_points_at_the_source() {
        let err = ShadowLang::run_programs(
            build_shadow_env(vec![]),
            SourceList::new_with_sources(vec![build_source_named(
                "500_ruby.lisp",
                "(provide \"ruby\")\n  (env/set \"A\" (format \"x\"))\n",
            )]),
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "error in 500_ruby.lisp:2:17: `format` is not defined; Shadowlisp only provides the functions documented in shadowlisp(5)\n\
             \x20     (env/set \"A\" (format \"x\"))\n\
             \x20                   ^~~~~~\n\
             error while evaluating shadowlisp"
        );
    }

    #[test]
    fn test_only_syntax_errors_start_with_the_file_name() {
        // dev matches /\A(\d+_dev_\w+\.lisp):/ on stderr and moves a matching file aside as
        // one it generated broken. Under Ketos only a file that failed to parse produced that
        // prefix; a file that parses but fails to compile or run must not.
        let first_line = |src: &str| {
            ShadowLang::run_programs(
                build_shadow_env(vec![]),
                SourceList::new_with_sources(vec![build_source_named("200_dev_x.lisp", src)]),
            )
            .unwrap_err()
            .detail()
            .lines()
            .next()
            .unwrap()
            .to_string()
        };
        for syntax in [
            "(env/set \"A\" \"1\"",
            "(env/set \"A\" \"\\e\")",
            "(env/set \"A\" @)",
        ] {
            assert!(
                first_line(syntax).starts_with("200_dev_x.lisp:1:"),
                "{syntax}"
            );
        }
        for other in [
            "(if)",
            "(env/sett \"A\" \"1\")",
            "(env/set \"A\" (concat \"x\" (env/get \"UNSET_X\")))",
        ] {
            assert!(
                first_line(other).starts_with("error in 200_dev_x.lisp:1:"),
                "{other}"
            );
        }
    }

    #[test]
    fn test_error_display_quotes_only_part_of_a_long_line() {
        let line = format!("(env/set \"A\" \"{}\" (format))", "x".repeat(10_000));
        let err = ShadowLang::run_programs(
            build_shadow_env(vec![]),
            SourceList::new_with_sources(vec![build_source(&line)]),
        )
        .unwrap_err();
        let shown = err.to_string();
        let excerpt = shown.lines().nth(1).unwrap();
        assert!(excerpt.chars().count() < 200, "{excerpt}");
        assert!(excerpt.ends_with("(format))"), "{excerpt}");
        let caret = shown.lines().nth(2).unwrap();
        let chars_before = |s: &str, i: usize| s[..i].chars().count();
        assert_eq!(
            chars_before(caret, caret.find('^').unwrap()),
            chars_before(excerpt, excerpt.find("format").unwrap())
        );
    }

    /// Doubles `seed` with `concat` `n` times in one `let`; each step is cheap to write.
    fn doubling(seed: &str, n: usize) -> String {
        let bindings: String = (1..=n)
            .map(|i| format!(" (a{i} (concat a{} a{}))", i - 1, i - 1))
            .collect();
        format!("(let ((a0 {seed}){bindings}) ())")
    }

    #[test]
    fn test_limits_bound_hostile_programs() {
        let too_large = format!(
            "value is larger than Shadowlisp's limit of {} bytes or elements",
            eval::MAX_VALUE_SIZE
        );
        // Without limits, each of these would exhaust memory or time.
        for (program, expected) in [
            (doubling(r#""xxxxxxxx""#, 64), too_large.as_str()),
            (doubling(r#"(list "x")"#, 64), too_large.as_str()),
            (
                format!(
                    "(let ((a0 (list \"x\")){}) ())",
                    (1..=100)
                        .map(|i| format!(" (a{i} (list a{}))", i - 1))
                        .collect::<String>()
                ),
                "lists may be nested at most 64 deep",
            ),
            (
                (0..20_000)
                    .map(|i| {
                        format!("(env/prepend-to-pathlist \"P\" \"/a/longish/directory/{i}\")\n")
                    })
                    .collect(),
                "program exceeded Shadowlisp's evaluation limit",
            ),
        ] {
            let err = run(&program).unwrap_err();
            assert_eq!(err.message, expected);
        }

        // The limits leave room for far larger programs than anyone writes.
        let doubled = run(&format!(
            r#"(let ((a0 "x"){}) (env/set "A" a16))"#,
            (1..=16)
                .map(|i| format!(" (a{i} (concat a{} a{}))", i - 1, i - 1))
                .collect::<String>()
        ))
        .unwrap();
        assert_eq!(doubled.get("A").map(|v| v.len()), Some(1 << 16));
        let bindings: String = (0..50_000).map(|i| format!(" (v{i} \"{i}\")")).collect();
        let many = run(&format!("(let ({bindings}) (env/set \"A\" v49999))")).unwrap();
        assert_eq!(many.get("A").as_deref(), Some("49999"));
    }

    #[test]
    fn test_rejects_values_the_environment_cannot_hold() {
        for (program, reason) in [
            (
                r#"(env/set "A" "a\0b")"#,
                "values may not contain a NUL byte",
            ),
            (
                r#"(env/set "A" "a\x1e\x02\x1fPATH\x1f/evil")"#,
                "values may not contain 0x1E, the porcelain record separator",
            ),
            (
                r#"(env/prepend-to-pathlist "PATH" "/x\x1f")"#,
                "values may not contain 0x1F, the porcelain field separator",
            ),
        ] {
            assert!(
                run(program).unwrap_err().message.ends_with(reason),
                "{program}"
            );
        }
    }

    /// Replays programs recorded under the Ketos interpreter this one replaced (see the fixture's
    /// description); each must produce exactly the environment Ketos did, or fail as Ketos did.
    #[test]
    fn test_matches_recorded_ketos_results() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/parity/ketos.json")).unwrap();
        let presets = fixture["presets"].as_array().unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() > 300);
        for (i, case) in cases.iter().enumerate() {
            let preset = case["preset"].as_u64().unwrap() as usize;
            let env: HashMap<String, String> =
                serde_json::from_value(presets[preset].clone()).unwrap();
            let files: BTreeMap<String, String> =
                serde_json::from_value(case["files"].clone()).unwrap();
            let expected: Option<HashMap<String, Option<String>>> =
                serde_json::from_value(case["expect"].clone()).unwrap();
            let source = Source {
                dir: "dir".to_string(),
                files: files
                    .iter()
                    .map(|(name, contents)| SourceFile {
                        name: name.clone(),
                        contents: contents.clone(),
                    })
                    .collect(),
            };
            let result = ShadowLang::run_programs(
                Shadowenv::new(env, Data::new(), 0, false),
                SourceList::new_with_sources(vec![source]),
            );
            match (result, expected) {
                (Ok(shadowenv), Some(expected)) => {
                    let mut exports = shadowenv.exports().unwrap();
                    exports.remove("__shadowenv_data");
                    assert_eq!(exports, expected, "case {i}: {files:?}");
                }
                (Err(_), None) => {}
                (Ok(_), None) => panic!("case {i}: Ketos failed, but this succeeded: {files:?}"),
                (Err(err), Some(_)) => panic!("case {i}: Ketos succeeded, but: {err}\n{files:?}"),
            }
        }
    }
}

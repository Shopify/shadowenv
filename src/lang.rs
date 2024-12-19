use crate::{
    ejson,
    hash::{Source, SourceList},
    shadowenv::Shadowenv,
};
use anyhow::anyhow;
use json_dotpath::DotPaths;
use ketos::{Context, Error, FromValueRef, Name, Value};
use ketos_derive::{ForeignValue, FromValueRef};
use std::{
    cell::{Ref, RefCell},
    collections::HashSet,
    env, fs,
    ops::DerefMut,
    path::{Path, PathBuf},
    rc::Rc,
};
use thiserror::Error;

pub struct ShadowLang {}

#[derive(Debug, Error)]
#[error("error while evaluating shadowlisp")]
pub struct ShadowlispError;

macro_rules! ketos_fn2 {
    ( $scope:expr => $name:expr => fn $ident:ident
            (...) -> $res:ty ) => {
        $scope.add_value_with_name($name, |name| {
            Value::new_foreign_fn(name, move |_scope, args| {
                let res = $ident(args)?;
                Ok(<$res as Into<Value>>::into(res))
            })
        })
    };
}

macro_rules! assert_args {
    ( $args:expr , $count:expr , $name:expr ) => {
        if $args.len() != $count {
            return Err(From::from(ketos::exec::ExecError::ArityError {
                name: Some($name),
                expected: ketos::function::Arity::Exact($count as u32),
                found: $args.len() as u32,
            }));
        }
    };
}

#[derive(Debug, ForeignValue, FromValueRef)]
// Sharing a value with Ketos means we can only access it through `&self`.
// Mutation of values is possible through internally mutable containers,
// such as `Cell` and `RefCell`.
struct RefCellWrapper<T>(RefCell<T>)
where
    T: std::fmt::Debug + 'static;

impl<T> RefCellWrapper<T>
where
    T: std::fmt::Debug + 'static,
{
    fn new(inner: T) -> Self {
        Self(RefCell::new(inner))
    }

    fn borrow_mut_env(&self) -> std::cell::RefMut<T> {
        self.0.borrow_mut()
    }

    fn borrow_env(&self) -> Ref<'_, T> {
        self.0.borrow()
    }

    fn into_inner(self) -> T {
        self.0.into_inner()
    }
}

fn get_value(ctx: &Context, value_name: Name) -> Value {
    ctx.scope()
        .get_constant(value_name)
        .expect("bug: value not defined")
}

fn path_concat(vals: &mut [Value]) -> Result<String, Error> {
    let res = vals.iter().fold(
        PathBuf::new(),
        |acc, v| acc.join(<&str as FromValueRef>::from_value_ref(v).unwrap()), // TODO(burke): don't unwrap
    );

    Ok(res.to_string_lossy().to_string())
}

impl ShadowLang {
    pub fn run_programs(
        shadowenv: Shadowenv,
        sources: &mut SourceList,
    ) -> Result<Shadowenv, Error> {
        let wrapper = Rc::new(RefCellWrapper::new(shadowenv));

        let dirs = sources.shortened_dirs();
        for source in sources.sources.iter_mut() {
            let ejson_tracker_wrapper = Rc::new(RefCellWrapper::new(HashSet::default()));
            Self::run(&wrapper, &ejson_tracker_wrapper, source)?;
            source
                .set_used_ejson_paths(Rc::try_unwrap(ejson_tracker_wrapper).unwrap().into_inner());
        }

        let mut result = Rc::try_unwrap(wrapper).unwrap().into_inner();
        result.add_dirs(dirs);

        Ok(result)
    }

    fn run(
        rc_wrapper: &Rc<RefCellWrapper<Shadowenv>>,
        ejson_tracker_wrapper: &Rc<RefCellWrapper<HashSet<PathBuf>>>,
        source: &mut Source,
    ) -> Result<(), Error> {
        let mut restrictions = ketos::RestrictConfig::strict();
        // "Maximum size of value stack, in values"
        // This also puts a cap on the size of string literals in a single function invocation.
        // The default limit of 256 then means a limit of 256 bytes of string per invocation.
        // We'll increase this to 8k, in case people want to embed an RSA cert or something (don't
        // construe this as an endorsement of that plan).
        restrictions.memory_limit = 8192;

        let interp = ketos::Builder::new()
            .restrict(restrictions)
            .io(Rc::new(ketos::GlobalIo::null())) // no printing
            .module_loader(Box::new(ketos::module::NullModuleLoader)) // nerf code loading
            .finish();

        let shadowenv_name = interp.scope().add_name("shadowenv");
        let ejson_tracker_name = interp.scope().add_name("used_ejson");

        interp
            .scope()
            .add_constant(shadowenv_name, Value::Foreign(rc_wrapper.clone()));

        interp.scope().add_constant(
            ejson_tracker_name,
            Value::Foreign(ejson_tracker_wrapper.clone()),
        );

        ketos_fn2! { interp.scope() => "path-concat" =>
        fn path_concat(...) -> String }

        interp.scope().add_value_with_name("env/get", |name| {
            Value::new_foreign_fn(name, move |ctx, args| {
                assert_args!(args, 1, name);

                let value = get_value(ctx, shadowenv_name);
                let wrapper: &RefCellWrapper<Shadowenv> = FromValueRef::from_value_ref(&value)?;
                let name = <&str as FromValueRef>::from_value_ref(&args[0])?;

                let result = wrapper
                    .borrow_env()
                    .get(name)
                    .map(<String as Into<Value>>::into)
                    .unwrap_or(Value::Unit);
                Ok(result)
            })
        });

        interp.scope().add_value_with_name("env/set", |name| {
            Value::new_foreign_fn(name, move |ctx, args| {
                assert_args!(args, 2, name);

                let value = get_value(ctx, shadowenv_name);
                let shadowenv =
                    <&RefCellWrapper<Shadowenv> as FromValueRef>::from_value_ref(&value)?;
                let name = <&str as FromValueRef>::from_value_ref(&args[0])?;
                let value = <&str as FromValueRef>::from_value_ref(&args[1]).ok();

                shadowenv.borrow_mut_env().set(name, value);
                Ok(Value::Unit)
            })
        });

        interp
            .scope()
            .add_value_with_name("env/append-to-pathlist", |name| {
                Value::new_foreign_fn(name, move |ctx, args| {
                    assert_args!(args, 2, name);

                    let value = get_value(ctx, shadowenv_name);
                    let wrapper: &RefCellWrapper<Shadowenv> = FromValueRef::from_value_ref(&value)?;
                    let name = <&str as FromValueRef>::from_value_ref(&args[0])?;
                    let value = <&str as FromValueRef>::from_value_ref(&args[1])?;

                    wrapper.borrow_mut_env().append_to_pathlist(name, value);
                    Ok(Value::Unit)
                })
            });

        interp
            .scope()
            .add_value_with_name("env/prepend-to-pathlist", |name| {
                Value::new_foreign_fn(name, move |ctx, args| {
                    assert_args!(args, 2, name);

                    let value = get_value(ctx, shadowenv_name);
                    let wrapper: &RefCellWrapper<Shadowenv> = FromValueRef::from_value_ref(&value)?;
                    let name = <&str as FromValueRef>::from_value_ref(&args[0])?;
                    let value = <&str as FromValueRef>::from_value_ref(&args[1])?;

                    wrapper.borrow_mut_env().prepend_to_pathlist(name, value);
                    Ok(Value::Unit)
                })
            });

        interp
            .scope()
            .add_value_with_name("env/remove-from-pathlist", |name| {
                Value::new_foreign_fn(name, move |ctx, args| {
                    assert_args!(args, 2, name);

                    let value = get_value(ctx, shadowenv_name);
                    let wrapper: &RefCellWrapper<Shadowenv> = FromValueRef::from_value_ref(&value)?;
                    let name = <&str as FromValueRef>::from_value_ref(&args[0])?;
                    let value = <&str as FromValueRef>::from_value_ref(&args[1])?;

                    wrapper.borrow_mut_env().remove_from_pathlist(name, value);
                    Ok(Value::Unit)
                })
            });

        interp
            .scope()
            .add_value_with_name("env/remove-from-pathlist-containing", |name| {
                Value::new_foreign_fn(name, move |ctx, args| {
                    assert_args!(args, 2, name);

                    let value = get_value(ctx, shadowenv_name);
                    let wrapper: &RefCellWrapper<Shadowenv> = FromValueRef::from_value_ref(&value)?;
                    let name = <&str as FromValueRef>::from_value_ref(&args[0])?;
                    let value = <&str as FromValueRef>::from_value_ref(&args[1])?;

                    wrapper
                        .borrow_mut_env()
                        .remove_from_pathlist_containing(name, value);
                    Ok(Value::Unit)
                })
            });

        interp.scope().add_value_with_name("provide", |name| {
            Value::new_foreign_fn(name, move |ctx, args| {
                let value = get_value(ctx, shadowenv_name);
                let wrapper: &RefCellWrapper<Shadowenv> = FromValueRef::from_value_ref(&value)?;

                let version = match args.len() {
                    1 => None,
                    2 => Some(<&str as FromValueRef>::from_value_ref(&args[1])?),
                    _ => {
                        return Err(From::from(ketos::exec::ExecError::ArityError {
                            name: Some(name),
                            expected: ketos::function::Arity::Range(1, 2),
                            found: args.len() as u32,
                        }));
                    }
                };
                let feature = <&str as FromValueRef>::from_value_ref(&args[0])?;

                wrapper.borrow_mut_env().add_feature(feature, version);
                Ok(Value::Unit)
            })
        });

        interp.scope().add_value_with_name("expand-path", |name| {
            Value::new_foreign_fn(name, move |_ctx, args| {
                assert_args!(args, 1, name);
                let path = <&str as FromValueRef>::from_value_ref(&args[0])?;
                let expanded = shellexpand::tilde(path);
                let canonicalized = match fs::canonicalize(expanded.to_string()) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(From::from(ketos::io::IoError {
                            err: e,
                            path: PathBuf::from(path),
                            mode: ketos::io::IoMode::Read,
                        }));
                    }
                };
                Ok(<String as Into<Value>>::into(
                    canonicalized.to_string_lossy().to_string(),
                ))
            })
        });

        // TODO: Should an eval error here stop the entire env injection? Right now, it just logs an error to stderr.
        interp.scope().add_value_with_name("env/ejson", |name| {
            Value::new_foreign_fn(name, move |ctx, args| {
                if args.len() < 1 {
                    return Err(From::from(ketos::exec::ExecError::ArityError {
                        name: Some(name),
                        expected: ketos::function::Arity::Min(1),
                        found: 0,
                    }));
                }

                let path = <&str as FromValueRef>::from_value_ref(&args[0])?;
                let expanded = shellexpand::tilde(path);
                let canonicalized = match fs::canonicalize(expanded.to_string()) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(From::from(ketos::io::IoError {
                            err: e,
                            path: PathBuf::from(path),
                            mode: ketos::io::IoMode::Read,
                        }));
                    }
                };

                let subpaths = args.get(1).and_then(|second_arg| match second_arg {
                    Value::List(elements) => {
                        // TODO: Handle invalid inputs.
                        Some(
                            elements
                                .iter()
                                .filter_map(|elem| match elem {
                                    Value::Char(c) => Some(c.to_string()),
                                    Value::String(s) => Some(s.to_string()),
                                    _ => None,
                                })
                                .collect(),
                        )
                    }
                    Value::String(s) => Some(vec![s.to_string()]),
                    Value::Unit => None,
                    _ => None,
                });

                let shadowenv_value = get_value(ctx, shadowenv_name);
                let shadowenv =
                    <&RefCellWrapper<Shadowenv> as FromValueRef>::from_value_ref(&shadowenv_value)?;
                let mut shadowenv_ref = shadowenv.borrow_mut_env();

                let esjon_tracker_value = get_value(ctx, ejson_tracker_name);
                let esjon_tracker =
                    <&RefCellWrapper<HashSet<PathBuf>> as FromValueRef>::from_value_ref(
                        &esjon_tracker_value,
                    )?;
                let mut esjon_tracker_ref = esjon_tracker.borrow_mut_env();
                esjon_tracker_ref.insert(canonicalized.clone());

                // TODO: Technically we shouldn't decode the entire file, only the queried subtree.
                //       This may matter on large secret files where we only pick a small subset.
                // TODO: This code needs some cleanup.
                match ejson::load_ejson_file(&canonicalized) {
                    Ok(ejson) => {
                        if let Some(subpaths) = subpaths {
                            for subpath in subpaths {
                                let _ = identify_ejson_subtree(&subpath, &ejson)
                                    .and_then(|subtree| {
                                        inject_ejson_contents(
                                            subpath.split(".").last().unwrap(),
                                            &subtree,
                                            shadowenv_ref.deref_mut(),
                                        )
                                    })
                                    .inspect_err(|err| eprintln!("{err}"));
                            }
                        } else {
                            // Load entire file.
                            let _ = inject_ejson_contents(
                                "",
                                &serde_json::Value::Object(ejson),
                                shadowenv_ref.deref_mut(),
                            )
                            .inspect_err(|err| eprintln!("{err}"));
                        }
                    }

                    Err(err) => {
                        // TODO: How to error handle correctly here? Should we repurpose `output::format_hook_error`?
                        // Note: Any print to stdout seems to be treated as input to the interpreter, must use stderr.
                        eprintln!("Error evalutating ejson: {err}");
                        return Ok(Value::Unit);
                    }
                };

                Ok(Value::Unit)
            })
        });

        let prelude = r#"
          ;; Better when/if/let macros
          (macro (when pred :rest body) `(if ,pred (do ,@body) ()))
          (macro (when-let assigns :rest body)
            `(let ,assigns (when (not (null ,(first (first assigns)))) ,@body)))
        "#;

        if let Err(err) = interp.run_code(prelude, None) {
            interp.display_error(&err);
            if let Some(trace) = interp.get_traceback() {
                eprintln!();
                interp.display_trace(&trace);
            }
            return Err(err);
        };

        let mut files = source.files.clone();
        files.sort();
        let original_path = env::current_dir();
        let _ = env::set_current_dir(Path::new(&source.dir));

        for source_file in &files {
            let fname = format!("__shadowenv__{}", source_file.name);
            let prog = format!("(define ({} env) (do {}))", fname, source_file.contents);

            // TODO: error type?
            if let Err(err) = interp.run_code(&prog, Some(source_file.name.to_string())) {
                interp.display_error(&err);
                if let Some(trace) = interp.get_traceback() {
                    eprintln!();
                    interp.display_trace(&trace);
                }
                return Err(err);
            };
        }

        for source_file in &files {
            let fname = format!("__shadowenv__{}", source_file.name);
            if let Err(err) = interp.call(&fname, vec![Value::Foreign(rc_wrapper.clone())]) {
                // TODO: error type?
                interp.display_error(&err);
                if let Some(trace) = interp.get_traceback() {
                    eprintln!();
                    interp.display_trace(&trace);
                }
                return Err(err);
            };
        }

        if let Ok(dir) = original_path {
            let _ = env::set_current_dir(dir);
        }

        Ok(())
    }
}

fn identify_ejson_subtree(
    at_path: &str,
    ejson: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value, anyhow::Error> {
    // TODO: It is unclear how the traversal & injection should actually work, eg.:
    // - How to handle arrays? Ignore them, use indexed keys (key_0=elem0, key_1=elem1, ...) or use compound values (key="elem1,elem2,...")
    //      - Compounding has problems with deeper nesting (`a: [{b: "b"}, ...]`). Indexing seems to be more universal.
    // - How to handle nulls?
    // - How should nested object keys compose? `{ a: { b: "c" }}` -> `A_B=c` or `B=c`?
    // - How to compose the name of the env var? `(env/ejson "..." "path.to.obj")` -> `OBJ_KEY1=VAL1`` or `KEY1=VAL1` or ...?
    // TODO: Unfortunately this copies the data, we should write our own simple dotpath traversal.
    match ejson.dot_get::<serde_json::Value>(at_path)? {
        Some(value) => Ok(value),
        None => {
            return Err(anyhow!("Json path {at_path} does not exist or is null."));
        }
    }
}

fn inject_ejson_contents(
    key: &str,
    value: &serde_json::Value,
    shadowenv: &mut Shadowenv,
) -> Result<(), anyhow::Error> {
    let key = key.replace(".", "_").to_ascii_uppercase();
    let prefix = if key.is_empty() {
        "".to_owned()
    } else {
        format!("{key}_")
    };

    match value {
        serde_json::Value::Null => return Ok(()), // TODO: Invalid? Unset value? Ignore? Ignoring for now.
        serde_json::Value::String(s) => shadowenv.set(&key, Some(s)),

        bool @ serde_json::Value::Bool(_) => {
            shadowenv.set(&key, serde_json::to_string(bool).ok().as_deref())
        }

        num @ serde_json::Value::Number(_) => {
            shadowenv.set(&key, serde_json::to_string(num).ok().as_deref())
        }

        serde_json::Value::Array(array) => {
            for (index, elem) in array.iter().enumerate() {
                inject_ejson_contents(&format!("{prefix}{index}"), elem, shadowenv)?;
            }
        }

        serde_json::Value::Object(map) => {
            for (k, v) in map {
                inject_ejson_contents(&format!("{prefix}{k}"), v, shadowenv)?;
            }
        }
    };

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::Feature;
    use crate::hash::SourceFile;
    use crate::undo::Data;
    use std::collections::{HashMap, HashSet};

    fn build_source(content: &str) -> Source {
        Source {
            dir: "dir".to_string(),
            files: vec![SourceFile {
                name: "file.lisp".to_string(),
                contents: content.to_string(),
            }],
            ejson_file_paths: vec![],
            used_ejson_files: HashSet::default(),
        }
    }

    fn build_shadow_env(env_variables: Vec<(&str, &str)>) -> Shadowenv {
        let env = env_variables
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>();
        Shadowenv::new(env, Data::new(), 0)
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
            ShadowLang::run_programs(shadowenv, &mut SourceList::new_with_sources(vec![source]));
        let env = result.unwrap().exports().unwrap();

        assert_eq!(env["VAL_A"].as_ref().unwrap(), "42");
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
            ShadowLang::run_programs(shadowenv, &mut SourceList::new_with_sources(vec![source]));
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
            ShadowLang::run_programs(shadowenv, &mut SourceList::new_with_sources(vec![source]));
        let env = result.unwrap().exports().unwrap();

        assert_eq!(env["PATH"].as_ref().unwrap(), "/something_else");
    }

    #[test]
    fn test_features() {
        let shadowenv = build_shadow_env(vec![]);

        let source = build_source(
            r#"
                (provide "ruby" "3.1.2")
            "#,
        );

        let shadowenv =
            ShadowLang::run_programs(shadowenv, &mut SourceList::new_with_sources(vec![source]))
                .unwrap();
        let expected = HashSet::from([Feature::new("ruby".to_string(), Some("3.1.2".to_string()))]);
        assert_eq!(shadowenv.features(), expected);
    }

    #[test]
    fn test_expand_path() {
        let shadowenv = build_shadow_env(vec![]);

        let source = build_source(
            r#"
                (env/set "EXPANDED" (expand-path "~"))
            "#,
        );
        let home = dirs::home_dir().map(|p| p.into_os_string().into_string().unwrap());
        let shadowenv =
            ShadowLang::run_programs(shadowenv, &mut SourceList::new_with_sources(vec![source]))
                .unwrap();
        assert_eq!(shadowenv.get("EXPANDED"), home);
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
            &mut SourceList::new_with_sources(vec![outer_source, inner_source]),
        )
        .unwrap();
        assert_eq!(shadowenv.get("TEST"), Some("TWO".to_string()));
    }
}

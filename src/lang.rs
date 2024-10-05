use crate::hash::{Source, SourceList};
use crate::shadowenv::Shadowenv;
use ketos::{Context, Error, FromValueRef, Name, Value};
use ketos_derive::{ForeignValue, FromValueRef};
use std::{
    cell::{Ref, RefCell},
    env, fs,
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
struct ShadowenvWrapper(RefCell<Shadowenv>);

impl ShadowenvWrapper {
    fn new(shadowenv: Shadowenv) -> Self {
        Self(RefCell::new(shadowenv))
    }
    fn borrow_mut_env(&self) -> std::cell::RefMut<Shadowenv> {
        self.0.borrow_mut()
    }
    fn borrow_env(&self) -> Ref<'_, Shadowenv> {
        self.0.borrow()
    }

    fn into_inner(self) -> Shadowenv {
        self.0.into_inner()
    }
}

fn get_value(ctx: &Context, shadowenv_name: Name) -> Value {
    ctx.scope()
        .get_constant(shadowenv_name)
        .expect("bug: shadowenv not defined")
}

fn path_concat(vals: &mut [Value]) -> Result<String, Error> {
    let res = vals.iter().fold(
        PathBuf::new(),
        |acc, v| acc.join(<&str as FromValueRef>::from_value_ref(v).unwrap()), // TODO(burke): don't unwrap
    );

    Ok(res.to_string_lossy().to_string())
}

impl ShadowLang {
    pub fn run_programs(shadowenv: Shadowenv, sources: SourceList) -> Result<Shadowenv, Error> {
        let wrapper = Rc::new(ShadowenvWrapper::new(shadowenv));
        let dirs = sources.shortened_dirs();
        for source in sources.consume() {
            Self::run(&wrapper, source)?;
        }
        let mut result = Rc::try_unwrap(wrapper).unwrap().into_inner();
        result.add_dirs(dirs);
        Ok(result)
    }

    fn run(rc_wrapper: &Rc<ShadowenvWrapper>, source: Source) -> Result<(), Error> {
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

        interp
            .scope()
            .add_constant(shadowenv_name, Value::Foreign(rc_wrapper.clone()));

        ketos_fn2! { interp.scope() => "path-concat" =>
        fn path_concat(...) -> String }

        interp.scope().add_value_with_name("env/get", |name| {
            Value::new_foreign_fn(name, move |ctx, args| {
                assert_args!(args, 1, name);

                let value = get_value(ctx, shadowenv_name);
                let wrapper: &ShadowenvWrapper = FromValueRef::from_value_ref(&value)?;
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
                let shadowenv = <&ShadowenvWrapper as FromValueRef>::from_value_ref(&value)?;
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
                    let wrapper: &ShadowenvWrapper = FromValueRef::from_value_ref(&value)?;
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
                    let wrapper: &ShadowenvWrapper = FromValueRef::from_value_ref(&value)?;
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
                    let wrapper: &ShadowenvWrapper = FromValueRef::from_value_ref(&value)?;
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
                    let wrapper: &ShadowenvWrapper = FromValueRef::from_value_ref(&value)?;
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
                let wrapper: &ShadowenvWrapper = FromValueRef::from_value_ref(&value)?;

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
        }
    }

    fn build_shadow_env(env_variables: Vec<(&str, &str)>) -> Shadowenv {
        let env = env_variables
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>();
        Shadowenv::new(env, Data::new(), 0, HashSet::new())
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
        let shadowenv = build_shadow_env(vec![]);

        let source = build_source(
            r#"
                (provide "ruby" "3.1.2")
            "#,
        );

        let shadowenv =
            ShadowLang::run_programs(shadowenv, SourceList::new_with_sources(vec![source]))
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
            ShadowLang::run_programs(shadowenv, SourceList::new_with_sources(vec![source]))
                .unwrap();
        assert_eq!(shadowenv.get("EXPANDED"), home);
    }
}

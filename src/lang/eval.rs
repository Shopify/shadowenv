//! Values, builtin functions, and evaluation.
//!
//! Builtins follow the semantics Ketos gave them, so existing programs behave the same.
//!
//! Programs come from directories the user trusted, but trust covers a directory, not its
//! contents, so evaluation must stay cheap and total for any input. Shadowlisp has no loops or
//! recursion, but sequential `let` bindings can still double a value at each step. Three limits
//! bound every run:
//!
//! - Values a program builds are at most [`MAX_VALUE_SIZE`]: a string's length in bytes, or a
//!   list's total count of elements, nested elements included (shared ones counted each time).
//! - Lists nest at most [`MAX_LIST_DEPTH`] deep, so recursive comparison, formatting, and
//!   dropping can't exhaust the stack.
//! - A run spends at most [`FUEL`] units of work, about one per expression evaluated, byte or
//!   element touched, or byte allocated. That bounds both time and memory deterministically.

use super::{
    compile::{Kind, Node},
    Fault, Span,
};
use crate::shadowenv::{Shadowenv, VariableError};
use path_clean::PathClean;
use std::{
    fmt::{self, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

/// 128 KiB: Linux won't pass a longer environment entry to a new process (`MAX_ARG_STRLEN`), so
/// nothing longer is useful in an environment variable anyway.
pub const MAX_VALUE_SIZE: usize = 128 * 1024;
pub const MAX_LIST_DEPTH: usize = 64;
/// About 32M units: ample for real programs, which spend thousands.
pub const FUEL: u64 = 1 << 25;

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Str(Rc<str>),
    /// Never empty; an empty list is `Unit`.
    List(Rc<List>),
    Builtin(Builtin),
}

#[derive(Debug)]
pub struct List {
    items: Box<[Value]>,
    /// One for the list itself plus the size of each item.
    size: usize,
    depth: usize,
}

impl Value {
    /// Builds a list, or `Unit` if `items` is empty. Callers check [`MAX_VALUE_SIZE`] with
    /// [`Value::size`] before collecting `items`, so an oversized list is never allocated.
    fn list(items: Vec<Value>, span: Span) -> Result<Self, Fault> {
        if items.is_empty() {
            return Ok(Value::Unit);
        }
        let size = items
            .iter()
            .fold(1usize, |size, item| size.saturating_add(item.size()));
        let depth = 1 + items.iter().map(Value::depth).max().unwrap_or(0);
        if size > MAX_VALUE_SIZE {
            return Err(too_large(span));
        }
        if depth > MAX_LIST_DEPTH {
            return Err(Fault::new(
                span,
                format!("lists may be nested at most {MAX_LIST_DEPTH} deep"),
            ));
        }
        Ok(Value::List(Rc::new(List {
            items: items.into(),
            size,
            depth,
        })))
    }

    /// The size [`MAX_VALUE_SIZE`] limits, and the cost of touching every part of the value.
    fn size(&self) -> usize {
        match self {
            Value::Str(s) => s.len().max(1),
            Value::List(list) => list.size,
            Value::Unit | Value::Bool(_) | Value::Int(_) | Value::Builtin(_) => 1,
        }
    }

    fn depth(&self) -> usize {
        match self {
            Value::List(list) => list.depth,
            _ => 0,
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Value::Unit => "()",
            Value::Bool(_) => "bool",
            Value::Int(_) => "integer",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Builtin(_) => "function",
        }
    }

    /// Strict equality, as `=` uses: comparing values of different types is an error. `eq` and
    /// `ne` turn that error into an answer. Takes time proportional to the smaller operand's size.
    fn strict_eq(&self, other: &Value) -> Result<bool, ()> {
        Ok(match (self, other) {
            (Value::Unit, Value::Unit) => true,
            (Value::Unit, Value::List(_)) | (Value::List(_), Value::Unit) => false,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            // Ketos compared its own functions by identity, but shadowenv's were foreign values
            // that compare with nothing, themselves included.
            (Value::Builtin(a), Value::Builtin(b))
                if !a.is_shadowenv_fn() && !b.is_shadowenv_fn() =>
            {
                a == b
            }
            (Value::List(a), Value::List(b)) => {
                if a.items.len() != b.items.len() {
                    return Ok(false);
                }
                for (a, b) in a.items.iter().zip(b.items.iter()) {
                    if !a.strict_eq(b)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => return Err(()),
        })
    }

    /// The value as it would be written, cut short for error messages.
    fn preview(&self) -> String {
        const LIMIT: usize = 120;
        let mut out = Bounded {
            out: String::new(),
            remaining: LIMIT,
        };
        if write!(out, "{self}").is_err() {
            out.out.push('…');
        }
        out.out
    }
}

/// A writer that fails once it has taken `remaining` bytes, so formatting a huge value stops
/// early instead of visiting all of it.
struct Bounded {
    out: String,
    remaining: usize,
}

impl Write for Bounded {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() <= self.remaining {
            self.out.push_str(s);
            self.remaining -= s.len();
            return Ok(());
        }
        let mut end = self.remaining;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.out.push_str(&s[..end]);
        self.remaining = 0;
        Err(fmt::Error)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unit => f.write_str("()"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::List(list) => {
                f.write_str("(")?;
                for (i, item) in list.items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str(")")
            }
            Value::Builtin(b) => f.write_str(b.name()),
        }
    }
}

enum Arity {
    Exact(usize),
    AtLeast(usize),
    Range(usize, usize),
}

macro_rules! builtins {
    ( $( $variant:ident => $name:literal, $arity:expr; )+ ) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Builtin { $( $variant, )+ }

        impl Builtin {
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $( $name => Some(Self::$variant), )+
                    _ => None,
                }
            }

            pub fn name(self) -> &'static str {
                match self { $( Self::$variant => $name, )+ }
            }

            fn arity(self) -> Arity {
                use Arity::*;
                match self { $( Self::$variant => $arity, )+ }
            }
        }
    };
}

builtins! {
    EnvGet => "env/get", Exact(1);
    EnvSet => "env/set", Exact(2);
    EnvPrepend => "env/prepend-to-pathlist", Exact(2);
    EnvAppend => "env/append-to-pathlist", Exact(2);
    EnvRemove => "env/remove-from-pathlist", Exact(2);
    EnvRemoveContaining => "env/remove-from-pathlist-containing", Exact(2);
    PathConcat => "path-concat", AtLeast(0);
    ExpandPath => "expand-path", Exact(1);
    Provide => "provide", Range(1, 2);
    Eq => "eq", AtLeast(2);
    Ne => "ne", AtLeast(2);
    StrictEq => "=", AtLeast(2);
    Not => "not", Exact(1);
    Null => "null", Exact(1);
    Append => "append", AtLeast(1);
    Elt => "elt", Exact(2);
    Concat => "concat", AtLeast(1);
    Join => "join", AtLeast(1);
    First => "first", Exact(1);
    Last => "last", Exact(1);
    Tail => "tail", Exact(1);
    List => "list", AtLeast(0);
}

impl Builtin {
    fn check_arity(self, found: usize, span: Span) -> Result<(), Fault> {
        let (ok, expected) = match self.arity() {
            Arity::Exact(n) => (found == n, plural(n)),
            Arity::AtLeast(n) => (found >= n, format!("at least {}", plural(n))),
            Arity::Range(lo, hi) => (
                (lo..=hi).contains(&found),
                format!("{lo} to {hi} arguments"),
            ),
        };
        if ok {
            Ok(())
        } else {
            Err(Fault::new(
                span,
                format!("`{}` expects {expected}; found {found}", self.name()),
            ))
        }
    }

    /// Whether the builtin rewrites the variable named by its first argument, scanning the
    /// variable's current value.
    fn edits_pathlist(self) -> bool {
        matches!(
            self,
            Builtin::EnvPrepend
                | Builtin::EnvAppend
                | Builtin::EnvRemove
                | Builtin::EnvRemoveContaining
        )
    }

    /// Whether Ketos provided the builtin itself or shadowenv added it, which affects equality.
    fn is_shadowenv_fn(self) -> bool {
        matches!(
            self,
            Builtin::EnvGet
                | Builtin::EnvSet
                | Builtin::EnvPrepend
                | Builtin::EnvAppend
                | Builtin::EnvRemove
                | Builtin::EnvRemoveContaining
                | Builtin::PathConcat
                | Builtin::ExpandPath
                | Builtin::Provide
        )
    }
}

fn plural(n: usize) -> String {
    if n == 1 {
        "1 argument".to_string()
    } else {
        format!("{n} arguments")
    }
}

/// Evaluates programs against a Shadowenv, sharing one [`FUEL`] budget across all of them.
pub struct Machine<'a> {
    env: &'a mut Shadowenv,
    /// Relative paths given to `expand-path` resolve against this directory.
    dir: &'a Path,
    locals: Vec<Value>,
    fuel: u64,
}

impl<'a> Machine<'a> {
    pub fn new(env: &'a mut Shadowenv) -> Self {
        Self {
            env,
            dir: Path::new(""),
            locals: Vec::new(),
            fuel: FUEL,
        }
    }

    pub fn run(&mut self, program: &[Node], dir: &'a Path) -> Result<(), Fault> {
        self.dir = dir;
        self.locals.clear();
        for node in program {
            self.eval(node)?;
        }
        Ok(())
    }

    fn charge(&mut self, cost: usize, span: Span) -> Result<(), Fault> {
        let cost = u64::try_from(cost).unwrap_or(u64::MAX);
        if cost > self.fuel {
            self.fuel = 0;
            return Err(Fault::new(
                span,
                "program exceeded Shadowlisp's evaluation limit",
            ));
        }
        self.fuel -= cost;
        Ok(())
    }

    fn eval(&mut self, node: &Node) -> Result<Value, Fault> {
        self.charge(1, node.span)?;
        match &node.kind {
            Kind::Literal(value) => Ok(value.clone()),
            Kind::Local(index) => Ok(self.locals[*index].clone()),
            Kind::Builtin(builtin, args) => {
                let args = self.eval_all(args)?;
                self.call(*builtin, args, node.span)
            }
            Kind::Call(callee, args) => {
                let callee = self.eval(callee)?;
                let args = self.eval_all(args)?;
                let Value::Builtin(builtin) = callee else {
                    return Err(type_error("function", &callee, node.span));
                };
                self.call(builtin, args, node.span)
            }
            Kind::If(test, then, otherwise) => {
                if self.test(test)? {
                    self.eval(then)
                } else {
                    otherwise
                        .as_ref()
                        .map_or(Ok(Value::Unit), |node| self.eval(node))
                }
            }
            Kind::Cond(clauses) => {
                for clause in clauses {
                    let matched = match &clause.test {
                        None => true,
                        Some(test) => self.test(test)?,
                    };
                    if matched {
                        return self.eval(&clause.body);
                    }
                }
                Ok(Value::Unit)
            }
            Kind::Do(nodes) => {
                let mut last = Value::Unit;
                for node in nodes {
                    last = self.eval(node)?;
                }
                Ok(last)
            }
            // As in Ketos, every operand but the last must be a bool; the last is returned as is.
            Kind::And(nodes) => {
                let Some((last, init)) = nodes.split_last() else {
                    return Ok(Value::Bool(true));
                };
                for node in init {
                    if !self.test(node)? {
                        return Ok(Value::Bool(false));
                    }
                }
                self.eval(last)
            }
            Kind::Or(nodes) => {
                let Some((last, init)) = nodes.split_last() else {
                    return Ok(Value::Bool(false));
                };
                for node in init {
                    if self.test(node)? {
                        return Ok(Value::Bool(true));
                    }
                }
                self.eval(last)
            }
            Kind::Let(values, body) => self.scoped(values, |m, _| m.eval(body)),
            // Every binding is evaluated; the body runs only if the deciding one is non-`()`.
            Kind::WhenLet(values, test, body) => {
                self.scoped(values, |m, depth| match m.locals[depth + test] {
                    Value::Unit => Ok(Value::Unit),
                    _ => m.eval(body),
                })
            }
        }
    }

    /// Binds `values` in order, each visible to the ones after it, then runs `body`, giving it
    /// the position in `locals` of the first binding.
    fn scoped(
        &mut self,
        values: &[Node],
        body: impl FnOnce(&mut Self, usize) -> Result<Value, Fault>,
    ) -> Result<Value, Fault> {
        let depth = self.locals.len();
        for value in values {
            let value = self.eval(value)?;
            self.locals.push(value);
        }
        let result = body(self, depth);
        self.locals.truncate(depth);
        result
    }

    fn eval_all(&mut self, nodes: &[Node]) -> Result<Vec<Value>, Fault> {
        nodes.iter().map(|node| self.eval(node)).collect()
    }

    /// Conditions must be booleans, as in Ketos.
    fn test(&mut self, node: &Node) -> Result<bool, Fault> {
        match self.eval(node)? {
            Value::Bool(b) => Ok(b),
            value => Err(type_error("bool", &value, node.span)),
        }
    }

    /// Arity is checked here, when the call runs, as Ketos did for shadowenv's own functions: a
    /// mistaken call in a branch that never runs doesn't stop the program.
    ///
    /// Every call is charged for its arguments' sizes, which bounds the work of reading them, and
    /// for its result's size, which bounds allocation. Results never exceed [`MAX_VALUE_SIZE`], so
    /// a single call costs at most a few multiples of it.
    fn call(&mut self, builtin: Builtin, args: Vec<Value>, span: Span) -> Result<Value, Fault> {
        builtin.check_arity(args.len(), span)?;
        let input = args
            .iter()
            .fold(0usize, |cost, arg| cost.saturating_add(arg.size()));
        self.charge(input, span)?;
        if builtin.edits_pathlist() {
            if let Value::Str(name) = &args[0] {
                let scanned = self.env.value_len(name);
                self.charge(scanned, span)?;
            }
        }
        let result = self.apply(builtin, args, span)?;
        self.charge(result.size(), span)?;
        Ok(result)
    }

    fn apply(&mut self, builtin: Builtin, args: Vec<Value>, span: Span) -> Result<Value, Fault> {
        let str_arg = |i: usize| match &args[i] {
            Value::Str(s) => Ok(&**s),
            value => Err(type_error("string", value, span)),
        };
        let unit = |result: Result<(), VariableError>| {
            result
                .map(|()| Value::Unit)
                .map_err(|e| Fault::new(span, e.to_string()))
        };

        match builtin {
            // Not limited by MAX_VALUE_SIZE: the value comes from the user's environment, and the
            // call has already been charged for it.
            Builtin::EnvGet => Ok(self
                .env
                .get(str_arg(0)?)
                .map_or(Value::Unit, |v| Value::Str(v.into()))),
            Builtin::EnvSet => {
                let value = match &args[1] {
                    Value::Unit => None,
                    Value::Str(s) => Some(&**s),
                    value => return Err(type_error("string or ()", value, span)),
                };
                unit(self.env.set(str_arg(0)?, value))
            }
            Builtin::EnvPrepend => unit(self.env.prepend_to_pathlist(str_arg(0)?, str_arg(1)?)),
            Builtin::EnvAppend => unit(self.env.append_to_pathlist(str_arg(0)?, str_arg(1)?)),
            Builtin::EnvRemove => unit(self.env.remove_from_pathlist(str_arg(0)?, str_arg(1)?)),
            Builtin::EnvRemoveContaining => unit(
                self.env
                    .remove_from_pathlist_containing(str_arg(0)?, str_arg(1)?),
            ),
            Builtin::PathConcat => {
                let mut len = 0usize;
                for i in 0..args.len() {
                    len = len.saturating_add(str_arg(i)?.len() + 1);
                }
                check_size(len, span)?;
                let mut path = PathBuf::new();
                for i in 0..args.len() {
                    path.push(str_arg(i)?);
                }
                string(path.clean().to_string_lossy().into_owned(), span)
            }
            Builtin::ExpandPath => {
                let path = str_arg(0)?;
                let expanded = shellexpand::tilde(path);
                let cannot = |e: &dyn std::fmt::Display| {
                    Fault::new(
                        span,
                        format!("cannot expand {}: {e}", Value::Str(path.into()).preview()),
                    )
                };
                // `absolute` rejects an empty path, as it did when Ketos called it directly.
                if expanded.is_empty() {
                    return Err(cannot(&"cannot make an empty path absolute"));
                }
                let absolute = std::path::absolute(self.dir.join(expanded.as_ref()))
                    .map_err(|e| cannot(&e))?;
                string(absolute.clean().to_string_lossy().into_owned(), span)
            }
            Builtin::Provide => {
                let version = if args.len() == 2 {
                    Some(str_arg(1)?)
                } else {
                    None
                };
                self.env.add_feature(str_arg(0)?, version);
                Ok(Value::Unit)
            }
            Builtin::Eq => {
                let (first, rest) = args.split_first().expect("arity checked");
                let equal = rest.iter().try_fold(true, |equal, other| {
                    Ok::<_, ()>(equal && first.strict_eq(other)?)
                });
                Ok(Value::Bool(equal.unwrap_or(false)))
            }
            Builtin::Ne => {
                let distinct = (|| {
                    for (i, a) in args.iter().enumerate() {
                        for b in &args[i + 1..] {
                            if a.strict_eq(b)? {
                                return Ok(false);
                            }
                        }
                    }
                    Ok::<_, ()>(true)
                })();
                Ok(Value::Bool(distinct.unwrap_or(true)))
            }
            Builtin::StrictEq => {
                let (first, rest) = args.split_first().expect("arity checked");
                for other in rest {
                    match first.strict_eq(other) {
                        Ok(true) => {}
                        Ok(false) => return Ok(Value::Bool(false)),
                        Err(()) => {
                            return Err(Fault::new(
                                span,
                                format!(
                                    "type error: cannot compare {}: {} with {}: {}",
                                    first.type_name(),
                                    first.preview(),
                                    other.type_name(),
                                    other.preview(),
                                ),
                            ))
                        }
                    }
                }
                Ok(Value::Bool(true))
            }
            Builtin::Not => match &args[0] {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                value => Err(type_error("bool", value, span)),
            },
            Builtin::Null => Ok(Value::Bool(matches!(args[0], Value::Unit))),
            Builtin::Append => {
                let (list, values) = args.split_first().expect("arity checked");
                let items = list_items(list, span)?;
                let size = values
                    .iter()
                    .fold(list.size(), |size, v| size.saturating_add(v.size()));
                check_size(size, span)?;
                let mut out = Vec::with_capacity(items.len() + values.len());
                out.extend_from_slice(items);
                out.extend_from_slice(values);
                Value::list(out, span)
            }
            Builtin::Elt => {
                let Value::List(list) = &args[0] else {
                    return Err(type_error("list", &args[0], span));
                };
                let index = match &args[1] {
                    Value::Int(i) if *i >= 0 => *i as usize,
                    value => return Err(type_error("non-negative integer", value, span)),
                };
                list.items.get(index).cloned().ok_or_else(|| {
                    Fault::new(
                        span,
                        format!(
                            "index {index} is out of bounds for a list of {}",
                            list.items.len()
                        ),
                    )
                })
            }
            Builtin::Concat => match &args[0] {
                Value::Unit | Value::List(_) => concat_lists(args, span),
                Value::Str(_) => concat_strings(args, span),
                value => Err(type_error("list or string", value, span)),
            },
            Builtin::Join => {
                let (separator, rest) = args.split_first().expect("arity checked");
                match separator {
                    Value::Unit => concat_lists(rest.to_vec(), span),
                    Value::List(separator) => join_lists(&separator.items, rest, span),
                    Value::Str(s) if s.is_empty() => concat_strings(rest.to_vec(), span),
                    Value::Str(separator) => join_strings(separator, rest, span),
                    value => Err(type_error("list or string", value, span)),
                }
            }
            Builtin::First => match &args[0] {
                Value::List(list) => Ok(list.items[0].clone()),
                value => Err(type_error("list", value, span)),
            },
            Builtin::Last => match &args[0] {
                Value::List(list) => Ok(list.items[list.items.len() - 1].clone()),
                value => Err(type_error("list", value, span)),
            },
            Builtin::Tail => match &args[0] {
                Value::List(list) => Value::list(list.items[1..].to_vec(), span),
                value => Err(type_error("list", value, span)),
            },
            Builtin::List => {
                let size = args
                    .iter()
                    .fold(1usize, |size, v| size.saturating_add(v.size()));
                check_size(size, span)?;
                Value::list(args, span)
            }
        }
    }
}

fn too_large(span: Span) -> Fault {
    Fault::new(
        span,
        format!("value is larger than Shadowlisp's limit of {MAX_VALUE_SIZE} bytes or elements"),
    )
}

fn check_size(size: usize, span: Span) -> Result<(), Fault> {
    if size > MAX_VALUE_SIZE {
        Err(too_large(span))
    } else {
        Ok(())
    }
}

fn string(s: String, span: Span) -> Result<Value, Fault> {
    check_size(s.len(), span)?;
    Ok(Value::Str(s.into()))
}

fn type_error(expected: &str, found: &Value, span: Span) -> Fault {
    Fault::new(
        span,
        format!(
            "type error: expected {expected}; found {}: {}",
            found.type_name(),
            found.preview()
        ),
    )
}

fn list_items(value: &Value, span: Span) -> Result<&[Value], Fault> {
    match value {
        Value::Unit => Ok(&[]),
        Value::List(list) => Ok(&list.items),
        value => Err(type_error("list", value, span)),
    }
}

// As in Ketos, concatenating a single value returns it unchanged.

fn concat_lists(args: Vec<Value>, span: Span) -> Result<Value, Fault> {
    join_lists(&[], &args, span)
}

fn concat_strings(args: Vec<Value>, span: Span) -> Result<Value, Fault> {
    join_strings("", &args, span)
}

fn join_strings(separator: &str, args: &[Value], span: Span) -> Result<Value, Fault> {
    if let [only] = args {
        if separator.is_empty() {
            return Ok(only.clone());
        }
    }
    let mut len = 0usize;
    for (i, arg) in args.iter().enumerate() {
        let Value::Str(s) = arg else {
            return Err(type_error("string", arg, span));
        };
        if i > 0 {
            len = len.saturating_add(separator.len());
        }
        len = len.saturating_add(s.len());
    }
    check_size(len, span)?;
    let mut out = String::with_capacity(len);
    for (i, arg) in args.iter().enumerate() {
        if let Value::Str(s) = arg {
            if i > 0 {
                out.push_str(separator);
            }
            out.push_str(s);
        }
    }
    Ok(Value::Str(out.into()))
}

fn join_lists(separator: &[Value], args: &[Value], span: Span) -> Result<Value, Fault> {
    if let [only] = args {
        if separator.is_empty() {
            return Ok(only.clone());
        }
    }
    let separator_size = separator
        .iter()
        .fold(0usize, |size, v| size.saturating_add(v.size()));
    let mut size = 1usize;
    let mut count = 0usize;
    for (i, arg) in args.iter().enumerate() {
        let items = list_items(arg, span)?;
        if i > 0 {
            size = size.saturating_add(separator_size);
            count += separator.len();
        }
        size = size.saturating_add(arg.size().saturating_sub(1));
        count += items.len();
    }
    check_size(size, span)?;
    let mut out = Vec::with_capacity(count);
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(separator);
        }
        out.extend_from_slice(list_items(arg, span)?);
    }
    Value::list(out, span)
}

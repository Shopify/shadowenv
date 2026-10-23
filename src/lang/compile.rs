//! Resolves expressions into an evaluable tree.
//!
//! Every name is resolved here: to a local binding, a builtin function, or a special form.
//! Nothing else exists, so a program that mentions any other name is rejected before it runs.

use super::{
    eval::{Builtin, Value},
    reader::{Expr, ExprKind},
    Fault, Span,
};
use std::collections::HashMap;

pub struct Node {
    pub kind: Kind,
    pub span: Span,
}

pub enum Kind {
    Literal(Value),
    /// Index into the evaluator's stack of local bindings.
    Local(usize),
    /// A direct call to a builtin.
    Builtin(Builtin, Vec<Node>),
    /// A call to a computed value, e.g. a builtin bound to a local name.
    Call(Box<Node>, Vec<Node>),
    If(Box<Node>, Box<Node>, Option<Box<Node>>),
    Cond(Vec<Clause>),
    Do(Vec<Node>),
    And(Vec<Node>),
    Or(Vec<Node>),
    Let(Vec<Node>, Box<Node>),
    /// Bindings, the position among them of the binding whose value decides whether the body
    /// runs, and the body.
    WhenLet(Vec<Node>, usize, Box<Node>),
}

pub struct Clause {
    /// `None` for the `else` clause.
    pub test: Option<Node>,
    pub body: Node,
}

#[derive(Clone, Copy)]
enum Form {
    Let,
    WhenLet,
    When,
    If,
    Cond,
    Do,
    And,
    Or,
}

impl Form {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "let" => Self::Let,
            "when-let" => Self::WhenLet,
            "when" => Self::When,
            "if" => Self::If,
            "cond" => Self::Cond,
            "do" => Self::Do,
            "and" => Self::And,
            "or" => Self::Or,
            _ => return None,
        })
    }
}

pub fn compile(program: &[Expr]) -> Result<Vec<Node>, Fault> {
    let mut compiler = Compiler {
        locals: Scope::default(),
    };
    compiler.exprs(program)
}

/// Local bindings in scope, innermost last. Lookup is constant-time, so programs with many
/// bindings compile in linear time.
#[derive(Default)]
struct Scope<'a> {
    stack: Vec<&'a str>,
    /// Each bound name's positions in `stack`, innermost last.
    slots: HashMap<&'a str, Vec<usize>>,
}

impl<'a> Scope<'a> {
    fn lookup(&self, name: &str) -> Option<usize> {
        self.slots.get(name)?.last().copied()
    }

    fn len(&self) -> usize {
        self.stack.len()
    }

    fn push(&mut self, name: &'a str) {
        self.slots.entry(name).or_default().push(self.stack.len());
        self.stack.push(name);
    }

    fn truncate(&mut self, len: usize) {
        while self.stack.len() > len {
            let name = self.stack.pop().expect("length checked");
            let slots = self
                .slots
                .get_mut(name)
                .expect("every pushed name has slots");
            slots.pop();
            if slots.is_empty() {
                self.slots.remove(name);
            }
        }
    }
}

struct Compiler<'a> {
    locals: Scope<'a>,
}

impl<'a> Compiler<'a> {
    fn local(&self, name: &str) -> Option<usize> {
        self.locals.lookup(name)
    }

    fn exprs(&mut self, exprs: &'a [Expr]) -> Result<Vec<Node>, Fault> {
        exprs.iter().map(|expr| self.expr(expr)).collect()
    }

    fn expr(&mut self, expr: &'a Expr) -> Result<Node, Fault> {
        let kind = match &expr.kind {
            ExprKind::Unit => Kind::Literal(Value::Unit),
            ExprKind::Bool(b) => Kind::Literal(Value::Bool(*b)),
            ExprKind::Int(i) => Kind::Literal(Value::Int(*i)),
            ExprKind::Str(s) => Kind::Literal(Value::Str(s.as_str().into())),
            ExprKind::Name(name) => self.reference(name, expr.span)?,
            ExprKind::List(items) => self.list(items, expr.span)?,
        };
        Ok(Node {
            kind,
            span: expr.span,
        })
    }

    fn reference(&self, name: &str, span: Span) -> Result<Kind, Fault> {
        if let Some(index) = self.local(name) {
            return Ok(Kind::Local(index));
        }
        if let Some(builtin) = Builtin::from_name(name) {
            return Ok(Kind::Literal(Value::Builtin(builtin)));
        }
        if Form::from_name(name).is_some() {
            return Err(Fault::new(
                span,
                format!("`{name}` is a special form and cannot be used as a value"),
            ));
        }
        Err(undefined(name, span))
    }

    fn list(&mut self, items: &'a [Expr], span: Span) -> Result<Kind, Fault> {
        let (head, args) = items.split_first().expect("lists are never empty");
        // Local bindings shadow builtins and special forms alike.
        let name = match &head.kind {
            ExprKind::Name(name) if self.local(name).is_none() => name,
            ExprKind::Name(_) | ExprKind::List(_) => {
                let callee = self.expr(head)?;
                return Ok(Kind::Call(Box::new(callee), self.exprs(args)?));
            }
            // As in Ketos, only a name or an expression can be called.
            ExprKind::Unit | ExprKind::Bool(_) | ExprKind::Int(_) | ExprKind::Str(_) => {
                return Err(Fault::new(
                    head.span,
                    "invalid call: expected a function name or expression",
                ))
            }
        };
        if let Some(form) = Form::from_name(name) {
            return self.form(form, name, args, span);
        }
        let Some(builtin) = Builtin::from_name(name) else {
            return Err(undefined(name, head.span));
        };
        Ok(Kind::Builtin(builtin, self.exprs(args)?))
    }

    fn form(
        &mut self,
        form: Form,
        name: &str,
        args: &'a [Expr],
        span: Span,
    ) -> Result<Kind, Fault> {
        let syntax =
            |usage: &str| Fault::new(span, format!("malformed `{name}`: expected {usage}"));
        Ok(match form {
            Form::Do => Kind::Do(self.exprs(args)?),
            Form::And => Kind::And(self.exprs(args)?),
            Form::Or => Kind::Or(self.exprs(args)?),
            Form::If => match args {
                [test, then] => Kind::If(self.boxed(test)?, self.boxed(then)?, None),
                [test, then, otherwise] => Kind::If(
                    self.boxed(test)?,
                    self.boxed(then)?,
                    Some(self.boxed(otherwise)?),
                ),
                _ => return Err(syntax("`(if condition then-case [ else-case ])`")),
            },
            Form::When => {
                let [test, body @ ..] = args else {
                    return Err(syntax("`(when condition [ then ... ])`"));
                };
                let test = self.boxed(test)?;
                let body = Node {
                    kind: Kind::Do(self.exprs(body)?),
                    span,
                };
                Kind::If(test, Box::new(body), None)
            }
            Form::Cond => Kind::Cond(self.cond(args, span)?),
            Form::Let => {
                let [bindings, body] = args else {
                    return Err(syntax(
                        "`(let ((name expression) ...) body)`; wrap multiple body forms in `do`",
                    ));
                };
                let depth = self.locals.len();
                let values = self.bindings(bindings, name)?;
                let body = self.boxed(body)?;
                self.locals.truncate(depth);
                Kind::Let(values, body)
            }
            Form::WhenLet => {
                let usage = "`(when-let ((name expression) ...) [ body ... ])`";
                let [bindings, body @ ..] = args else {
                    return Err(syntax(usage));
                };
                if bindings.kind == ExprKind::Unit {
                    return Err(syntax(usage));
                }
                let depth = self.locals.len();
                let values = self.bindings(bindings, name)?;
                // As in Ketos, the body runs if the first binding's *name* is non-`()` once all
                // bindings are made. If a later binding reuses that name, its value decides.
                let first = self.locals.stack[depth];
                let test = self.locals.lookup(first).expect("just bound") - depth;
                let body = Node {
                    kind: Kind::Do(self.exprs(body)?),
                    span,
                };
                self.locals.truncate(depth);
                Kind::WhenLet(values, test, Box::new(body))
            }
        })
    }

    /// Compiles `((name expression) ...)`, binding each name for the expressions after it.
    /// The caller pops the bindings once their scope ends.
    fn bindings(&mut self, bindings: &'a Expr, form: &str) -> Result<Vec<Node>, Fault> {
        let malformed = |span| {
            Fault::new(
                span,
                format!("malformed `{form}` binding: expected `(name expression)`"),
            )
        };
        let items: &[Expr] = match &bindings.kind {
            ExprKind::Unit => &[],
            ExprKind::List(items) => items,
            _ => return Err(malformed(bindings.span)),
        };
        let mut values = Vec::with_capacity(items.len());
        for binding in items {
            let ExprKind::List(pair) = &binding.kind else {
                return Err(malformed(binding.span));
            };
            let [Expr {
                kind: ExprKind::Name(name),
                ..
            }, value] = &pair[..]
            else {
                return Err(malformed(binding.span));
            };
            values.push(self.expr(value)?);
            self.locals.push(name);
        }
        Ok(values)
    }

    fn cond(&mut self, clauses: &'a [Expr], span: Span) -> Result<Vec<Clause>, Fault> {
        let mut compiled = Vec::with_capacity(clauses.len());
        for (i, clause) in clauses.iter().enumerate() {
            let ExprKind::List(parts) = &clause.kind else {
                return Err(malformed_clause(clause.span));
            };
            let [test, body] = &parts[..] else {
                return Err(malformed_clause(clause.span));
            };
            // As in Ketos, a leading `else` is the catch-all even if `else` is bound locally.
            let test = match &test.kind {
                ExprKind::Name(name) if name == "else" => {
                    if i + 1 != clauses.len() {
                        return Err(Fault::new(
                            clause.span,
                            "`else` must be the last `cond` clause",
                        ));
                    }
                    None
                }
                _ => Some(self.expr(test)?),
            };
            compiled.push(Clause {
                test,
                body: self.expr(body)?,
            });
        }
        if compiled.is_empty() {
            return Err(Fault::new(
                span,
                "malformed `cond`: expected at least one clause",
            ));
        }
        Ok(compiled)
    }

    fn boxed(&mut self, expr: &'a Expr) -> Result<Box<Node>, Fault> {
        self.expr(expr).map(Box::new)
    }
}

fn malformed_clause(span: Span) -> Fault {
    Fault::new(
        span,
        "malformed `cond` clause: expected `(predicate branch)`",
    )
}

fn undefined(name: &str, span: Span) -> Fault {
    Fault::new(
        span,
        format!("`{name}` is not defined; Shadowlisp only provides the functions documented in shadowlisp(5)"),
    )
}

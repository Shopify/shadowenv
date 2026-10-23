//! Reads Shadowlisp source text into expressions.

use super::{Fault, Span};

/// Deeper nesting is rejected rather than risking stack exhaustion.
const MAX_NESTING: usize = 64;

#[derive(Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, PartialEq)]
pub enum ExprKind {
    Unit,
    Bool(bool),
    Int(i64),
    Str(String),
    Name(String),
    /// Never empty; `()` reads as `Unit`.
    List(Vec<Expr>),
}

pub fn read(src: &str) -> Result<Vec<Expr>, Fault> {
    let mut reader = Reader { src, pos: 0 };
    let mut exprs = Vec::new();
    while reader.skip_trivia()? {
        exprs.push(reader.expr(0)?);
    }
    Ok(exprs)
}

fn is_name_char(c: char) -> bool {
    matches!(
        c,
        '!' | '$'
            | '%'
            | '&'
            | '*'
            | '+'
            | '-'
            | '.'
            | '/'
            | '<'
            | '='
            | '>'
            | '?'
            | '^'
            | '_'
            | '|'
    ) || c.is_alphanumeric()
}

fn is_hex(digits: &str) -> bool {
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit())
}

struct Reader<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn char_span(&self, pos: usize) -> Span {
        let len = self.src[pos..].chars().next().map_or(0, char::len_utf8);
        Span::new(pos, pos + len)
    }

    /// Skips whitespace and comments, returning whether any input remains.
    fn skip_trivia(&mut self) -> Result<bool, Fault> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some(';') => self.skip_line(),
                Some('#') if self.rest().starts_with("#|") => self.skip_block_comment()?,
                Some(_) => return Ok(true),
                None => return Ok(false),
            }
        }
    }

    fn skip_line(&mut self) {
        self.pos = match self.rest().find('\n') {
            Some(i) => self.pos + i + 1,
            None => self.src.len(),
        };
    }

    /// `#| ... |#`, nestable. As in Ketos, a `;` inside one comments out the rest of its line.
    fn skip_block_comment(&mut self) -> Result<(), Fault> {
        let start = self.pos;
        self.pos += 2;
        let mut depth = 1;
        while depth > 0 {
            if self.rest().starts_with("|#") {
                depth -= 1;
                self.pos += 2;
            } else if self.rest().starts_with("#|") {
                depth += 1;
                self.pos += 2;
            } else if self.rest().starts_with(';') {
                self.skip_line();
            } else if self.bump().is_none() {
                return Err(Fault::new(
                    Span::new(start, start + 2),
                    "unterminated block comment",
                ));
            }
        }
        Ok(())
    }

    /// Reads one expression; the caller guarantees non-trivia input remains.
    fn expr(&mut self, depth: usize) -> Result<Expr, Fault> {
        let start = self.pos;
        let rest = self.rest();
        let kind = match self.peek().expect("caller checked for remaining input") {
            '(' => return self.list(depth),
            ')' => return Err(Fault::new(self.char_span(start), "unmatched `)`")),
            '"' => ExprKind::Str(self.string()?),
            c if c.is_ascii_digit()
                || (c == '-' && rest[1..].starts_with(|c: char| c.is_ascii_digit())) =>
            {
                ExprKind::Int(self.integer()?)
            }
            c if is_name_char(c) => match self.name() {
                "true" => ExprKind::Bool(true),
                "false" => ExprKind::Bool(false),
                name => ExprKind::Name(name.to_string()),
            },
            c => {
                return Err(Fault::new(
                    self.char_span(start),
                    format!("unexpected character {c:?}"),
                ))
            }
        };
        Ok(Expr {
            kind,
            span: Span::new(start, self.pos),
        })
    }

    fn list(&mut self, depth: usize) -> Result<Expr, Fault> {
        let start = self.pos;
        if depth == MAX_NESTING {
            return Err(Fault::new(
                self.char_span(start),
                "expressions are nested too deeply",
            ));
        }
        self.bump();
        let mut items = Vec::new();
        loop {
            if !self.skip_trivia()? {
                return Err(Fault::new(self.char_span(start), "missing close paren"));
            }
            if self.peek() == Some(')') {
                self.bump();
                break;
            }
            items.push(self.expr(depth + 1)?);
        }
        let kind = if items.is_empty() {
            ExprKind::Unit
        } else {
            ExprKind::List(items)
        };
        Ok(Expr {
            kind,
            span: Span::new(start, self.pos),
        })
    }

    fn name(&mut self) -> &'a str {
        let rest = self.rest();
        let len = rest.find(|c| !is_name_char(c)).unwrap_or(rest.len());
        self.pos += len;
        &rest[..len]
    }

    fn integer(&mut self) -> Result<i64, Fault> {
        let start = self.pos;
        let literal = self.name();
        let span = Span::new(start, self.pos);
        let digits = literal.strip_prefix('-').unwrap_or(literal);
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Fault::new(
                span,
                format!("invalid integer literal `{literal}`"),
            ));
        }
        literal
            .parse()
            .map_err(|_| Fault::new(span, "integer literal is out of range"))
    }

    fn string(&mut self) -> Result<String, Fault> {
        let start = self.pos;
        self.bump();
        let mut out = String::new();
        loop {
            let Some(c) = self.bump() else {
                return Err(Fault::new(self.char_span(start), "unterminated string"));
            };
            match c {
                '"' => return Ok(out),
                '\\' => {
                    if let Some(c) = self.escape()? {
                        out.push(c);
                    }
                }
                // As in Ketos, CRLF line endings inside a string read as LF.
                '\r' if self.peek() == Some('\n') => {}
                c => out.push(c),
            }
        }
    }

    /// Reads the escape after a `\`. Returns `None` for a line continuation, which skips the
    /// newline and any leading spaces or tabs on the next line.
    fn escape(&mut self) -> Result<Option<char>, Fault> {
        let start = self.pos - 1;
        let Some(c) = self.bump() else {
            return Err(Fault::new(
                Span::new(start, self.pos),
                "unterminated string",
            ));
        };
        let unescaped = match c {
            '\r' | '\n' => {
                if c == '\r' && self.peek() == Some('\n') {
                    self.bump();
                }
                while matches!(self.peek(), Some(' ' | '\t')) {
                    self.bump();
                }
                return Ok(None);
            }
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            '0' => '\0',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'x' => self.hex_escape(start)?,
            'u' => self.unicode_escape(start)?,
            c => {
                return Err(Fault::new(
                    Span::new(start, self.pos),
                    format!("unknown string escape `\\{c}`"),
                ))
            }
        };
        Ok(Some(unescaped))
    }

    /// `\xHH`, limited to ASCII (`\x00`-`\x7f`).
    fn hex_escape(&mut self, start: usize) -> Result<char, Fault> {
        let value = self
            .rest()
            .get(..2)
            .filter(|d| is_hex(d))
            .and_then(|d| u8::from_str_radix(d, 16).ok())
            .filter(|&v| v <= 0x7f);
        let Some(value) = value else {
            return Err(Fault::new(
                Span::new(start, self.pos),
                "invalid `\\x` escape: expected two hex digits from 00 to 7f",
            ));
        };
        self.pos += 2;
        Ok(char::from(value))
    }

    /// `\u{H}` through `\u{HHHHHH}`; underscores may separate digits.
    fn unicode_escape(&mut self, start: usize) -> Result<char, Fault> {
        let invalid = |end| {
            Fault::new(
                Span::new(start, end),
                "invalid `\\u` escape: expected `\\u{...}` with 1 to 6 hex digits",
            )
        };
        let body = self
            .rest()
            .strip_prefix('{')
            .and_then(|r| r.find('}').map(|end| &r[..end]))
            .ok_or_else(|| invalid(self.pos))?;
        let end = self.pos + body.len() + 2;
        let digits: String = body.chars().filter(|&c| c != '_').collect();
        if digits.len() > 6 || !is_hex(&digits) {
            return Err(invalid(end));
        }
        let c = u32::from_str_radix(&digits, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| invalid(end))?;
        self.pos = end;
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<ExprKind> {
        read(src).unwrap().into_iter().map(|e| e.kind).collect()
    }

    fn message(src: &str) -> String {
        read(src).unwrap_err().message
    }

    #[test]
    fn reads_atoms_and_lists() {
        assert_eq!(
            kinds(r#"() ( ) true false 42 -7 "s" env/get weird.name-with$stuff -x"#),
            [
                ExprKind::Unit,
                ExprKind::Unit,
                ExprKind::Bool(true),
                ExprKind::Bool(false),
                ExprKind::Int(42),
                ExprKind::Int(-7),
                ExprKind::Str("s".into()),
                ExprKind::Name("env/get".into()),
                ExprKind::Name("weird.name-with$stuff".into()),
                ExprKind::Name("-x".into()),
            ]
        );
        let [ExprKind::List(items)] = &kinds("(a (b))")[..] else {
            panic!("expected one list");
        };
        assert_eq!(items[0].kind, ExprKind::Name("a".into()));
        assert!(matches!(&items[1].kind, ExprKind::List(inner) if inner.len() == 1));
    }

    #[test]
    fn skips_comments() {
        assert_eq!(
            kinds("; line\n;; doc-style\n#| block #| nested |# ; |# ignored\n|# 1 ; trailing"),
            [ExprKind::Int(1)]
        );
        assert_eq!(kinds("; comment without newline"), []);
    }

    #[test]
    fn string_escapes_match_ketos() {
        assert_eq!(
            kinds(r#""a\\b\"c\'d\0\n\r\t\x41\x7f\u{1F600}\u{1_F6_00}""#),
            [ExprKind::Str(
                "a\\b\"c'd\0\n\r\tA\x7f\u{1F600}\u{1F600}".into()
            )]
        );
        assert_eq!(
            kinds("\"one \\\n    two\" \"crlf\r\nline\""),
            [
                ExprKind::Str("one two".into()),
                ExprKind::Str("crlf\nline".into())
            ]
        );
        for bad in [
            r#""\q""#,
            r#""\x80""#,
            r#""\x4""#,
            r#""\x+1""#,
            r#""\u{}""#,
            r#""\u{+1}""#,
            r#""\u{1234567}""#,
        ] {
            assert!(read(bad).is_err(), "expected {bad} to be rejected");
        }
    }

    #[test]
    fn rejects_syntax_outside_shadowlisp() {
        for (src, expected) in [
            ("'a", r#"unexpected character '\''"#),
            ("`(a)", "unexpected character '`'"),
            (":rest", "unexpected character ':'"),
            ("#'a'", "unexpected character '#'"),
            ("1.5", "invalid integer literal `1.5`"),
            ("1abc", "invalid integer literal `1abc`"),
            ("99999999999999999999", "integer literal is out of range"),
        ] {
            assert_eq!(message(src), expected, "{src}");
        }
    }

    #[test]
    fn reports_unbalanced_input_with_position() {
        let err = read("(a\n  (b)").unwrap_err();
        assert_eq!(
            (err.message.as_str(), err.span),
            ("missing close paren", Span::new(0, 1))
        );
        let err = read("(a))").unwrap_err();
        assert_eq!(
            (err.message.as_str(), err.span),
            ("unmatched `)`", Span::new(3, 4))
        );
        assert_eq!(message("\"abc"), "unterminated string");
        assert_eq!(message("#| abc"), "unterminated block comment");
    }

    #[test]
    fn limits_nesting() {
        let ok = format!("{}{}", "(a ".repeat(MAX_NESTING), ")".repeat(MAX_NESTING));
        read(&ok).unwrap();
        let deep = format!(
            "{}{}",
            "(a ".repeat(MAX_NESTING + 1),
            ")".repeat(MAX_NESTING + 1)
        );
        assert_eq!(message(&deep), "expressions are nested too deeply");
    }
}

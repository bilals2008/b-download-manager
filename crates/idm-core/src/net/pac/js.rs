//! The JavaScript a proxy auto-config script is written in.
//!
//! Only a small part of it is used: comparisons, logical operators, strings,
//! numbers, member access, calls to the PAC helper functions and
//! parentheses. This is a parser and an interpreter for that part — there is
//! no JavaScript engine here, and the script gets no access to anything but
//! the values it builds itself.
//!
//! Everything else is refused with an error rather than guessed at: a wrong
//! answer would send the reader's downloads through a proxy the script did
//! not choose.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::helpers::{self, Clock};
use super::PacError;

/// How many steps a script may take before it is stopped: a script that
/// loops forever must not hold the engine.
const MAX_STEPS: usize = 500_000;

/// How deep a call may nest before it is refused.
const MAX_DEPTH: usize = 64;

/// The helper functions that are in scope before the script runs.
const NATIVES: &[&str] = &[
    "isPlainHostName",
    "dnsDomainIs",
    "localHostOrDomainIs",
    "dnsResolve",
    "isResolvable",
    "myIpAddress",
    "dnsDomainLevels",
    "convert_addr",
    "isInNet",
    "shExpMatch",
    "weekdayRange",
    "timeRange",
    "alert",
];

/// A value the script works with.
#[derive(Clone, Debug)]
pub(super) enum Value {
    /// A variable that was never given a value.
    Undefined,
    /// `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A number.
    Num(f64),
    /// A string.
    Str(String),
    /// A list of values.
    Array(Rc<Vec<Value>>),
    /// A function the script defined.
    Closure(Rc<Closure>),
    /// One of the helper functions.
    Native(&'static str),
}

/// Two values are the same when they are the same kind and hold the same
/// thing. Two functions are the same only when they are the same function.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Undefined, Value::Undefined) => true,
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
            (Value::Native(a), Value::Native(b)) => a == b,
            _ => false,
        }
    }
}

/// A function the script defined.
#[derive(Debug)]
pub(super) struct Closure {
    /// The names of its arguments.
    params: Vec<String>,
    /// What it does.
    body: Rc<Vec<Stmt>>,
    /// The scope it was defined in.
    scope: Scope,
}

/// The variables in scope.
type Scope = Rc<RefCell<ScopeData>>;

/// The variables of one scope, and the scope it was made in.
#[derive(Debug, Default)]
struct ScopeData {
    /// The variables of this scope.
    vars: HashMap<String, Value>,
    /// The scope this one was made in.
    parent: Option<Scope>,
}

impl ScopeData {
    /// The value of a name, looked for in this scope and the ones above it.
    fn get(&self, name: &str) -> Option<Value> {
        if let Some(value) = self.vars.get(name) {
            return Some(value.clone());
        }
        self.parent.as_ref().and_then(|parent| parent.borrow().get(name))
    }
}

/// A scope with that scope above it.
fn scope_in(parent: Option<Scope>) -> Scope {
    Rc::new(RefCell::new(ScopeData {
        vars: HashMap::new(),
        parent,
    }))
}

/// A value as text, the way `String(value)` gives it.
pub(super) fn as_text(value: &Value) -> String {
    match value {
        Value::Undefined => "undefined".to_owned(),
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => if *flag { "true" } else { "false" }.to_owned(),
        Value::Num(number) => format!("{number}"),
        Value::Str(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(as_text)
            .collect::<Vec<String>>()
            .join(","),
        Value::Closure(_) => "function".to_owned(),
        Value::Native(name) => (*name).to_owned(),
    }
}

/// Whether a value counts as true.
pub(super) fn truthy(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Num(number) => *number != 0.0 && !number.is_nan(),
        Value::Str(text) => !text.is_empty(),
        Value::Array(_) | Value::Closure(_) | Value::Native(_) => true,
    }
}

/// The number of a value, the way `parseInt(value, 10)` gives it.
pub(super) fn to_int(value: &Value) -> Option<i64> {
    match value {
        Value::Num(number) => {
            if number.is_finite() {
                Some(*number as i64)
            } else {
                None
            }
        }
        Value::Bool(flag) => Some(i64::from(*flag)),
        Value::Str(text) => {
            let text = text.trim();
            let bytes = text.as_bytes();
            let mut end = 0;
            if end < bytes.len() && (bytes[end] == b'-' || bytes[end] == b'+') {
                end += 1;
            }
            let digits = end;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end == digits {
                return None;
            }
            text[..end].parse::<i64>().ok()
        }
        _ => None,
    }
}

/// Whether two values are equal, the way `==` decides it.
fn loose_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Undefined, Value::Undefined) => true,
        (Value::Null, Value::Null) => true,
        (Value::Undefined, Value::Null) | (Value::Null, Value::Undefined) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Num(a), Value::Num(b)) => a == b,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
        (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
        (Value::Native(a), Value::Native(b)) => a == b,
        (Value::Bool(a), other) => loose_eq(&Value::Num(f64::from(*a)), other),
        (other, Value::Bool(b)) => loose_eq(other, &Value::Num(f64::from(*b))),
        (Value::Num(a), Value::Str(b)) => loose_eq(&Value::Num(*a), &Value::Num(text_to_num(b))),
        (Value::Str(a), Value::Num(b)) => loose_eq(&Value::Num(text_to_num(a)), &Value::Num(*b)),
        _ => false,
    }
}

/// Whether two values are equal, the way `===` decides it: the two sides
/// must be of the same type, so a number is never the same as the text that
/// spells it.
fn strict_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Num(a), Value::Num(b)) => a == b,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Null, Value::Null) => true,
        (Value::Undefined, Value::Undefined) => true,
        (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
        (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
        (Value::Native(a), Value::Native(b)) => a == b,
        _ => false,
    }
}

/// The number a string holds, or `NaN`.
fn text_to_num(text: &str) -> f64 {
    let text = text.trim();
    if text.is_empty() {
        return 0.0;
    }
    text.parse::<f64>().unwrap_or(f64::NAN)
}

/// Whether two values stand in that order to each other.
///
/// Values that cannot be compared (a string and a list) are not in an order
/// at all, so the answer is false rather than a guess.
fn matches_order(left: &Value, right: &Value, order: Order) -> bool {
    let Some(placing) = placing(left, right) else {
        return false;
    };
    match order {
        Order::Less => placing.is_lt(),
        Order::LessOrEqual => placing.is_le(),
        Order::Greater => placing.is_gt(),
        Order::GreaterOrEqual => placing.is_ge(),
    }
}

/// Where two values stand to each other, when they can be compared.
fn placing(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (Value::Bool(a), other) => placing(&Value::Num(f64::from(*a)), other),
        (other, Value::Bool(b)) => placing(other, &Value::Num(f64::from(*b))),
        (Value::Num(a), Value::Num(b)) => a.partial_cmp(b),
        (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
        (Value::Num(a), Value::Str(b)) => a.partial_cmp(&text_to_num(b)),
        (Value::Str(a), Value::Num(b)) => text_to_num(a).partial_cmp(b),
        _ => None,
    }
}

// The tokens of the script.

/// One token of the script.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A name.
    Ident(String),
    /// A string.
    Str(String),
    /// A number.
    Num(f64),
    /// A mark or an operator.
    Op(&'static str),
    /// The end of the script.
    Eof,
}

/// A token and the line it is on.
#[derive(Clone, Debug)]
struct Token {
    /// The token.
    tok: Tok,
    /// The line it is on, for the messages.
    line: usize,
}

/// Reads the script into tokens.
struct Lexer<'a> {
    /// The script.
    src: &'a str,
    /// Where the reading is.
    at: usize,
    /// The line the reading is on.
    line: usize,
}

impl<'a> Lexer<'a> {
    /// The next character, without taking it.
    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.at).copied()
    }

    /// The character after that one.
    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.src.as_bytes().get(self.at + ahead).copied()
    }

    /// The next character, as a character.
    fn peek_char(&self) -> Option<char> {
        self.src.get(self.at..).and_then(|rest| rest.chars().next())
    }

    /// Skips the spaces and the comments.
    fn skip_trivia(&mut self) -> Result<(), PacError> {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => {
                    if c == b'\n' {
                        self.line += 1;
                    }
                    self.at += 1;
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.at += 1;
                    }
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    self.at += 2;
                    loop {
                        match self.peek_char() {
                            None => {
                                return Err(PacError::syntax(self.line, "the comment is not closed"))
                            }
                            Some('\n') => {
                                self.line += 1;
                                self.at += 1;
                            }
                            Some('*') if self.peek_at(1) == Some(b'/') => {
                                self.at += 2;
                                break;
                            }
                            Some(c) => self.at += c.len_utf8(),
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// Reads a string.
    fn string(&mut self, quote: char) -> Result<String, PacError> {
        self.at += quote.len_utf8();
        let mut out = String::new();
        loop {
            let Some(c) = self.peek_char() else {
                return Err(PacError::syntax(self.line, "the string is not closed"));
            };
            self.at += c.len_utf8();
            if c == quote {
                return Ok(out);
            }
            if c == '\n' {
                self.line += 1;
                out.push(c);
                continue;
            }
            if c != '\\' {
                out.push(c);
                continue;
            }
            let Some(escaped) = self.peek_char() else {
                return Err(PacError::syntax(self.line, "the string is not closed"));
            };
            self.at += escaped.len_utf8();
            match escaped {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '0' => out.push('\0'),
                'u' => out.push(self.escaped_code()?),
                other => out.push(other),
            }
        }
    }

    /// Reads the four hex digits of a `\\u` escape.
    fn escaped_code(&mut self) -> Result<char, PacError> {
        let start = self.at;
        for _ in 0..4 {
            match self.peek() {
                Some(c) if c.is_ascii_hexdigit() => self.at += 1,
                _ => return Err(PacError::syntax(self.line, "the escape is not a character")),
            }
        }
        let code = u32::from_str_radix(&self.src[start..self.at], 16)
            .map_err(|_| PacError::syntax(self.line, "the escape is not a character"))?;
        char::from_u32(code).ok_or_else(|| PacError::syntax(self.line, "the escape is not a character"))
    }

    /// Reads a number.
    fn number(&mut self) -> Result<f64, PacError> {
        let start = self.at;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.at += 1;
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.at += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            let mark = self.at;
            self.at += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.at += 1;
            }
            if matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.at += 1;
                }
            } else {
                self.at = mark;
            }
        }
        self.src[start..self.at]
            .parse::<f64>()
            .map_err(|_| PacError::syntax(self.line, "the number cannot be read"))
    }

    /// Reads a name.
    fn ident(&mut self) -> String {
        let start = self.at;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                self.at += 1;
            } else {
                break;
            }
        }
        self.src[start..self.at].to_owned()
    }

    /// Reads a mark or an operator.
    fn op(&mut self) -> Result<&'static str, PacError> {
        // The three-character marks are read first: `===` starts with `==`,
        // and reading the shorter one first would leave a `=` behind that
        // the parser cannot use.
        let three = self.src.get(self.at..self.at + 3).unwrap_or("");
        for candidate in ["===", "!=="] {
            if three.starts_with(candidate) {
                self.at += 3;
                return Ok(candidate);
            }
        }
        let two = self.src.get(self.at..self.at + 2).unwrap_or("");
        for candidate in ["==", "!=", "&&", "||", "<=", ">="] {
            if two.starts_with(candidate) {
                self.at += 2;
                return Ok(candidate);
            }
        }
        let c = self.src.as_bytes()[self.at];
        self.at += 1;
        Ok(match c {
            b'(' => "(",
            b')' => ")",
            b'{' => "{",
            b'}' => "}",
            b'[' => "[",
            b']' => "]",
            b',' => ",",
            b';' => ";",
            b'.' => ".",
            b'!' => "!",
            b'=' => "=",
            b'?' => "?",
            b':' => ":",
            b'+' => "+",
            b'-' => "-",
            b'*' => "*",
            b'/' => "/",
            b'%' => "%",
            b'<' => "<",
            b'>' => ">",
            _ => {
                return Err(PacError::syntax(
                    self.line,
                    "that character is not part of the language",
                ))
            }
        })
    }
}

/// The words the language knows by name. They are read as marks, so a
/// script cannot use one of them as a name of its own.
const KEYWORDS: &[&str] = &[
    "var", "if", "else", "return", "function", "true", "false", "null", "undefined",
];

/// Reads the script into tokens.
fn tokenize(src: &str) -> Result<Vec<Token>, PacError> {
    let mut lexer = Lexer { src, at: 0, line: 1 };
    let mut tokens = Vec::new();
    loop {
        lexer.skip_trivia()?;
        let line = lexer.line;
        let Some(c) = lexer.peek() else {
            tokens.push(Token {
                tok: Tok::Eof,
                line,
            });
            return Ok(tokens);
        };
        let tok = match c {
            b'"' | b'\'' => Tok::Str(lexer.string(c as char)?),
            b'0'..=b'9' => Tok::Num(lexer.number()?),
            c if c.is_ascii_alphabetic() || c == b'_' || c == b'$' => {
                let name = lexer.ident();
                match KEYWORDS.iter().find(|word| **word == name) {
                    Some(word) => Tok::Op(word),
                    None => Tok::Ident(name),
                }
            }
            _ => Tok::Op(lexer.op()?),
        };
        tokens.push(Token { tok, line });
    }
}

// What the script says.

/// The orders two values can be compared in.
#[derive(Debug, Clone, Copy)]
enum Order {
    /// `<`
    Less,
    /// `<=`
    LessOrEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterOrEqual,
}

/// A value in the script.
#[derive(Debug)]
pub(super) enum Expr {
    /// A string.
    Str(String),
    /// A number.
    Num(f64),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
    /// `undefined`.
    Undefined,
    /// A name.
    Ident(String),
    /// `object.property`.
    Member {
        /// The object.
        object: Box<Expr>,
        /// The name of the property.
        property: String,
    },
    /// `object[index]`.
    Index {
        /// The object.
        object: Box<Expr>,
        /// The index.
        index: Box<Expr>,
    },
    /// A call.
    Call {
        /// What is called.
        callee: Box<Expr>,
        /// The arguments.
        args: Vec<Expr>,
    },
    /// `!value`.
    Not(Box<Expr>),
    /// `left && right`.
    And(Box<Expr>, Box<Expr>),
    /// `left || right`.
    Or(Box<Expr>, Box<Expr>),
    /// `left == right` or `left != right`.
    Eq {
        /// The left side.
        left: Box<Expr>,
        /// The right side.
        right: Box<Expr>,
        /// Whether it is `==`.
        equal: bool,
        /// Whether it is `===`: the two sides must have the same type.
        strict: bool,
    },
    /// `left < right` and the other orders.
    Order {
        /// The left side.
        left: Box<Expr>,
        /// The right side.
        right: Box<Expr>,
        /// Which order it is.
        order: Order,
    },
    /// `[a, b, c]`.
    Array(Vec<Expr>),
}

/// Something the script does.
#[derive(Debug)]
pub(super) enum Stmt {
    /// `var name = value;`
    Var {
        /// The name.
        name: String,
        /// The value, when it was given.
        value: Option<Expr>,
    },
    /// `return value;`
    Return(Option<Expr>),
    /// `if (cond) ... else ...`
    If {
        /// The condition.
        cond: Expr,
        /// What happens when it is true.
        then: Box<Stmt>,
        /// What happens when it is false.
        alt: Option<Box<Stmt>>,
    },
    /// `{ ... }`
    Block(Vec<Stmt>),
    /// A value that is thrown away.
    Expr(Expr),
    /// `function name() { ... }`
    Func {
        /// The name of the function.
        name: String,
        /// The names of its arguments.
        params: Vec<String>,
        /// What it does.
        body: Rc<Vec<Stmt>>,
    },
}

/// Reads the script into what it says.
struct Parser {
    /// The tokens.
    tokens: Vec<Token>,
    /// Which one is next.
    at: usize,
}

impl Parser {
    /// The token that is next.
    fn peek(&self) -> &Tok {
        &self.tokens[self.at.min(self.tokens.len() - 1)].tok
    }

    /// The line the next token is on.
    fn line(&self) -> usize {
        self.tokens[self.at.min(self.tokens.len() - 1)].line
    }

    /// Whether the next token is that mark.
    fn at_op(&self, op: &str) -> bool {
        matches!(self.peek(), Tok::Op(mark) if *mark == op)
    }

    /// Takes the next token.
    fn bump(&mut self) -> Tok {
        let tok = self.tokens[self.at.min(self.tokens.len() - 1)].tok.clone();
        if self.at < self.tokens.len() - 1 {
            self.at += 1;
        }
        tok
    }

    /// Takes the next token when it is that mark.
    fn eat(&mut self, op: &str) -> bool {
        if self.at_op(op) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Takes the next token, which must be that mark.
    fn expect(&mut self, op: &str) -> Result<(), PacError> {
        if self.eat(op) {
            Ok(())
        } else {
            Err(PacError::syntax(self.line(), format!("'{op}' is expected")))
        }
    }

    /// Reads a name.
    fn ident(&mut self) -> Result<String, PacError> {
        match self.bump() {
            Tok::Ident(name) => Ok(name),
            _ => Err(PacError::syntax(self.line(), "a name is expected")),
        }
    }

    /// Reads the statements of the script.
    fn program(&mut self) -> Result<Vec<Stmt>, PacError> {
        let mut stmts = Vec::new();
        while !matches!(self.peek(), Tok::Eof) {
            stmts.push(self.statement()?);
        }
        Ok(stmts)
    }

    /// Reads one statement.
    fn statement(&mut self) -> Result<Stmt, PacError> {
        match self.peek().clone() {
            Tok::Op("var") => {
                self.bump();
                let name = self.ident()?;
                let value = if self.eat("=") {
                    Some(self.expression()?)
                } else {
                    None
                };
                self.semicolon();
                Ok(Stmt::Var { name, value })
            }
            Tok::Op("return") => {
                self.bump();
                let value = if self.at_op(";") || self.at_op("}") {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.semicolon();
                Ok(Stmt::Return(value))
            }
            Tok::Op("if") => {
                self.bump();
                self.expect("(")?;
                let cond = self.expression()?;
                self.expect(")")?;
                let then = Box::new(self.statement()?);
                let alt = if self.eat("else") {
                    Some(Box::new(self.statement()?))
                } else {
                    None
                };
                Ok(Stmt::If { cond, then, alt })
            }
            Tok::Op("{") => Ok(Stmt::Block(self.block()?)),
            Tok::Op("function") => self.function(),
            _ => {
                let expr = self.expression()?;
                self.semicolon();
                Ok(Stmt::Expr(expr))
            }
        }
    }

    /// Reads the `;` after a statement, when there is one.
    fn semicolon(&mut self) {
        self.eat(";");
    }

    /// Reads the statements of a block.
    fn block(&mut self) -> Result<Vec<Stmt>, PacError> {
        self.expect("{")?;
        let mut stmts = Vec::new();
        while !self.at_op("}") {
            if matches!(self.peek(), Tok::Eof) {
                return Err(PacError::syntax(self.line(), "the block is not closed"));
            }
            stmts.push(self.statement()?);
        }
        self.bump();
        Ok(stmts)
    }

    /// Reads a function.
    fn function(&mut self) -> Result<Stmt, PacError> {
        self.bump();
        let name = self.ident()?;
        self.expect("(")?;
        let mut params = Vec::new();
        if !self.at_op(")") {
            loop {
                params.push(self.ident()?);
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect(")")?;
        let body = Rc::new(self.block()?);
        Ok(Stmt::Func { name, params, body })
    }

    /// Reads a value.
    fn expression(&mut self) -> Result<Expr, PacError> {
        self.or()
    }

    /// Reads `left || right`.
    fn or(&mut self) -> Result<Expr, PacError> {
        let mut left = self.and()?;
        while self.eat("||") {
            let right = self.and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Reads `left && right`.
    fn and(&mut self) -> Result<Expr, PacError> {
        let mut left = self.comparison()?;
        while self.eat("&&") {
            let right = self.comparison()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Reads `left < right` and the other orders.
    fn comparison(&mut self) -> Result<Expr, PacError> {
        let mut left = self.equality()?;
        loop {
            let order = if self.eat("<") {
                Some(Order::Less)
            } else if self.eat("<=") {
                Some(Order::LessOrEqual)
            } else if self.eat(">") {
                Some(Order::Greater)
            } else if self.eat(">=") {
                Some(Order::GreaterOrEqual)
            } else {
                None
            };
            match order {
                Some(order) => {
                    let right = self.equality()?;
                    left = Expr::Order {
                        left: Box::new(left),
                        right: Box::new(right),
                        order,
                    };
                }
                None => return Ok(left),
            }
        }
    }

    /// Reads `left == right` and `left != right`.
    fn equality(&mut self) -> Result<Expr, PacError> {
        let mut left = self.unary()?;
        loop {
            let (equal, strict) = if self.eat("===") {
                (true, true)
            } else if self.eat("!==") {
                (false, true)
            } else if self.eat("==") {
                (true, false)
            } else if self.eat("!=") {
                (false, false)
            } else {
                return Ok(left);
            };
            let right = self.unary()?;
            left = Expr::Eq {
                left: Box::new(left),
                right: Box::new(right),
                equal,
                strict,
            };
        }
    }

    /// Reads `!value`.
    fn unary(&mut self) -> Result<Expr, PacError> {
        if self.eat("!") {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.postfix()
    }

    /// Reads a value with its properties and calls.
    fn postfix(&mut self) -> Result<Expr, PacError> {
        let mut expr = self.primary()?;
        loop {
            if self.eat(".") {
                let property = self.ident()?;
                expr = Expr::Member {
                    object: Box::new(expr),
                    property,
                };
            } else if self.eat("[") {
                let index = self.expression()?;
                self.expect("]")?;
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                };
            } else if self.at_op("(") {
                let args = self.arguments()?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                };
            } else {
                return Ok(expr);
            }
        }
    }

    /// Reads the arguments of a call.
    fn arguments(&mut self) -> Result<Vec<Expr>, PacError> {
        self.expect("(")?;
        let mut args = Vec::new();
        if !self.at_op(")") {
            loop {
                args.push(self.expression()?);
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect(")")?;
        Ok(args)
    }

    /// Reads one value.
    fn primary(&mut self) -> Result<Expr, PacError> {
        match self.peek().clone() {
            Tok::Str(text) => {
                self.bump();
                Ok(Expr::Str(text))
            }
            Tok::Num(number) => {
                self.bump();
                Ok(Expr::Num(number))
            }
            Tok::Ident(name) => {
                self.bump();
                Ok(Expr::Ident(name))
            }
            Tok::Op("true") => {
                self.bump();
                Ok(Expr::Bool(true))
            }
            Tok::Op("false") => {
                self.bump();
                Ok(Expr::Bool(false))
            }
            Tok::Op("null") => {
                self.bump();
                Ok(Expr::Null)
            }
            Tok::Op("undefined") => {
                self.bump();
                Ok(Expr::Undefined)
            }
            Tok::Op("(") => {
                self.bump();
                let expr = self.expression()?;
                self.expect(")")?;
                Ok(expr)
            }
            Tok::Op("[") => {
                self.bump();
                let mut items = Vec::new();
                if !self.at_op("]") {
                    loop {
                        items.push(self.expression()?);
                        if !self.eat(",") {
                            break;
                        }
                    }
                }
                self.expect("]")?;
                Ok(Expr::Array(items))
            }
            _ => Err(PacError::syntax(self.line(), "a value is expected")),
        }
    }
}

/// Reads the script into what it says.
fn parse_program(script: &str) -> Result<Vec<Stmt>, PacError> {
    let tokens = tokenize(script)?;
    let mut parser = Parser { tokens, at: 0 };
    parser.program()
}

/// Runs the script and asks it where `url` goes.
pub(super) fn call_entry(
    script: &str,
    url: &str,
    host: &str,
    clock: &Clock,
) -> Result<String, PacError> {
    let stmts = parse_program(script)?;

    let globals = scope_in(None);
    for name in NATIVES {
        globals
            .borrow_mut()
            .vars
            .insert((*name).to_owned(), Value::Native(name));
    }
    let mut interp = Interp {
        clock: *clock,
        steps: 0,
        depth: 0,
    };
    for stmt in &stmts {
        if interp.exec(stmt, &globals)?.is_some() {
            return Err(PacError::runtime(
                "the script answered outside FindProxyForURL",
            ));
        }
    }
    let entry = globals.borrow().get("FindProxyForURL");
    let Some(Value::Closure(closure)) = entry else {
        return Err(PacError::NoEntryPoint);
    };
    let answer = interp.call(
        &closure,
        &[Value::Str(url.to_owned()), Value::Str(host.to_owned())],
    )?;
    match answer {
        Value::Str(text) => Ok(text),
        other => Err(PacError::runtime(format!(
            "FindProxyForURL answered {}",
            as_text(&other)
        ))),
    }
}

/// Runs what the script says.
struct Interp {
    /// The clock the date helpers read.
    clock: Clock,
    /// How much the script has done so far.
    steps: usize,
    /// How deep the calls are.
    depth: usize,
}

impl Interp {
    /// Counts a step, and stops a script that does too much.
    fn step(&mut self) -> Result<(), PacError> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(PacError::runtime("the script takes too long"));
        }
        Ok(())
    }

    /// Does one statement, and gives what a `return` returned.
    fn exec(&mut self, stmt: &Stmt, scope: &Scope) -> Result<Option<Value>, PacError> {
        self.step()?;
        match stmt {
            Stmt::Var { name, value } => {
                let value = match value {
                    Some(expr) => self.eval(expr, scope)?,
                    None => Value::Undefined,
                };
                scope.borrow_mut().vars.insert(name.clone(), value);
                Ok(None)
            }
            Stmt::Return(expr) => {
                let value = match expr {
                    Some(expr) => self.eval(expr, scope)?,
                    None => Value::Undefined,
                };
                Ok(Some(value))
            }
            Stmt::If { cond, then, alt } => {
                let condition = self.eval(cond, scope)?;
                if truthy(&condition) {
                    self.exec(then, scope)
                } else if let Some(alt) = alt {
                    self.exec(alt, scope)
                } else {
                    Ok(None)
                }
            }
            Stmt::Block(stmts) => {
                for stmt in stmts {
                    if let Some(value) = self.exec(stmt, scope)? {
                        return Ok(Some(value));
                    }
                }
                Ok(None)
            }
            Stmt::Expr(expr) => {
                self.eval(expr, scope)?;
                Ok(None)
            }
            Stmt::Func { name, params, body } => {
                let closure = Value::Closure(Rc::new(Closure {
                    params: params.clone(),
                    body: body.clone(),
                    scope: scope.clone(),
                }));
                scope.borrow_mut().vars.insert(name.clone(), closure);
                Ok(None)
            }
        }
    }

    /// Calls a function the script defined.
    fn call(&mut self, closure: &Rc<Closure>, args: &[Value]) -> Result<Value, PacError> {
        if self.depth >= MAX_DEPTH {
            return Err(PacError::runtime("the script calls itself too deeply"));
        }
        self.depth += 1;
        let scope = scope_in(Some(closure.scope.clone()));
        for (at, param) in closure.params.iter().enumerate() {
            let value = args.get(at).cloned().unwrap_or(Value::Undefined);
            scope.borrow_mut().vars.insert(param.clone(), value);
        }
        let answer = self.run_body(closure, &scope);
        self.depth -= 1;
        answer
    }

    /// Does what a function does, and gives what it returned.
    fn run_body(&mut self, closure: &Rc<Closure>, scope: &Scope) -> Result<Value, PacError> {
        for stmt in closure.body.iter() {
            if let Some(value) = self.exec(stmt, scope)? {
                return Ok(value);
            }
        }
        Ok(Value::Undefined)
    }

    /// The value of a piece of the script.
    fn eval(&mut self, expr: &Expr, scope: &Scope) -> Result<Value, PacError> {
        self.step()?;
        match expr {
            Expr::Str(text) => Ok(Value::Str(text.clone())),
            Expr::Num(number) => Ok(Value::Num(*number)),
            Expr::Bool(flag) => Ok(Value::Bool(*flag)),
            Expr::Null => Ok(Value::Null),
            Expr::Undefined => Ok(Value::Undefined),
            Expr::Ident(name) => scope.borrow().get(name).ok_or_else(|| {
                PacError::unsupported(format!("the script uses the unknown name '{name}'"))
            }),
            Expr::Member { object, property } => {
                let object = self.eval(object, scope)?;
                member(&object, property)
            }
            Expr::Index { object, index } => {
                let object = self.eval(object, scope)?;
                let index = self.eval(index, scope)?;
                indexed(&object, &index)
            }
            Expr::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, scope)?);
                }
                Ok(Value::Array(Rc::new(values)))
            }
            Expr::Not(inner) => Ok(Value::Bool(!truthy(&self.eval(inner, scope)?))),
            Expr::And(left, right) => {
                let left = self.eval(left, scope)?;
                if truthy(&left) {
                    self.eval(right, scope)
                } else {
                    Ok(left)
                }
            }
            Expr::Or(left, right) => {
                let left = self.eval(left, scope)?;
                if truthy(&left) {
                    Ok(left)
                } else {
                    self.eval(right, scope)
                }
            }
            Expr::Eq {
                left,
                right,
                equal,
                strict,
            } => {
                let left = self.eval(left, scope)?;
                let right = self.eval(right, scope)?;
                let same = if *strict {
                    strict_eq(&left, &right)
                } else {
                    loose_eq(&left, &right)
                };
                Ok(Value::Bool(if *equal { same } else { !same }))
            }
            Expr::Order {
                left,
                right,
                order,
            } => {
                let left = self.eval(left, scope)?;
                let right = self.eval(right, scope)?;
                Ok(Value::Bool(matches_order(&left, &right, *order)))
            }
            Expr::Call { callee, args } => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, scope)?);
                }
                match callee.as_ref() {
                    Expr::Member { object, property } => {
                        let object = self.eval(object, scope)?;
                        method(&object, property, &values)
                    }
                    _ => {
                        let called = self.eval(callee, scope)?;
                        self.call_value(&called, &values)
                    }
                }
            }
        }
    }

    /// Calls something that is a function.
    fn call_value(&mut self, called: &Value, args: &[Value]) -> Result<Value, PacError> {
        match called {
            Value::Closure(closure) => self.call(closure, args),
            Value::Native(name) => match helpers::call(name, args, &self.clock) {
                Some(answer) => answer,
                None => Err(PacError::unsupported(format!(
                    "the script calls the unknown function '{name}'"
                ))),
            },
            _ => Err(PacError::runtime(
                "the script calls something that is not a function",
            )),
        }
    }
}

/// The property of a value.
fn member(object: &Value, property: &str) -> Result<Value, PacError> {
    match object {
        Value::Str(text) => match property {
            "length" => Ok(Value::Num(text.chars().count() as f64)),
            _ => Err(PacError::unsupported(format!(
                "a string has no property '{property}'"
            ))),
        },
        Value::Array(items) => match property {
            "length" => Ok(Value::Num(items.len() as f64)),
            _ => Err(PacError::unsupported(format!(
                "a list has no property '{property}'"
            ))),
        },
        Value::Null => Err(PacError::runtime(
            "the script reads a property of null",
        )),
        Value::Undefined => Err(PacError::runtime(
            "the script reads a property of undefined",
        )),
        _ => Err(PacError::unsupported(format!(
            "the script reads the property '{property}' of something else"
        ))),
    }
}

/// The item of a list.
fn indexed(object: &Value, index: &Value) -> Result<Value, PacError> {
    let Value::Array(items) = object else {
        return Err(PacError::runtime("the script indexes something that is not a list"));
    };
    let at = to_int(index).unwrap_or(0);
    if at < 0 || at as usize >= items.len() {
        return Ok(Value::Undefined);
    }
    Ok(items[at as usize].clone())
}

/// A method of a string or a list.
fn method(object: &Value, name: &str, args: &[Value]) -> Result<Value, PacError> {
    match object {
        Value::Str(text) => string_method(text, name, args),
        Value::Array(items) => array_method(items, name, args),
        Value::Null | Value::Undefined => {
            Err(PacError::runtime("the script calls a method of nothing"))
        }
        _ => Err(PacError::unsupported(format!(
            "the script calls the unknown method '{name}'"
        ))),
    }
}

/// The number an argument holds.
fn argument_number(args: &[Value], at: usize) -> i64 {
    to_int(&args.get(at).cloned().unwrap_or(Value::Undefined)).unwrap_or(0)
}

/// The text an argument holds.
fn argument_text(args: &[Value], at: usize) -> String {
    args.get(at).map(as_text).unwrap_or_default()
}

/// A method of a string.
fn string_method(text: &str, name: &str, args: &[Value]) -> Result<Value, PacError> {
    let characters: Vec<char> = text.chars().collect();
    let number = |at: usize| argument_number(args, at);
    Ok(match name {
        "length" => Value::Num(characters.len() as f64),
        "charAt" => {
            let at = number(0);
            match characters.get(at.max(0) as usize) {
                Some(c) => Value::Str(c.to_string()),
                None => Value::Str(String::new()),
            }
        }
        "indexOf" => {
            let needle = argument_text(args, 0);
            match text.find(&needle) {
                Some(at) => Value::Num(text[..at].chars().count() as f64),
                None => Value::Num(-1.0),
            }
        }
        "lastIndexOf" => {
            let needle = argument_text(args, 0);
            match text.rfind(&needle) {
                Some(at) => Value::Num(text[..at].chars().count() as f64),
                None => Value::Num(-1.0),
            }
        }
        "startsWith" => Value::Bool(text.starts_with(&argument_text(args, 0))),
        "endsWith" => Value::Bool(text.ends_with(&argument_text(args, 0))),
        "includes" => Value::Bool(text.contains(&argument_text(args, 0))),
        "toLowerCase" => Value::Str(text.to_lowercase()),
        "toUpperCase" => Value::Str(text.to_uppercase()),
        "trim" => Value::Str(text.trim().to_owned()),
        "toString" => Value::Str(text.to_owned()),
        "substring" => {
            let mut from = number(0).max(0) as usize;
            let mut to = if args.len() > 1 {
                number(1).max(0) as usize
            } else {
                characters.len()
            };
            if from > to {
                std::mem::swap(&mut from, &mut to);
            }
            let from = from.min(characters.len());
            let to = to.min(characters.len());
            Value::Str(characters[from..to].iter().collect())
        }
        "slice" => {
            let from = slice_start(number(0), characters.len());
            let to = if args.len() > 1 {
                slice_start(number(1), characters.len())
            } else {
                characters.len()
            };
            let (from, to) = (from.min(characters.len()), to.min(characters.len()));
            if from >= to {
                Value::Str(String::new())
            } else {
                Value::Str(characters[from..to].iter().collect())
            }
        }
        "split" => {
            let separator = argument_text(args, 0);
            let parts: Vec<Value> = if separator.is_empty() {
                characters.iter().map(|c| Value::Str(c.to_string())).collect()
            } else {
                text.split(&separator as &str)
                    .map(|part| Value::Str(part.to_owned()))
                    .collect()
            };
            Value::Array(Rc::new(parts))
        }
        "replace" => {
            let from = argument_text(args, 0);
            let to = argument_text(args, 1);
            Value::Str(text.replacen(&from as &str, &to, 1))
        }
        "concat" => Value::Str(format!("{text}{}", argument_text(args, 0))),
        other => {
            return Err(PacError::unsupported(format!(
                "the script calls the unknown method '{other}' of a string"
            )))
        }
    })
}

/// Where a slice begins or ends, counting from the end when it is negative.
fn slice_start(at: i64, len: usize) -> usize {
    let at = if at < 0 {
        len as i64 + at
    } else {
        at
    };
    at.clamp(0, len as i64) as usize
}

/// A method of a list.
fn array_method(items: &Rc<Vec<Value>>, name: &str, args: &[Value]) -> Result<Value, PacError> {
    Ok(match name {
        "length" => Value::Num(items.len() as f64),
        "indexOf" => {
            let wanted = args.first().cloned().unwrap_or(Value::Undefined);
            match items
                .iter()
                .position(|item| loose_eq(item, &wanted))
            {
                Some(at) => Value::Num(at as f64),
                None => Value::Num(-1.0),
            }
        }
        "includes" => {
            let wanted = args.first().cloned().unwrap_or(Value::Undefined);
            Value::Bool(items.iter().any(|item| loose_eq(item, &wanted)))
        }
        "join" => {
            let separator = args
                .first()
                .map(as_text)
                .unwrap_or_else(|| ",".to_owned());
            Value::Str(
                items
                    .iter()
                    .map(as_text)
                    .collect::<Vec<String>>()
                    .join(&separator),
            )
        }
        "slice" => {
            let from = slice_start(argument_number(args, 0), items.len());
            let to = if args.len() > 1 {
                slice_start(argument_number(args, 1), items.len())
            } else {
                items.len()
            };
            let (from, to) = (from.min(items.len()), to.min(items.len()));
            if from >= to {
                Value::Array(Rc::new(Vec::new()))
            } else {
                Value::Array(Rc::new(items[from..to].to_vec()))
            }
        }
        "concat" => {
            let mut all = items.as_ref().clone();
            if let Some(Value::Array(other)) = args.first() {
                all.extend(other.iter().cloned());
            }
            Value::Array(Rc::new(all))
        }
        "toString" => Value::Str(
            items
                .iter()
                .map(as_text)
                .collect::<Vec<String>>()
                .join(","),
        ),
        other => {
            return Err(PacError::unsupported(format!(
                "the script calls the unknown method '{other}' of a list"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tokens_are_read() {
        let tokens = tokenize("var a = 'x'; // a comment\n/* another */ b(1, 2.5)").unwrap();
        let kinds: Vec<&Tok> = tokens.iter().map(|t| &t.tok).collect();
        assert_eq!(kinds[0], &Tok::Op("var"));
        assert_eq!(kinds[1], &Tok::Ident("a".into()));
        assert_eq!(kinds[2], &Tok::Op("="));
        assert_eq!(kinds[3], &Tok::Str("x".into()));
        assert!(matches!(kinds.last(), Some(Tok::Eof)));
    }

    #[test]
    fn a_broken_script_is_an_error() {
        assert!(parse_program("this is not javascript (").is_err());
        assert!(parse_program("function FindProxyForURL(u, h { return 'x'; }").is_err());
        assert!(parse_program("var x = ;").is_err());
    }
}

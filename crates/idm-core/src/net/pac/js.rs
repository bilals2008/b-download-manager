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
        self.parent
            .as_ref()
            .and_then(|parent| parent.borrow().get(name))
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
        Value::Array(items) => items.iter().map(as_text).collect::<Vec<String>>().join(","),
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

mod builtins;
mod lexer;
mod parser;

use builtins::{indexed, member, method};
use parser::parse_program;
pub use parser::Order;
pub(super) use parser::{Expr, Stmt};

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
            Expr::Order { left, right, order } => {
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

#[cfg(test)]
mod tests {
    use super::lexer::{tokenize, Tok};
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

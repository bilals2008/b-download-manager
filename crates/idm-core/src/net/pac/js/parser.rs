//! Reading the tokens into what the script says.

use super::*;
use super::lexer::{tokenize, Tok, Token};

// What the script says.

/// The orders two values can be compared in.
#[derive(Debug, Clone, Copy)]
pub enum Order {
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
pub(in crate::net::pac) enum Expr {
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
pub(in crate::net::pac) enum Stmt {
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
pub(super) fn parse_program(script: &str) -> Result<Vec<Stmt>, PacError> {
    let tokens = tokenize(script)?;
    let mut parser = Parser { tokens, at: 0 };
    parser.program()
}

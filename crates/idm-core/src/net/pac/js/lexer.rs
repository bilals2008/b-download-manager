//! Reading the script into tokens.

use super::*;

// The tokens of the script.

/// One token of the script.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Tok {
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
pub(super) struct Token {
    /// The token.
    pub(super) tok: Tok,
    /// The line it is on, for the messages.
    pub(super) line: usize,
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
pub(super) fn tokenize(src: &str) -> Result<Vec<Token>, PacError> {
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

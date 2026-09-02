use std::{char, fmt};

pub struct Lexer<'a> {
    src: &'a str,             // borrowed string source
    cursor: usize,            // currently evaluated byte index
    cur_token: Option<Token>, // currently held token, Some(_) when last called is peek
}

#[derive(Debug)]
pub enum LexErr {
    UntermString(char),
    Expect(char, char), // Expect first char, found the second // TODO: do something better
    InvalidNumeric(char), // Found char in numeric expression
    InvalidChar(char),  // Just don't like this char, nothing much
    InvalidWord(char),  // Found the char in a word, not supposed to be there
}

#[derive(Debug, PartialEq)]
pub enum TokenType {
    KeyWord(KeyWord),
    ID,
    Literal(Literal),
    Op(Op),
    Punc(Punc),
}

/// All reserved keywords
// Change this -> change scan_word
// TODO: do we even need this? Just use content?
#[derive(Debug, PartialEq)]
pub enum KeyWord {
    Select,
    From,
    New,
    Table,
    Primary,
    Insert,
    Describe,
}

// Literal values
#[derive(Debug, PartialEq)]
pub enum Literal {
    Int,
    Float,
    String,
    Bool,
}

// Operator used in expression
// change this -> change is op
#[derive(Debug, PartialEq)]
pub enum Op {
    Plus,
    Minus,
    Star, // '*' <- can mean "multiply" or "all columns"
    Div,
    Eq,
    Neq,
}

// Punctuation character in query
// change this -> change is_punctuation, scan_punctuation
#[derive(Debug, PartialEq)]
pub enum Punc {
    Comma,
    Colon,
    Semi,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBrack,
    RBrack,
    Dot,
}

pub struct Token {
    pub token_type: TokenType,
    pub content: String,
}

// Helper for punctuation character
fn is_punctuation(c: char) -> bool {
    matches!(c, ',' | ':' | ';' | '(' | ')' | '{' | '}' | '[' | ']' | '.')
}

// Helper for punctuation character
fn is_op(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/' | '=' | '!')
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Lexer {
        Lexer {
            src,
            cursor: 0,
            cur_token: None,
        }
    }

    /// Peek the next token in the command without popping it
    pub fn peek(&mut self) -> Result<Option<&Token>, LexErr> {
        if self.cur_token.is_none() {
            self.cur_token = self.next()?;
        }

        return Ok(self.cur_token.as_ref());
    }

    /// Return the next token from the command
    pub fn next(&mut self) -> Result<Option<Token>, LexErr> {
        // If already have cur_token ready, just return it
        if self.cur_token.is_some() {
            return Ok(self.cur_token.take());
        }

        let skip_whitespace_res = self.skip_whitespace();
        if skip_whitespace_res == None {
            return Ok(None);
        }
        let first_char = skip_whitespace_res.unwrap();

        let token = match first_char {
            '\'' => self.scan_string_literal()?,
            c if is_punctuation(c) => self.scan_punctuation()?,
            c if is_op(c) => self.scan_op()?,
            '0'..='9' => self.scan_numeric()?,
            c if c.is_ascii_alphabetic() => self.scan_word()?,
            _ => return Err(LexErr::InvalidChar(first_char)),
        };

        return Ok(Some(token));
    }

    /// Move cursor until found non-whitespace character and
    /// return that character (return None if end of string)
    fn skip_whitespace(&mut self) -> Option<char> {
        let skip_len = self.src[self.cursor..].find(|c| !char::is_whitespace(c))?;
        self.cursor += skip_len;
        return self.cur_char();
    }

    /// Move cursor until found whitespace or end
    /// of string and return characters we skip.
    fn to_whitespace(&mut self) -> &str {
        let move_len = self.src[self.cursor..]
            .find(char::is_whitespace)
            .unwrap_or(self.src[self.cursor..].len());

        let res = &self.src[self.cursor..self.cursor + move_len];
        self.cursor += move_len;

        return res;
    }

    // Get current character cursor point to
    fn cur_char(&self) -> Option<char> {
        debug_assert!(self.src.is_char_boundary(self.cursor));
        self.src[self.cursor..].chars().next()
    }

    // Get next character cursor point to
    fn next_char(&self) -> Option<char> {
        debug_assert!(self.src.is_char_boundary(self.cursor));
        self.src[self.cursor..].chars().nth(1)
    }

    /// Scan current cursor trying to tokenize a string literal
    // requires: cursor should point to the first '\'' that string start
    // TODO: need escape sequence for '\'' inside the string
    fn scan_string_literal(&mut self) -> Result<Token, LexErr> {
        debug_assert_eq!(self.cur_char().unwrap(), '\'');

        let string_len = self.src[self.cursor + 1..]
            .find('\'')
            .ok_or(LexErr::UntermString(
                self.src[self.cursor..].chars().next().unwrap(),
            ))?;

        let token = Token {
            token_type: TokenType::Literal(Literal::String),
            content: self.src[self.cursor..self.cursor + string_len + 2].to_string(),
        };

        self.cursor += string_len + 2;

        return Ok(token);
    }

    /// Scan punctuation character and return token
    // requires: cursor should point to ','
    fn scan_punctuation(&mut self) -> Result<Token, LexErr> {
        debug_assert!(is_punctuation(self.cur_char().unwrap()));
        let ch = self.cur_char().unwrap();
        let token_type = match ch {
            ',' => TokenType::Punc(Punc::Comma),
            ':' => TokenType::Punc(Punc::Colon),
            ';' => TokenType::Punc(Punc::Semi),
            '(' => TokenType::Punc(Punc::LParen),
            ')' => TokenType::Punc(Punc::RParen),
            '{' => TokenType::Punc(Punc::LBrace),
            '}' => TokenType::Punc(Punc::RBrace),
            '[' => TokenType::Punc(Punc::LBrack),
            ']' => TokenType::Punc(Punc::RBrack),
            '.' => TokenType::Punc(Punc::Dot),
            _ => panic!("Expect punctuation character, found {}", ch),
        };

        self.cursor += 1; // All the punctuation character takes 1 byte
        return Ok(Token {
            token_type,
            content: ch.to_string(),
        });
    }

    /// Scan and return operator token
    fn scan_op(&mut self) -> Result<Token, LexErr> {
        let ch = self.cur_char().unwrap();
        let mut skip = 1;
        let token_type = match ch {
            '+' => TokenType::Op(Op::Plus),
            '-' => TokenType::Op(Op::Minus),
            '*' => TokenType::Op(Op::Star),
            '/' => TokenType::Op(Op::Div),
            '=' => TokenType::Op(Op::Eq),
            '!' => {
                // The next char must be '='
                self.next_char()
                    .filter(|&c| c == '=')
                    .ok_or(LexErr::Expect('=', '\0'))?;
                skip = 2; // '!' + '=' takes 2 bytes
                TokenType::Op(Op::Neq)
            }
            _ => panic!("Expect operator character, but found {}", ch),
        };

        self.cursor += skip;

        let token = Token {
            token_type,
            content: format!("{}", ch).to_string(),
        };

        return Ok(token);
    }

    /// Scan and return numeric token
    fn scan_numeric(&mut self) -> Result<Token, LexErr> {
        debug_assert!(matches!(self.cur_char().unwrap(), '0'..='9'));

        // Scan numbers, only allow 1 dot. That's it.
        let mut dotted = false;

        let start = self.cursor;
        while let Some(ch) = self.cur_char() {
            if matches!(ch, '0'..='9') {
                self.cursor += 1; // '0' - '9' takes 1 byte
            } else if ch == '.' && !dotted {
                dotted = true;
                self.cursor += 1; // '.' takes 1 byte
            } else if ch.is_whitespace() || is_op(ch) || is_punctuation(ch) {
                // Should only allow whitespace, punctuation, op to end the word
                break;
            } else {
                // Shouldn't be anything other than numbers, '.', and
                // whitespace in a litteral numeric expression.
                return Err(LexErr::InvalidNumeric(ch));
            }
        }

        let token_type = if dotted {
            TokenType::Literal(Literal::Float)
        } else {
            TokenType::Literal(Literal::Int)
        };

        let token = Token {
            token_type,
            content: self.src[start..self.cursor].to_string(),
        };
        return Ok(token);
    }

    /// Scan current cursor trying to get a snake case word.
    // - only allow ASCII alphanumeric and '_'
    // - Must start with ASCII alphabet
    // - If found something else, then it's not a part of the word.
    fn scan_word(&mut self) -> Result<Token, LexErr> {
        debug_assert!(self.cur_char().unwrap().is_ascii_alphabetic());
        let start = self.cursor;
        while let Some(ch) = self.cur_char() {
            if ch.is_ascii_alphabetic() || ch.is_ascii_alphanumeric() || ch == '_' {
                self.cursor += 1;
            } else if ch.is_whitespace() || is_punctuation(ch) {
                // Should only allow whitespace or punctuation to end the word
                break;
            } else {
                return Err(LexErr::InvalidWord(ch));
            }
        }

        let word = &self.src[start..self.cursor];

        let token_type = match word.to_lowercase().as_str() {
            "select" => TokenType::KeyWord(KeyWord::Select),
            "from" => TokenType::KeyWord(KeyWord::From),
            "new" => TokenType::KeyWord(KeyWord::New),
            "table" => TokenType::KeyWord(KeyWord::Table),
            "primary" => TokenType::KeyWord(KeyWord::Primary),
            "insert" => TokenType::KeyWord(KeyWord::Insert),
            "describe" => TokenType::KeyWord(KeyWord::Describe),
            "true" | "false" => TokenType::Literal(Literal::Bool),
            _ => TokenType::ID,
        };
        let token = Token {
            token_type,
            content: word.to_string(),
        };

        return Ok(token);
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.content)
    }
}

impl fmt::Display for LexErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use LexErr::*;
        match self {
            UntermString(found) => write!(f, "Expect '\'' to end string, found {}", found),
            Expect(expected, found) => write!(f, "Expect '{}', found '{}'", expected, found),
            InvalidNumeric(found) => {
                write!(f, "Invalid character '{}' inside numeric token", found)
            }
            InvalidChar(ch) => write!(f, "Invalid character '{}' in query", ch),
            InvalidWord(found) => write!(f, "Invalid character '{}' in a word", found),
        }
    }
}

mod tests {
    use super::*;

    fn expect(token: Token, expect_type: TokenType, content: &'static str) {
        assert_eq!(token.token_type, expect_type);
        assert_eq!(token.content, content);
    }

    #[test]
    fn tokenize_normal() {
        let mut lexer = Lexer::new("select * from foo;");
        let select = lexer.next().unwrap().unwrap();
        expect(select, TokenType::KeyWord(KeyWord::Select), "select");

        let star = lexer.next().unwrap().unwrap();
        expect(star, TokenType::Op(Op::Star), "*");

        let from = lexer.next().unwrap().unwrap();
        expect(from, TokenType::KeyWord(KeyWord::From), "from");

        let foo = lexer.next().unwrap().unwrap();
        expect(foo, TokenType::ID, "foo");

        let semi = lexer.next().unwrap().unwrap();
        expect(semi, TokenType::Punc(Punc::Semi), ";");

        assert!(lexer.next().unwrap().is_none());
    }

    #[test]
    fn tokenize_numeric() {
        let mut lexer = Lexer::new("insert foo (1+2, -3.4, true);");

        let insert = lexer.next().unwrap().unwrap();
        expect(insert, TokenType::KeyWord(KeyWord::Insert), "insert");

        let foo = lexer.next().unwrap().unwrap();
        expect(foo, TokenType::ID, "foo");

        let lparen = lexer.next().unwrap().unwrap();
        expect(lparen, TokenType::Punc(Punc::LParen), "(");

        let one = lexer.next().unwrap().unwrap();
        expect(one, TokenType::Literal(Literal::Int), "1");

        let plus = lexer.next().unwrap().unwrap();
        expect(plus, TokenType::Op(Op::Plus), "+");

        let two = lexer.next().unwrap().unwrap();
        expect(two, TokenType::Literal(Literal::Int), "2");

        let first_comma = lexer.next().unwrap().unwrap();
        expect(first_comma, TokenType::Punc(Punc::Comma), ",");

        let minus = lexer.next().unwrap().unwrap();
        expect(minus, TokenType::Op(Op::Minus), "-");

        let float = lexer.next().unwrap().unwrap();
        expect(float, TokenType::Literal(Literal::Float), "3.4");

        let second_comma = lexer.next().unwrap().unwrap();
        expect(second_comma, TokenType::Punc(Punc::Comma), ",");

        let bool_true = lexer.next().unwrap().unwrap();
        expect(bool_true, TokenType::Literal(Literal::Bool), "true");

        let rparen = lexer.next().unwrap().unwrap();
        expect(rparen, TokenType::Punc(Punc::RParen), ")");

        let semi = lexer.next().unwrap().unwrap();
        expect(semi, TokenType::Punc(Punc::Semi), ";");
    }
}

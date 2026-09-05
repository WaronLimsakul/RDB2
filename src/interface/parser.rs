use std::fmt;

use crate::{
    interface::lexer::{KeyWord, LexErr, Lexer, Literal, Op, Punc, Token, TokenType},
    storage::{self, EngineErr, table::TableSchema},
};

#[derive(Debug)]
pub enum ParseErr {
    Expect(&'static str, String), // Expect "String 1" found "String 2"
    LexErr(LexErr),
    InvalidRowValue(String), // Invalid string inside provided row value
    InvalidExpr(String),     // Found this tring in the expression
    PKeyDefined(bool),       // true = defined more than once (not supported), false = not provided
    InvalidPKeyType(storage::Type),
}

pub enum Stmt {
    // Query type: root
    Select {
        table: TableNode,
        columns: ColumnList,
    }, // table = Table, columns = Vec<Column>
    Insert {
        table: TableNode,
        values: Vec<RowValueNode>,
    },
    New {
        table: TableNode,
        schema: TableSchema, // Will just use engine's schema right away
    },
}

#[derive(Debug, PartialEq)]
pub struct TableNode {
    pub name: String,
}

#[derive(Debug, PartialEq)]
pub enum ColumnList {
    All, // '*'
    Listed(Vec<ColumnNode>),
}

#[derive(Debug, PartialEq)]
pub struct ColumnNode {
    pub name: String,
}

// Row value provided in "insert" statement
#[derive(Debug, PartialEq)]
pub struct RowValueNode {
    pub values: Vec<ExprNode>,
}

// Reprent any expression for row value (for now)
#[derive(Debug, PartialEq)]
pub enum ExprNode {
    Literal(LiteralExpr),
    Op(OpExpr),
}

#[derive(Debug, PartialEq)]
pub enum LiteralExpr {
    String(String),
    Bool(bool),
    Int(i64),
    Float(f64),
}

#[derive(Debug, PartialEq)]
pub enum OpExpr {
    Plus(Box<ExprNode>, Box<ExprNode>),
    Minus(Box<ExprNode>, Box<ExprNode>),
    Mult(Box<ExprNode>, Box<ExprNode>),
    Div(Box<ExprNode>, Box<ExprNode>),
}

pub struct ParseTree {
    pub root: Stmt,
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Parser {
            lexer: Lexer::new(input),
        }
    }

    /// Parse raw user input into AST
    pub fn parse(&mut self) -> Result<ParseTree, ParseErr> {
        let first_token = self.peek_token()?;

        let root = match first_token.token_type {
            TokenType::KeyWord(KeyWord::Select) => self.parse_select_stmt()?,
            TokenType::KeyWord(KeyWord::Insert) => self.parse_insert_stmt()?,
            TokenType::KeyWord(KeyWord::New) => self.parse_new_stmt()?,
            _ => {
                return Err(ParseErr::Expect(
                    "First token of type: select/insert/new",
                    first_token.to_string(),
                ));
            }
        };

        Ok(ParseTree { root })
    }

    // Grammar: select <col1>, <col2>, ... from table
    // can do <col_name> or *
    fn parse_select_stmt(&mut self) -> Result<Stmt, ParseErr> {
        debug_assert_eq!(
            self.peek_token()?.token_type,
            TokenType::KeyWord(KeyWord::Select)
        );

        self.next_token()?; // Pop the "select" token out

        // Parse column list
        let column_list = self.parse_column_list()?;

        // Next token = "from"
        let from_token = self.next_token()?;
        if from_token.token_type != TokenType::KeyWord(KeyWord::From) {
            return Err(ParseErr::Expect("'from' keyword", from_token.to_string()));
        }

        // Parse table name
        let table = self.parse_table()?;

        // Last token must be ';'
        let last_token = self.next_token()?;
        if last_token.token_type != TokenType::Punc(Punc::Semi) {
            return Err(ParseErr::Expect(";", last_token.content));
        }

        let select = Stmt::Select {
            table,
            columns: column_list,
        };
        Ok(select)
    }

    // Grammar:
    // `insert <table> [(c1, c2, ...), (c1, c2, ...), ...]` or
    // `insert <table> (c1, c2, ...)`
    fn parse_insert_stmt(&mut self) -> Result<Stmt, ParseErr> {
        debug_assert_eq!(
            self.peek_token()?.token_type,
            TokenType::KeyWord(KeyWord::Insert)
        );

        self.next_token()?; // Pop the "insert" keyword

        // Second child: table name
        let table = self.parse_table()?;

        // Third child: value or value list
        // If start with
        // - '['      : value list
        // - '('      : value
        // - otherwise: invalid
        let start_val = self.peek_token()?;
        let val_node = match start_val.content.as_str() {
            "[" => self.parse_row_value_list()?,
            "(" => vec![self.parse_row_value()?],
            _ => {
                return Err(ParseErr::InvalidRowValue(self.next_token()?.content));
            }
        };

        // Last token must be ';'
        let last_token = self.next_token()?;
        if last_token.token_type != TokenType::Punc(Punc::Semi) {
            return Err(ParseErr::Expect(";", last_token.content));
        }

        let insert = Stmt::Insert {
            table,
            values: val_node,
        };
        Ok(insert)
    }

    // Grammar:
    // `new table <table> { <c1> : <t1> primary, <c2> : <t2>, ... }`
    fn parse_new_stmt(&mut self) -> Result<Stmt, ParseErr> {
        debug_assert_eq!(
            self.peek_token()?.token_type,
            TokenType::KeyWord(KeyWord::New)
        );
        self.next_token()?; // Pop "new"

        let table_keyword = self.next_token()?; // Pop "table"
        if table_keyword.token_type != TokenType::KeyWord(KeyWord::Table) {
            return Err(ParseErr::Expect("'table'", table_keyword.content));
        }

        let table = self.parse_table()?; // table name
        let schema = self.parse_table_schema()?; // schema

        // Last token must be ';'
        let last_token = self.next_token()?;
        if last_token.token_type != TokenType::Punc(Punc::Semi) {
            return Err(ParseErr::Expect(";", last_token.content));
        }

        let stmt = Stmt::New { table, schema };
        Ok(stmt)
    }

    // Grammar:
    // `*` or `<col1>, <col2>, ...`
    fn parse_column_list(&mut self) -> Result<ColumnList, ParseErr> {
        let first_col = self.peek_token()?;

        // If it's `*` then we just pop the token and done
        if first_col.content == "*" {
            self.next_token()?;
            return Ok(ColumnList::All);
        }

        let mut column_list: Vec<ColumnNode> = Vec::new();
        // Parse a column then ',' and keep repeating until not found ','
        loop {
            column_list.push(self.parse_column()?);
            if self.peek_token()?.token_type != TokenType::Punc(Punc::Comma) {
                break;
            }
            self.next_token()?; // pop the comma
        }

        Ok(ColumnList::Listed(column_list))
    }

    // Grammar: just `<col_name>`
    // NOTE: might support alias later
    fn parse_column(&mut self) -> Result<ColumnNode, ParseErr> {
        // The column name should only be id right now
        // TODO: support table.column when support join

        let col_name = self.next_token()?;
        if col_name.token_type != TokenType::ID {
            return Err(ParseErr::Expect("Column name", col_name.content));
        }

        let col_node = ColumnNode {
            name: col_name.content,
        };
        Ok(col_node)
    }

    // Grammar: just `<table_name>`
    // NOTE: might support alias later
    fn parse_table(&mut self) -> Result<TableNode, ParseErr> {
        let table_token = self.next_token()?;
        if table_token.token_type != TokenType::ID {
            return Err(ParseErr::Expect("Table name", table_token.content));
        }

        let node = TableNode {
            name: table_token.content,
        };
        Ok(node)
    }

    // Grammar: [<row_value_1>, <row_value_2>, ...]
    fn parse_row_value_list(&mut self) -> Result<Vec<RowValueNode>, ParseErr> {
        debug_assert_eq!(self.peek_token()?.token_type, TokenType::Punc(Punc::LBrack));

        self.next_token()?; // [
        let mut value_list: Vec<RowValueNode> = Vec::new();

        // Each child is row value node
        loop {
            let row_value = self.parse_row_value()?;
            value_list.push(row_value);

            if self.peek_token()?.token_type != TokenType::Punc(Punc::Comma) {
                break;
            }
            self.next_token()?; // pop the comma
        }

        // Should end with ]
        let rbrak = self.next_token()?;
        if rbrak.token_type != TokenType::Punc(Punc::RBrack) {
            return Err(ParseErr::Expect("]", rbrak.content));
        }

        Ok(value_list)
    }

    // Grammar: (v1, v2, ...)
    fn parse_row_value(&mut self) -> Result<RowValueNode, ParseErr> {
        let lparen = self.next_token()?;
        if lparen.token_type != TokenType::Punc(Punc::LParen) {
            return Err(ParseErr::Expect("(", lparen.content));
        }

        let mut vals: Vec<ExprNode> = Vec::new();
        loop {
            let value = self.parse_expr()?;
            vals.push(value);

            if self.peek_token()?.token_type != TokenType::Punc(Punc::Comma) {
                break;
            }
            self.next_token()?; // Pop the comma
        }

        let rparen = self.next_token()?;
        if rparen.token_type != TokenType::Punc(Punc::RParen) {
            return Err(ParseErr::Expect(")", lparen.content));
        }

        Ok(RowValueNode { values: vals })
    }

    // Grammar: just literal value for now
    // TODO: support operation on literal value
    fn parse_expr(&mut self) -> Result<ExprNode, ParseErr> {
        // A literal can be
        // - string
        // - boolean
        // - numeric
        //   - a single numeric literal
        //   - a minus + numeric literal

        let first_token = self.next_token()?;
        let mut minus = false; // In case first token is just minus sign
        let mut node = match first_token.token_type {
            TokenType::Literal(Literal::Bool) => {
                ExprNode::Literal(LiteralExpr::Bool(first_token.content == "true"))
            }
            TokenType::Literal(Literal::String) => {
                // Should start and end with '\'' from lexer
                debug_assert_eq!(first_token.content.chars().nth(0).unwrap(), '\'');
                debug_assert_eq!(first_token.content.chars().nth_back(0).unwrap(), '\'');
                ExprNode::Literal(LiteralExpr::String(
                    // Strip leading + trailing '\''
                    first_token.content[1..first_token.content.len() - 1].to_string(),
                ))
            }
            TokenType::Literal(Literal::Int) => {
                ExprNode::Literal(LiteralExpr::Int(first_token.content.parse().unwrap()))
            }
            TokenType::Literal(Literal::Float) => {
                ExprNode::Literal(LiteralExpr::Float(first_token.content.parse().unwrap()))
            }
            TokenType::Op(Op::Minus) => {
                minus = true;
                ExprNode::Literal(LiteralExpr::Int(-1))
            }
            _ => {
                return Err(ParseErr::InvalidExpr(first_token.content));
            }
        };

        // If first token is minus sign, the second one must be numeric
        if minus {
            let second_token = self.next_token()?;
            node = match second_token.token_type {
                TokenType::Literal(Literal::Int) => ExprNode::Literal(LiteralExpr::Int(
                    -1 * second_token.content.parse::<i64>().unwrap(),
                )),
                TokenType::Literal(Literal::Float) => ExprNode::Literal(LiteralExpr::Float(
                    -1.0 * second_token.content.parse::<f64>().unwrap(),
                )),
                _ => {
                    return Err(ParseErr::Expect("Numeric expression", second_token.content));
                }
            };
        }

        Ok(node)
    }

    // Grammar: { <c1> : <t1> primary, <c2> : <t2>, ... }`
    // - Allowed types: see parse_column_type()
    // - One of the column must be `primary` key and use `uint` or `ulong`
    fn parse_table_schema(&mut self) -> Result<TableSchema, ParseErr> {
        // Must start with '{'
        let lbrace = self.next_token()?;
        if lbrace.token_type != TokenType::Punc(Punc::LBrace) {
            return Err(ParseErr::Expect("{", lbrace.content));
        }

        let mut schema = TableSchema::new();

        let mut primary_set = false;
        // Until not found ','
        loop {
            let col = self.parse_column()?;

            let colon = self.next_token()?;
            if colon.token_type != TokenType::Punc(Punc::Colon) {
                return Err(ParseErr::Expect(":", colon.content));
            }

            let col_type = self.parse_column_type()?;
            // In case primary key
            if self.peek_token()?.token_type == TokenType::KeyWord(KeyWord::Primary) {
                // Can only have 1 primary key for now
                if primary_set {
                    return Err(ParseErr::PKeyDefined(true));
                }

                let key_type = col_type
                    .try_into()
                    .map_err(|_| ParseErr::InvalidPKeyType(col_type))?;
                schema.set_key(col.name, key_type);

                self.next_token()?; // Pop "primary"
                primary_set = true;
            } else {
                // Non-primary key type
                schema.add_val_type(col.name, col_type);
            }

            if self.peek_token()?.token_type != TokenType::Punc(Punc::Comma) {
                break;
            }

            self.next_token()?; // Pop ','
        }

        if !primary_set {
            return Err(ParseErr::PKeyDefined(false));
        }

        // Must end with '}'
        let rbrace = self.next_token()?;
        if rbrace.token_type != TokenType::Punc(Punc::RBrace) {
            return Err(ParseErr::Expect("}", rbrace.content));
        }

        Ok(schema)
    }

    // Allowed types: `int`, `uint`, `long`, `ulong`, `bool`, `string`, `float`
    fn parse_column_type(&mut self) -> Result<storage::Type, ParseErr> {
        let type_token = self.next_token()?;
        // Just consider data type to by ID for now
        if type_token.token_type != TokenType::ID {
            return Err(ParseErr::Expect("Data type", type_token.content));
        }

        use storage::Type;
        let col_type = match type_token.content.as_str() {
            "int" => Type::Int,
            "uint" => Type::Uint,
            "long" => Type::Long,
            "ulong" => Type::Ulong,
            "string" => Type::String,
            "bool" => Type::Bool,
            "float" => Type::Float,
            _ => {
                return Err(ParseErr::Expect("Data type", type_token.content));
            }
        };

        Ok(col_type)
    }

    // Helper for peeking next token from lexer. Return error if none found
    fn peek_token(&mut self) -> Result<&Token, ParseErr> {
        self.lexer
            .peek()
            .map_err(|e| ParseErr::LexErr(e))?
            .ok_or_else(|| ParseErr::Expect("something", "nothing".to_string()))
    }

    // Helper for getting next token from lexer. Return error if none found
    fn next_token(&mut self) -> Result<Token, ParseErr> {
        self.lexer
            .next()
            .map_err(|e| ParseErr::LexErr(e))?
            .ok_or_else(|| ParseErr::Expect("something", "nothing".to_string()))
    }
}

impl fmt::Display for ParseErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ParseErr::*;
        match self {
            Expect(expected, found) => write!(f, "Expect {}, Found {}", expected, found),
            LexErr(le) => write!(f, "Lexer error: {}", le),
            InvalidRowValue(found) => {
                write!(f, "Invalid string '{}' inside provided row value", found)
            }
            InvalidExpr(found) => write!(f, "Invalid string '{}' inside an expression", found),
            PKeyDefined(defined) => {
                if !defined {
                    write!(f, "Primary key not provided")
                } else {
                    write!(f, "Primary key defined more than once.")
                }
            }
            InvalidPKeyType(t) => write!(
                f,
                "Invalid primary key type {}. Only uint and ulong allowed.",
                t
            ),
        }
    }
}

impl fmt::Display for LiteralExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use LiteralExpr::*;
        match self {
            Int(v) => write!(f, "{v}"),
            Float(v) => write!(f, "{v}"),
            String(v) => write!(f, "{v}"),
            Bool(v) => write!(f, "{v}"),
        }
    }
}

mod tests {
    use super::*;

    #[test]
    fn parse_select() {
        let mut parser = Parser::new("select c1, c2, c3 from foo;");
        let stmt = parser.parse().unwrap().root;

        assert!(matches!(
            stmt,
            Stmt::Select {
                table: _,
                columns: _,
            }
        ));

        let expected_table = TableNode {
            name: "foo".to_string(),
        };
        let expected_columns = ColumnList::Listed(vec![
            ColumnNode {
                name: "c1".to_string(),
            },
            ColumnNode {
                name: "c2".to_string(),
            },
            ColumnNode {
                name: "c3".to_string(),
            },
        ]);
        if let Stmt::Select { table, columns } = stmt {
            assert_eq!(table, expected_table);
            assert_eq!(columns, expected_columns);
        }
    }

    #[test]
    fn parse_insert() {
        let mut parser = Parser::new(
            "insert foo [
                (10, \'hello\', true), 
                (93, \'world\', false), 
                (-39, \'bar\', true)
            ];",
        );
        let stmt = parser.parse().unwrap().root;

        assert!(matches!(
            stmt,
            Stmt::Insert {
                table: _,
                values: _
            }
        ));

        let expected_table = TableNode {
            name: "foo".to_string(),
        };
        let expected_values = vec![
            RowValueNode {
                values: vec![
                    ExprNode::Literal(LiteralExpr::Int(10)),
                    ExprNode::Literal(LiteralExpr::String("hello".to_string())),
                    ExprNode::Literal(LiteralExpr::Bool(true)),
                ],
            },
            RowValueNode {
                values: vec![
                    ExprNode::Literal(LiteralExpr::Int(93)),
                    ExprNode::Literal(LiteralExpr::String("world".to_string())),
                    ExprNode::Literal(LiteralExpr::Bool(false)),
                ],
            },
            RowValueNode {
                values: vec![
                    ExprNode::Literal(LiteralExpr::Int(-39)),
                    ExprNode::Literal(LiteralExpr::String("bar".to_string())),
                    ExprNode::Literal(LiteralExpr::Bool(true)),
                ],
            },
        ];

        if let Stmt::Insert { table, values } = stmt {
            assert_eq!(table, expected_table);
            assert_eq!(values, expected_values);
        }
    }

    #[test]
    fn parse_new() {
        let mut parser = Parser::new(
            "new table foo { 
                id: ulong primary, 
                name: string, 
                age: uint,
                is_cool: bool
            };",
        );
        let stmt = parser.parse().unwrap().root;

        assert!(matches!(
            stmt,
            Stmt::New {
                table: _,
                schema: _
            }
        ));

        let expected_table = TableNode {
            name: "foo".to_string(),
        };
        let expected_schema = TableSchema {
            key: ("id".to_string(), storage::KeyType::Ulong),
            vals: vec![
                ("name".to_string(), storage::Type::String),
                ("age".to_string(), storage::Type::Uint),
                ("is_cool".to_string(), storage::Type::Bool),
            ],
        };

        if let Stmt::New { table, schema } = stmt {
            assert_eq!(table, expected_table);
            assert_eq!(schema, expected_schema);
        }
    }
}

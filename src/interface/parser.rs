use std::{fmt, ops};

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
    InvalidMeta(String), // Meta command `String` not found
}

pub enum Stmt {
    // Query type: root
    Select {
        tables: Vec<TableNode>,
        columns: ColumnList,
        conds: WhereNode,
    }, // table = Table, columns = Vec<Column>
    Insert {
        table: TableNode,
        values: Vec<RowValueNode>,
    },
    New {
        table: TableNode,
        schema: TableSchema, // Will just use engine's schema right away
    },
    Delete {
        table: TableNode,
    },
    Describe {
        table: TableNode,
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

// Represent all the `where` clause
#[derive(Debug, PartialEq)]
pub struct WhereNode {
    pub preds: Vec<ExprNode>, // requires: ExprNode must be .is_pred()
}

// Row value provided in "insert" statement
#[derive(Debug, PartialEq)]
pub struct RowValueNode {
    pub values: Vec<ExprNode>,
}

// Reprent any value expression
#[derive(Debug, PartialEq)]
pub enum ExprNode {
    Literal(LiteralExpr),
    Column(ColumnNode),
    Op(OpExpr),
}

#[derive(Debug, PartialEq, Clone)]
pub enum LiteralExpr {
    String(String),
    Bool(bool),
    Int(i64),
    Float(f64),
}

// Constant expression with operation.
// Change this -> change
// - parser::parse_expr
// - lexer::Op
// - query::expr_to_col_data
#[derive(Debug, PartialEq)]
pub enum OpExpr {
    // Only numerical for now
    Plus(Box<ExprNode>, Box<ExprNode>),
    Minus(Box<ExprNode>, Box<ExprNode>),
    Mult(Box<ExprNode>, Box<ExprNode>),
    Div(Box<ExprNode>, Box<ExprNode>),

    // Only Predicate for now
    Eq(Box<ExprNode>, Box<ExprNode>),
    Neq(Box<ExprNode>, Box<ExprNode>),
    GT(Box<ExprNode>, Box<ExprNode>),
    GTE(Box<ExprNode>, Box<ExprNode>),
    LT(Box<ExprNode>, Box<ExprNode>),
    LTE(Box<ExprNode>, Box<ExprNode>),
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
            TokenType::KeyWord(KeyWord::Delete) => self.parse_delete_stmt()?,
            TokenType::KeyWord(KeyWord::Describe) => self.parse_describe_stmt()?,
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

        // Parse tables
        let tables = self.parse_table_list()?;

        // Parse where clause if exists
        let where_node = if self.peek_token()?.token_type == TokenType::KeyWord(KeyWord::Where) {
            self.parse_where_clause()?
        } else {
            WhereNode { preds: vec![] }
        };

        // Last token must be ';'
        let last_token = self.next_token()?;
        if last_token.token_type != TokenType::Punc(Punc::Semi) {
            return Err(ParseErr::Expect(";", last_token.content));
        }

        let select = Stmt::Select {
            tables,
            columns: column_list,
            conds: where_node,
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

    // Grammar: `delete table <table>;`
    fn parse_delete_stmt(&mut self) -> Result<Stmt, ParseErr> {
        debug_assert_eq!(
            self.peek_token()?.token_type,
            TokenType::KeyWord(KeyWord::Delete)
        );

        self.next_token()?; // Pop 'delete'
        let table_keyword = self.next_token()?;
        if table_keyword.token_type != TokenType::KeyWord(KeyWord::Table) {
            return Err(ParseErr::Expect("table", table_keyword.content));
        }

        let table = self.parse_table()?;

        // Last token must be ';'
        let last_token = self.next_token()?;
        if last_token.token_type != TokenType::Punc(Punc::Semi) {
            return Err(ParseErr::Expect(";", last_token.content));
        }

        let stmt = Stmt::Delete { table };
        Ok(stmt)
    }

    // Grammar: `describe <table>`
    fn parse_describe_stmt(&mut self) -> Result<Stmt, ParseErr> {
        debug_assert_eq!(
            self.peek_token()?.token_type,
            TokenType::KeyWord(KeyWord::Describe)
        );

        self.next_token()?; // Pop 'describe'
        let table = self.parse_table()?;

        // Last token must be ';'
        let last_token = self.next_token()?;
        if last_token.token_type != TokenType::Punc(Punc::Semi) {
            return Err(ParseErr::Expect(";", last_token.content));
        }

        let stmt = Stmt::Describe { table };
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

    // Grammar: `<table>, <table>, ...`
    // (at least 1 table tho)
    fn parse_table_list(&mut self) -> Result<Vec<TableNode>, ParseErr> {
        let mut tables: Vec<TableNode> = Vec::new();
        loop {
            tables.push(self.parse_table()?);
            if self.peek_token()?.token_type != TokenType::Punc(Punc::Comma) {
                break;
            }
            self.next_token()?; // Pop the ','
        }
        Ok(tables)
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

    // Grammar: value expression. Can be:
    // Terminal:
    // - '(' Expr ')'
    // - Column expression
    // - Literal expression
    // Non Terminal:
    // - <Expr> Operator <Expr>
    fn parse_expr(&mut self) -> Result<ExprNode, ParseErr> {
        let first_token = self.peek_token()?;
        let first = match first_token.token_type {
            // '(' Expr ')'
            TokenType::Punc(Punc::LParen) => {
                self.next_token()?; // Pop '('
                let res = self.parse_expr()?;
                let rparen = self.next_token()?;
                if rparen.token_type != TokenType::Punc(Punc::RParen) {
                    return Err(ParseErr::Expect(")", rparen.content));
                }
                res
            }
            // Column expression
            TokenType::ID => ExprNode::Column(self.parse_column()?),
            // Literal expression
            TokenType::Literal(_) | TokenType::Op(Op::Minus) => self.parse_literal_expr()?,
            _ => return Err(ParseErr::Expect("Expression", self.next_token()?.content)),
        };
        let next_token = self.peek_token()?;

        // && and || operator are not recursive for now
        if matches!(
            next_token.token_type,
            TokenType::Op(Op::And) | TokenType::Op(Op::Or)
        ) {
            return Ok(first);
        }

        match next_token.token_type {
            // If next token is operator, we keep chaining it as ExprNode::Op.
            // Therefore, to ensure correctness, user should use parentheses.
            TokenType::Op(_) => {
                let op = self.next_token()?;
                let second = self.parse_expr()?;
                let op_expr = match op.token_type {
                    TokenType::Op(Op::Plus) => OpExpr::Plus(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::Minus) => OpExpr::Minus(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::Star) => OpExpr::Mult(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::Div) => OpExpr::Div(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::Eq) => OpExpr::Eq(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::Neq) => OpExpr::Neq(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::GT) => OpExpr::GT(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::GTE) => OpExpr::GTE(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::LT) => OpExpr::LT(Box::new(first), Box::new(second)),
                    TokenType::Op(Op::LTE) => OpExpr::LTE(Box::new(first), Box::new(second)),
                    _ => {
                        return Err(ParseErr::Expect("Operator", op.content));
                    }
                };
                Ok(ExprNode::Op(op_expr))
            }
            // Otherwise, it's terminal
            _ => Ok(first),
        }
    }

    /// Parse ExprNode::LiteralExpr
    // A literal can be
    // - string
    // - boolean
    // - numeric
    //   - a single numeric literal
    //   - a minus + numeric literal
    fn parse_literal_expr(&mut self) -> Result<ExprNode, ParseErr> {
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
        let col_type = match type_token.content.to_lowercase().as_str() {
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

    // Grammar: where <pred1> && <pred2> && ...
    // - pred = see parse_predicate
    // TODO: support `or` later
    fn parse_where_clause(&mut self) -> Result<WhereNode, ParseErr> {
        debug_assert_eq!(
            self.peek_token()?.token_type,
            TokenType::KeyWord(KeyWord::Where)
        );

        self.next_token()?; // Pop 'where'

        let mut preds: Vec<ExprNode> = Vec::new();
        loop {
            preds.push(self.parse_predicate()?);
            if self.peek_token()?.token_type != TokenType::Op(Op::And) {
                break;
            }
            self.next_token()?; // Pop '&&'
        }

        Ok(WhereNode { preds })
    }

    // Grammar: <expr> that is <pred_op>
    // - pred_op = OpExpr that .is_pred
    // TODO: deal with nested condition
    fn parse_predicate(&mut self) -> Result<ExprNode, ParseErr> {
        let pred = self.parse_expr()?;

        // check if operator is predicate-able
        if let ExprNode::Op(op) = &pred {
            if !op.is_pred() {
                return Err(ParseErr::Expect("Predicate operator", format!("{op}")));
            }
        } else {
            return Err(ParseErr::Expect(
                "Operator Expression in predicate",
                "Something else".to_string(),
            ));
        }

        Ok(pred)
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
            InvalidMeta(c) => write!(f, "Invalid meta command '{}'.", c),
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

//
// Numerical operator and comparison for literal expr:
// sometimes, we don't know the target type to convert
// to ColData, so we have to do calculation on raw expr.
//

impl ops::Add for LiteralExpr {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        use LiteralExpr::*;
        match (self, rhs) {
            (Int(lhs), Int(rhs)) => Int(lhs + rhs),
            (Float(lhs), Float(rhs)) => Float(lhs + rhs),
            (Int(lhs), Float(rhs)) => Float(lhs as f64 + rhs),
            (Float(lhs), Int(rhs)) => Float(lhs + rhs as f64),
            _ => panic!("Add non-numeric or not same type"),
        }
    }
}

impl ops::Sub for LiteralExpr {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        use LiteralExpr::*;
        match (self, rhs) {
            (Int(lhs), Int(rhs)) => Int(lhs - rhs),
            (Float(lhs), Float(rhs)) => Float(lhs - rhs),
            (Int(lhs), Float(rhs)) => Float(lhs as f64 - rhs),
            (Float(lhs), Int(rhs)) => Float(lhs - rhs as f64),
            _ => panic!("Subtract non-numeric or not same type"),
        }
    }
}

impl ops::Mul for LiteralExpr {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        use LiteralExpr::*;
        match (self, rhs) {
            (Int(lhs), Int(rhs)) => Int(lhs * rhs),
            (Float(lhs), Float(rhs)) => Float(lhs * rhs),
            (Int(lhs), Float(rhs)) => Float(lhs as f64 * rhs),
            (Float(lhs), Int(rhs)) => Float(lhs * rhs as f64),
            _ => panic!("Multiply non-numeric or not same type"),
        }
    }
}

impl ops::Div for LiteralExpr {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        use LiteralExpr::*;
        match (self, rhs) {
            (Int(lhs), Int(rhs)) => Int(lhs / rhs),
            (Float(lhs), Float(rhs)) => Float(lhs / rhs),
            (Int(lhs), Float(rhs)) => Float(lhs as f64 / rhs),
            (Float(lhs), Int(rhs)) => Float(lhs / rhs as f64),
            _ => panic!("Divide non-numeric or not same type"),
        }
    }
}

impl PartialOrd for LiteralExpr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use LiteralExpr::*;
        match (self, other) {
            (Int(lhs), Int(rhs)) => lhs.partial_cmp(rhs),
            (Float(lhs), Float(rhs)) => lhs.partial_cmp(rhs),
            (Bool(lhs), Bool(rhs)) => lhs.partial_cmp(rhs),
            _ => None,
        }
    }
}

impl OpExpr {
    /// Whether this operator expression represent predicate
    // TODO: might be pred or something else in the future
    // change this -> change is_predable, parse_predicate
    pub fn is_pred(&self) -> bool {
        use OpExpr::*;
        match self {
            Eq(_, _) | Neq(_, _) | GT(_, _) | GTE(_, _) | LT(_, _) | LTE(_, _) => true,
            _ => false,
        }
    }
}

impl fmt::Display for OpExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use OpExpr::*;
        match self {
            Plus(_, _) => write!(f, "+"),
            Minus(_, _) => write!(f, "-"),
            Mult(_, _) => write!(f, "*"),
            Div(_, _) => write!(f, "/"),
            Eq(_, _) => write!(f, "="),
            Neq(_, _) => write!(f, "!="),
            GT(_, _) => write!(f, ">"),
            GTE(_, _) => write!(f, ">="),
            LT(_, _) => write!(f, "<"),
            LTE(_, _) => write!(f, "<="),
        }
    }
}

// Static helper to check if this token is operator that can
// be use in a predicate (not include the logical operator).
// change this -> change parse_predicate, OpExpr::is_pred
fn is_predable(token: &Token) -> bool {
    match &token.token_type {
        TokenType::Op(op) => match op {
            Op::Eq | Op::Neq | Op::GT | Op::GTE | Op::LT | Op::LTE => true,
            _ => false,
        },
        _ => false,
    }
}

mod tests {
    use super::*;

    #[test]
    fn parse_basic_select() {
        let mut parser = Parser::new("select c1, c2, c3 from foo;");
        let stmt = parser.parse().unwrap().root;

        assert!(matches!(
            stmt,
            Stmt::Select {
                table: _,
                columns: _,
                conds: _,
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
        if let Stmt::Select {
            table,
            columns,
            conds: _,
        } = stmt
        {
            assert_eq!(table, expected_table);
            assert_eq!(columns, expected_columns);
        }
    }

    #[test]
    fn parse_select_where() {
        let mut parser = Parser::new("select * from Foo where c1 = 2;");
        let stmt = parser.parse().unwrap().root;
        assert!(matches!(
            stmt,
            Stmt::Select {
                table: _,
                columns: _,
                conds: _
            }
        ));

        let expected_conds = WhereNode {
            preds: vec![ExprNode::Op(OpExpr::Eq(
                Box::new(expr_node_col("c1")),
                Box::new(lit_node_int(2)),
            ))],
        };

        if let Stmt::Select {
            table: _,
            columns: _,
            conds,
        } = stmt
        {
            assert_eq!(conds, expected_conds);
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

    #[test]
    fn parse_op_expr_numeric() {
        let mut parser = Parser::new("insert Foo (1+(2-3), 5*6);");
        let stmt = parser.parse().unwrap().root;

        assert!(matches!(
            stmt,
            Stmt::Insert {
                table: _,
                values: _
            }
        ));

        let expected_values = vec![RowValueNode {
            values: vec![
                // 1 + (2 - 3)
                ExprNode::Op(OpExpr::Plus(
                    Box::new(lit_node_int(1)),
                    Box::new(ExprNode::Op(OpExpr::Minus(
                        Box::new(lit_node_int(2)),
                        Box::new(lit_node_int(3)),
                    ))),
                )),
                // 5 * 6
                ExprNode::Op(OpExpr::Mult(
                    Box::new(lit_node_int(5)),
                    Box::new(lit_node_int(6)),
                )),
            ],
        }];

        if let Stmt::Insert { table: _, values } = stmt {
            assert_eq!(expected_values, values);
        }
    }

    // TODO: Support boolean-type const operator expression

    fn lit_node_int(v: i64) -> ExprNode {
        ExprNode::Literal(LiteralExpr::Int(v))
    }
    fn lit_node_str(s: &'static str) -> ExprNode {
        ExprNode::Literal(LiteralExpr::String(s.to_string()))
    }
    fn expr_node_col(c: &'static str) -> ExprNode {
        ExprNode::Column(ColumnNode {
            name: c.to_string(),
        })
    }
}

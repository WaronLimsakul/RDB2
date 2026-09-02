use crate::interface::parser::ParseTree;

mod lexer;
mod parser;
pub mod repl;

pub enum Cmd {
    Meta { cmd: MetaCmd, raw: String },
    DDL { ast: ParseTree, raw: String },
    DML { ast: ParseTree, raw: String },
    DQL { ast: ParseTree, raw: String },
}

pub enum MetaCmd {
    Quit,
}

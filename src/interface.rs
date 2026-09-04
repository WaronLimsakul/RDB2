use crate::interface::parser::ParseTree;

mod lexer;
pub mod parser;
pub mod printer;
pub mod repl;

pub enum Cmd {
    // TODO: consider remove raw if no one's gonna use it
    Meta { cmd: MetaCmd, raw: String },
    DDL { ast: ParseTree, raw: String },
    DML { ast: ParseTree, raw: String },
    DQL { ast: ParseTree, raw: String },
}

pub enum MetaCmd {
    Quit,
}

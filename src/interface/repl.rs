use std::io::Write;

use crate::interface::{
    Cmd, MetaCmd,
    parser::{ParseErr, Parser, Stmt},
};

/// The the parse input from the raw user input
/// NOTE: the parse tree doesn't know the catalog. Caller must
/// check the correctness of the mentioned table/col name/type
pub fn get_input() -> Result<Cmd, ParseErr> {
    let raw = get_raw_input();

    // Metacommand case
    if raw.chars().nth(0).unwrap() == '.' {
        let meta_cmd = match &raw[1..] {
            "quit" => MetaCmd::Quit,
            "exit" => MetaCmd::Quit,
            _ => {
                panic!("Meta command {} not found", String::from(raw));
            } // TODO: handle it better
        };

        return Ok(Cmd::Meta { cmd: meta_cmd, raw });
    }

    // Query case
    let mut parser = Parser::new(raw.as_str());
    let ast = parser.parse()?;
    let cmd = match &ast.root {
        Stmt::Select {
            table: _,
            columns: _,
        } => Cmd::DQL { ast, raw },
        Stmt::Insert {
            table: _,
            values: _,
        } => Cmd::DML { ast, raw },
        Stmt::New {
            table: _,
            schema: _,
        } => Cmd::DDL { ast, raw },
    };

    Ok(cmd)
}

pub fn get_raw_input() -> String {
    print!("> ");
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    String::from(input.trim_end())
}

pub fn output(msg: &str) {
    println!("{msg}");
}

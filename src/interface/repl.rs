use std::io::Write;

use rustyline::{DefaultEditor, error::ReadlineError};

use crate::interface::{
    Cmd, MetaCmd,
    parser::{ParseErr, Parser, Stmt},
    printer::PrintableTable,
};

/// Get and parse input from the raw user input
/// NOTE: the parse tree doesn't know the catalog. Caller must
/// check the correctness of the mentioned table/col name/type
pub fn get_input(line_reader: &mut DefaultEditor) -> Result<Cmd, ParseErr> {
    let raw = get_raw_input(line_reader);

    // Metacommand case
    if raw.chars().next().unwrap() == '.' {
        let meta_cmd = match &raw[1..] {
            "quit" => MetaCmd::Quit,
            "exit" => MetaCmd::Quit,
            "tables" => MetaCmd::Tables,
            "ls" => MetaCmd::Tables,
            "help" => MetaCmd::Help,
            _ => return Err(ParseErr::InvalidMeta(raw)),
        };

        return Ok(Cmd::Meta { cmd: meta_cmd, raw });
    }

    // Query case
    let mut parser = Parser::new(raw.as_str());
    let ast = parser.parse()?;
    let cmd = match &ast.root {
        Stmt::Select {
            tables: _,
            columns: _,
            conds: _,
        } => Cmd::DQL { ast, raw },
        Stmt::Insert {
            table: _,
            values: _,
        } => Cmd::DML { ast, raw },
        Stmt::New {
            table: _,
            schema: _,
        } => Cmd::DDL { ast, raw },
        Stmt::Delete { table: _ } => Cmd::DDL { ast, raw },
        Stmt::Describe { table: _ } => Cmd::DDL { ast, raw },
    };

    Ok(cmd)
}

const PROMPT_SYMBOL: &str = "» ";

/// Prompt user input until it get something that is not empty
pub fn get_raw_input(line_reader: &mut DefaultEditor) -> String {
    match line_reader.readline(PROMPT_SYMBOL) {
        Ok(line) => {
            line_reader.add_history_entry(line.as_str()).unwrap();
            line
        }
        // CTRL-C and CTRL-D are counted as exit for now
        Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => ".exit".to_string(),
        Err(err) => {
            panic!("Readline error {}", err);
        }
    }
}

pub fn output(msg: &str) {
    println!("{msg}");
}

pub fn output_table(table: &PrintableTable) {
    print!("{table}");
    std::io::stdout().flush().unwrap();
}

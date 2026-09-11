use crate::interface::parser::ParseTree;

mod lexer;
pub mod parser;
pub mod printer;
pub mod repl;

/// All types of command user can do in RDB2
pub enum Cmd {
    // TODO: consider remove raw if no one's gonna use it
    Meta { cmd: MetaCmd, raw: String },
    DDL { ast: ParseTree, raw: String },
    DML { ast: ParseTree, raw: String },
    DQL { ast: ParseTree, raw: String },
}

pub enum MetaCmd {
    Quit,
    Tables, // List all tables in db
    Help,   // Print help message
}

const LOGO: &str = r#"
    ____              ____  ____ ___ 
   / __ \____  ____  / __ \/ __ )__ \
  / /_/ / __ \/ __ \/ / / / __  |_/ /
 / _, _/ /_/ / / / / /_/ / /_/ / __/ 
/_/ |_|\____/_/ /_/_____/_____/____/
"#;

/// Print a welcome message
pub fn welcome() {
    repl::output(LOGO);
    repl::output("For help, do `.help`")
}

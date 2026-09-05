use std::process;

use rustyline::DefaultEditor;

use crate::interface::repl;

mod execution;
mod interface;
mod storage;

// TODO: let user determine this
const DB_DIR: &str = ".rdb";

fn main() {
    interface::welcome();

    // Single line reader instance
    let mut line_reader = match DefaultEditor::new() {
        Ok(lr) => lr,
        Err(e) => {
            repl::output(&format!("Error create line reader: {}", e));
            process::exit(1);
        }
    };

    // Single storage engine instance
    let mut engine = storage::engine::StorageEngine::new(DB_DIR).unwrap();

    // REPL
    loop {
        let input = match repl::get_input(&mut line_reader) {
            Ok(cmd) => cmd,
            Err(err) => {
                repl::output(&format!("Invalid input: {}", err));
                continue;
            }
        };
        // If error occurs, just print
        if let Err(e) = execution::execute(input, &mut engine) {
            repl::output(&format!("Error: {}", e));
        }
    }
}

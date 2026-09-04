use std::process;

use crate::{execution::ExecErr, interface::repl};

mod execution;
mod interface;
mod storage;

fn main() {
    interface::welcome();
    // TODO: input user selected root dir
    let mut engine = storage::engine::StorageEngine::new(".rdb").unwrap();
    loop {
        let input = match repl::get_input() {
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

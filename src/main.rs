use std::process;

use crate::{execution::ExecErr, interface::repl};

mod execution;
mod interface;
mod storage;

fn main() {
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
        match execution::execute(input, &mut engine) {
            Err(e) => {
                repl::output(&format!("Error: {}", e));
            }
            Ok(_) => {} // TODO: print result when support query
        }
    }
}

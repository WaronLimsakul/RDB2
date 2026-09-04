//! # Meta Execution
//!
//! Main function for executiong meta command
//!

use std::process;

use crate::{
    interface::MetaCmd,
    storage::{EngineErr, engine::StorageEngine},
};

pub fn execute(cmd: MetaCmd, engine: &mut StorageEngine) -> Result<(), EngineErr> {
    match cmd {
        MetaCmd::Quit => {
            engine.flush_all()?;
            process::exit(0);
        }
    }
}

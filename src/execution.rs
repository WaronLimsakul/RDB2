use crate::{
    interface::Cmd,
    storage::{EngineErr, engine::StorageEngine},
};

pub mod meta;

pub enum ExecErr {
    Storage(EngineErr), // Something wrong happen in storage engine
}

// TODO: Ok can be cursor or something interface should shows
pub fn execute(cmd: Cmd, engine: &mut StorageEngine) -> Result<(), ExecErr> {
    match cmd {
        Cmd::Meta {
            cmd: meta_cmd,
            raw: _,
        } => {
            meta::execute(meta_cmd, engine).map_err(|e| ExecErr::Storage(e))?;
        }
        _ => {} // TODO NOW: handle other input
                // Especially checking data type against schema
    }
    return Ok(());
}

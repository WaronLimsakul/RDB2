use crate::{
    interface::{Cmd, CmdType},
    storage::{EngineErr, engine::StorageEngine},
};

pub mod meta;

pub enum ExecErr {
    Storage(EngineErr), // Something wrong happen in storage engine
}

// TODO: Ok can be cursor or something interface should shows
pub fn execute(cmd: Cmd, engine: &mut StorageEngine) -> Result<(), ExecErr> {
    match cmd.cmd_type {
        CmdType::Meta(meta_cmd) => {
            meta::execute(meta_cmd, engine).map_err(|e| ExecErr::Storage(e))?;
        }
        _ => {} // TODO: handle other input
    }
    return Ok(());
}

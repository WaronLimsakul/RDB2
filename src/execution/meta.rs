//! # Meta Execution
//!
//! Main function for executiong meta command
//!

use std::process;

use crate::{
    execution::query::{Column, Row, Schema},
    interface::{MetaCmd, printer::PrintableTable, repl::output_table},
    storage::{ColData, EngineErr, Type, engine::StorageEngine},
};

pub fn execute(cmd: MetaCmd, engine: &mut StorageEngine) -> Result<(), EngineErr> {
    match cmd {
        MetaCmd::Quit => {
            engine.flush_all()?;
            process::exit(0);
        }
        MetaCmd::Tables => {
            let schema = Schema::from(vec![Column {
                name: "table".to_string(),
                col_type: Type::String,
            }]);
            let tables = engine
                .list_tables()?
                .into_iter()
                .map(|table| Row::from(vec![ColData::String(table)]))
                .collect();
            let report = PrintableTable {
                schema: &schema,
                rows: tables,
            };
            output_table(&report);
            Ok(())
        }
    }
}

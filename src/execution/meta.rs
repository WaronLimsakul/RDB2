//! # Meta Execution
//!
//! Main function for executiong meta command
//!

use std::process;

use crate::{
    execution::query::{Column, Row, Schema},
    interface::{
        MetaCmd,
        printer::PrintableTable,
        repl::{self, output_table},
    },
    storage::{ColData, EngineErr, Type, engine::StorageEngine},
};

const HELP_MSG: &str = r#"
RDB2 - a small relational database engine.

Meta commands
  .help             Show this message
  .tables, .ls      List every table in the database
  .quit, .exit      Flush changes to disk and exit

Statements (end with a semicolon)
  new table <name> { <col>: <type> primary, ... };
      Create a table. One column must be primary, typed uint or ulong.

  insert <name> (<values>);
  insert <name> [(<values>), ...];
      Insert one row, or several at once. Values are positional: primary
      key first, then the rest in the order the columns were declared.

  select <cols> from <name>[, <name>...] [where <cond>];
      Query rows. <cols> is * or a comma-separated list of column names.
      Listing several tables forms their cartesian product.

  describe <name>;  (desc works too)
      Show a table's schema.

  delete table <name>;
      Drop a table and its data.

Types        int, long, uint, ulong, float, bool, string
Conditions   =  !=  <  <=  >  >=   combined with &&
Keywords and types are case-insensitive.
"#;

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
        MetaCmd::Help => {
            repl::output(HELP_MSG);
            Ok(())
        }
    }
}

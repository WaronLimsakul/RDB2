use std::{fmt, num::TryFromIntError};

use crate::{
    execution::query::Row,
    interface::{Cmd, parser::LiteralExpr, printer, repl},
    storage::{EngineErr, Type, engine::StorageEngine},
};

mod cartesian;
mod filter;
mod meta;
mod project;
pub mod query;
mod scan;

#[derive(Debug)]
pub enum ExecErr {
    Storage(EngineErr),                   // Something wrong happen in storage engine
    InvalidColName(String),               // Invalid column name
    InvalidVal(LiteralExpr, Type),        // Invalid provided `LiteralExpr`, expect `Type` type
    IntConversion(TryFromIntError),       // Error converting int input value to target type
    UnmatchedNumValues(usize, usize), // Expect <first usize> values, user provides <second usize> values,
    NonNumericType(Type),             // Found `Type` in a place that should be for numeric type
    NonBooleanType(Type),             // Found `Type` in a place that should be for boolean type
    InvalidType(Type, Type),          // Expect first `Type`, found the second `Type`
    InvalidPredicate,                 // Not `col <op> literal` or `literal <op> col`
    AmbiguousCol(String),             // Specified column name is ambiguous (can mean many columns)
    InvalidJoinPredTypes(String, String), // Type of `String` column is not the same as second `String` column
}

/// Execute non-metadata command
/// Effect: output something if succeed
pub fn execute(cmd: Cmd, engine: &mut StorageEngine) -> Result<(), ExecErr> {
    match cmd {
        Cmd::Meta {
            cmd: meta_cmd,
            raw: _,
        } => {
            meta::execute(meta_cmd, engine).map_err(|e| ExecErr::Storage(e))?;
        }
        Cmd::DQL { ast, raw: _ } => {
            let mut exec_tree = query::execute_dql(ast, engine)?;
            let mut rows: Vec<Row> = Vec::new();
            // Keep driving the execution tree
            loop {
                match exec_tree.next() {
                    Ok(Some(row)) => {
                        rows.push(row);
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }

            let print_table = printer::PrintableTable {
                schema: exec_tree.schema(),
                rows,
            };
            // TODO: support other output, only terminal for now
            repl::output_table(&print_table);
        }
        Cmd::DDL { ast, raw: _ } => {
            query::execute_ddl(ast, engine)?;
        }
        Cmd::DML { ast, raw: _ } => {
            query::execute_dml(ast, engine)?;
        }
    }

    Ok(())
}

impl fmt::Display for ExecErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ExecErr::*;
        match self {
            Storage(e) => write!(f, "Storage error: {}", e),
            InvalidColName(c) => write!(f, "Invalid column '{}'.", c),
            InvalidVal(got, expect) => {
                write!(f, "Invalid provided value {}, expect type {}", got, expect)
            }
            IntConversion(e) => write!(f, "Invalid integer conversion: {}", e),
            UnmatchedNumValues(expect, got) => {
                write!(f, "User provide {} values, expect {} values", got, expect)
            }
            NonNumericType(got) => write!(f, "Expect numeric type, found {}", got),
            NonBooleanType(got) => write!(f, "Expect boolean type, found {}", got),
            InvalidType(expect, got) => write!(f, "Expect {} type, found {}", expect, got),
            InvalidPredicate => write!(
                f,
                "Invalid values in predicate, expect `column <op> literal` or `literal <op> column`"
            ),
            AmbiguousCol(col) => write!(f, "Column '{}' is ambiguous", col),
            InvalidJoinPredTypes(col1, col2) => write!(
                f,
                "Cross-column predicates of {} and {} has incompatible types",
                col1, col2
            ),
        }
    }
}

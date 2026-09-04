use std::{fmt, num::TryFromIntError};

use crate::{
    interface::{Cmd, parser::LiteralExpr},
    storage::{self, EngineErr, Type, engine::StorageEngine},
};

mod meta;
mod project;
mod query;
mod scan;

pub enum ExecErr {
    Storage(EngineErr),               // Something wrong happen in storage engine
    InvalidColName(String),           // Invalid column name
    InvalidVal(LiteralExpr, Type),    // Invalid provided `LiteralExpr`, expect `Type` type
    IntConversion(TryFromIntError),   // Error converting int input value to target type
    UnmatchedNumValues(usize, usize), // Expect <first usize> values, user provides <second usize> values,
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
        Cmd::DQL { ast, raw: _ } => {
            let mut exec_tree = query::execute_dql(ast, engine)?;
            // Keep driving the execution tree
            loop {
                match exec_tree.next() {
                    Ok(Some(row)) => {
                        println!("{:?}", row); // TODO NOW: print better
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
        }
        Cmd::DDL { ast, raw: _ } => {
            query::execute_ddl(ast, engine)?;
            println!("Executed"); // TODO NOW: print better
        }
        Cmd::DML { ast, raw: _ } => {
            query::execute_dml(ast, engine)?;
            println!("Executed"); // TODO NOW: print better
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
        }
    }
}

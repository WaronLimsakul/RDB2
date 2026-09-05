//! # Query Execution
//!
//! Main function to execute query language
//!

use crate::{
    execution::{ExecErr, project::Project, scan::Scan},
    interface::parser::{ColumnList, ExprNode, LiteralExpr, ParseTree, RowValueNode, Stmt},
    storage::{
        ColData, KeyData, KeyType, RecData, RowData, Type, engine::StorageEngine,
        row_cursor::RowCursor, table::TableSchema,
    },
};

// Something like storange's TableSchema, but we don't care
// about PK anymore. It's just another column, so we flatten it.
pub type Schema = Vec<Column>;

#[derive(Clone)]
pub struct Column {
    pub name: String,
    pub col_type: Type,
}

/// Every operator will pass on a `Row` (flatten RowData)
pub type Row = Vec<ColData>;

/// All execution tree node must implement this
pub trait Operator {
    // Get the next row
    fn next(&mut self) -> Result<Option<Row>, ExecErr>;
    // What is the schema of the output row
    fn schema(&self) -> &Schema;
}

/// Execute DQL: only `select` statement for now
pub fn execute_dql<'a>(
    query: ParseTree,
    engine: &'a mut StorageEngine,
) -> Result<Box<dyn Operator + 'a>, ExecErr> {
    debug_assert!(matches!(
        query.root,
        Stmt::Select {
            table: _,
            columns: _
        }
    ));

    // Extract table and columns out
    let (table, cols) = if let Stmt::Select { table, columns } = query.root {
        (table, columns)
    } else {
        panic!("Should always be select statement");
    };

    // Build execution tree from here
    // TODO: Might have to refactor it to somewhere else
    let scan = Scan::new(&table.name, engine)?;
    let exec_tree: Box<dyn Operator + 'a> = match cols {
        ColumnList::All => Box::from(scan),
        ColumnList::Listed(listed_cols) => Box::from(Project::new(listed_cols, Box::from(scan))?),
    };

    Ok(exec_tree)
}

/// Execute DDL: only `create table` for now
pub fn execute_ddl<'a>(query: ParseTree, engine: &'a mut StorageEngine) -> Result<(), ExecErr> {
    let (table, schema) = if let Stmt::New { table, schema } = query.root {
        (table, schema)
    } else {
        panic!("Should be new statement");
    };

    engine
        .new_table(table.name.as_str(), schema)
        .map_err(|e| ExecErr::Storage(e))?;

    Ok(())
}

/// Execute DML: only `insert` statement for now
pub fn execute_dml<'a>(query: ParseTree, engine: &'a mut StorageEngine) -> Result<(), ExecErr> {
    let (table_node, values) = if let Stmt::Insert { table, values } = query.root {
        (table, values)
    } else {
        panic!("Should be insert statement");
    };

    let table = engine
        .get_table_mut(table_node.name.as_str())
        .map_err(|e| ExecErr::Storage(e))?;

    for val_node in values {
        let row_data = assemble_row_data(val_node, table.schema())?;
        table
            .insert_row(row_data)
            .map_err(|e| ExecErr::Storage(e))?;
    }
    Ok(())
}

/// Convert parse tree's RowValueNode to storage engine's RowData
// TODO: Support positional + operator literal
fn assemble_row_data(val_node: RowValueNode, schema: &TableSchema) -> Result<RowData, ExecErr> {
    // NOTE: PK might have to be the first for now.

    // Check len first, so no index problem for sure
    if val_node.values.len() != schema.num_cols() {
        return Err(ExecErr::UnmatchedNumValues(
            schema.num_cols(),
            val_node.values.len(),
        ));
    }

    let mut values = val_node.values.into_iter();
    let key_data = if let ExprNode::Literal(literal) = values.next().unwrap() {
        literal_to_col_data(literal, schema.key.1.into())?
    } else {
        unimplemented!("Have not implemented non-literal key yet.");
    };

    let mut rec_data = RecData {
        vals: Vec::with_capacity(schema.vals.len()),
    };
    for (i, col_val) in values.enumerate() {
        let col_data = if let ExprNode::Literal(literal) = col_val {
            literal_to_col_data(literal, schema.vals[i].1)?
        } else {
            unimplemented!("Have not implemented non-literal data yet.");
        };
        rec_data.vals.push(col_data);
    }

    Ok(RowData {
        key: key_data.try_into().map_err(|e| ExecErr::Storage(e))?,
        vals: rec_data,
    })
}

/// Convert literal value from user input to storage engine's ColData
fn literal_to_col_data(literal: LiteralExpr, target: Type) -> Result<ColData, ExecErr> {
    use LiteralExpr::*;
    let col_data = match literal {
        Int(i) => match target {
            Type::Int => ColData::Int(i32::try_from(i).map_err(|e| ExecErr::IntConversion(e))?),
            Type::Uint => ColData::Uint(u32::try_from(i).map_err(|e| ExecErr::IntConversion(e))?),
            Type::Long => ColData::Long(i),
            Type::Ulong => ColData::Ulong(u64::try_from(i).map_err(|e| ExecErr::IntConversion(e))?),
            _ => {
                return Err(ExecErr::InvalidVal(literal, target));
            }
        },
        Float(f) => match target {
            Type::Float => ColData::Float(f as f32), // TODO: see if we should just parse as f32
            _ => {
                return Err(ExecErr::InvalidVal(literal, target));
            }
        },
        String(s) => {
            if target == Type::String {
                ColData::String(s)
            } else {
                return Err(ExecErr::InvalidVal(String(s), target));
            }
        }
        Bool(b) => {
            if target == Type::Bool {
                ColData::Bool(b)
            } else {
                return Err(ExecErr::InvalidVal(literal, target));
            }
        }
    };

    Ok(col_data)
}

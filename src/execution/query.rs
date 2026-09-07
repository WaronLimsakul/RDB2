//! # Query Execution
//!
//! Main function to execute query language
//!

use std::slice::Iter;

use crate::{
    execution::{ExecErr, filter::Filter, project::Project, scan::Scan},
    interface::parser::{ColumnList, ExprNode, LiteralExpr, OpExpr, ParseTree, RowValueNode, Stmt},
    storage::{ColData, RecData, RowData, Type, engine::StorageEngine, table::TableSchema},
};

// Something like storange's TableSchema, but we don't care
// about PK anymore. It's just another column, so we flatten it.
pub struct Schema {
    pub cols: Vec<Column>,
}

#[derive(Clone)]
pub struct Column {
    pub name: String,
    pub col_type: Type,
}

/// Every operator will pass on a `Row` (flatten RowData)
pub struct Row {
    pub data: Vec<ColData>,
}

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
            columns: _,
            conds: _
        }
    ));

    // Extract table and columns out
    let (table, cols, conds) = if let Stmt::Select {
        table,
        columns,
        conds,
    } = query.root
    {
        (table, columns, conds)
    } else {
        panic!("Should always be select statement");
    };

    // Build execution tree from here
    // TODO: Might have to refactor it to somewhere else

    // Always start with scan
    let scan = Box::from(Scan::new(&table.name, engine)?);

    // In case we have to filter
    let filterable_scan: Box<dyn Operator + 'a> = if !conds.preds.is_empty() {
        Box::from(Filter::new(scan, conds)?)
    } else {
        scan
    };

    // In case we have to project
    let exec_tree: Box<dyn Operator + 'a> = match cols {
        ColumnList::All => filterable_scan,
        ColumnList::Listed(listed_cols) => Box::from(Project::new(listed_cols, filterable_scan)?),
    };

    Ok(exec_tree)
}

/// Execute DDL: `new table` or `delete table`
pub fn execute_ddl<'a>(query: ParseTree, engine: &'a mut StorageEngine) -> Result<(), ExecErr> {
    match query.root {
        Stmt::New { table, schema } => {
            engine
                .new_table(table.name.as_str(), schema)
                .map_err(|e| ExecErr::Storage(e))?;
        }
        Stmt::Delete { table } => {
            engine
                .delete_table(table.name.as_str())
                .map_err(|e| ExecErr::Storage(e))?;
        }
        _ => unreachable!("DDL shouldn't be this"),
    }

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
    let key_data = expr_to_col_data(values.next().unwrap(), schema.key.1.into())?;

    let mut rec_data = RecData {
        vals: Vec::with_capacity(schema.vals.len()),
    };
    for (i, expr_node) in values.enumerate() {
        let col_data = expr_to_col_data(expr_node, schema.vals[i].1)?;
        rec_data.vals.push(col_data);
    }

    Ok(RowData {
        key: key_data.try_into().map_err(|e| ExecErr::Storage(e))?,
        vals: rec_data,
    })
}

/// Convert const expression node in parse tree to storage engine's ColData
fn expr_to_col_data(expr: ExprNode, target: Type) -> Result<ColData, ExecErr> {
    match expr {
        ExprNode::Op(op) => match op {
            OpExpr::Plus(lhs, rhs) => {
                if !target.is_numeric() {
                    return Err(ExecErr::NonNumericType(target));
                }
                let l_data = expr_to_col_data(*lhs, target)?;
                let r_data = expr_to_col_data(*rhs, target)?;
                Ok(l_data + r_data)
            }
            OpExpr::Minus(lhs, rhs) => {
                if !target.is_numeric() {
                    return Err(ExecErr::NonNumericType(target));
                }
                let l_data = expr_to_col_data(*lhs, target)?;
                let r_data = expr_to_col_data(*rhs, target)?;
                Ok(l_data - r_data)
            }
            OpExpr::Mult(lhs, rhs) => {
                if !target.is_numeric() {
                    return Err(ExecErr::NonNumericType(target));
                }
                let l_data = expr_to_col_data(*lhs, target)?;
                let r_data = expr_to_col_data(*rhs, target)?;
                Ok(l_data * r_data)
            }
            OpExpr::Div(lhs, rhs) => {
                if !target.is_numeric() {
                    return Err(ExecErr::NonNumericType(target));
                }
                let l_data = expr_to_col_data(*lhs, target)?;
                let r_data = expr_to_col_data(*rhs, target)?;
                Ok(l_data / r_data)
            }
            // TODO: support pred in insert here
            OpExpr::Eq(_, _)
            | OpExpr::Neq(_, _)
            | OpExpr::GT(_, _)
            | OpExpr::GTE(_, _)
            | OpExpr::LT(_, _)
            | OpExpr::LTE(_, _) => {
                unimplemented!("Have not implement other operators yet");
            }
        },
        ExprNode::Literal(lit) => literal_to_col_data(lit, target),
        ExprNode::Column(_) => {
            unimplemented!("Have not implement insert with column yet");
        }
    }
}

/// Convert literal value from user input to storage engine's ColData
pub fn literal_to_col_data(literal: LiteralExpr, target: Type) -> Result<ColData, ExecErr> {
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

/// Frequently used method for Row
impl Row {
    /// New Row with specified capacity for columns
    pub fn with_capacity(cap: usize) -> Self {
        Row {
            data: Vec::with_capacity(cap),
        }
    }

    /// New Row from provided column data
    pub fn from(data: Vec<ColData>) -> Self {
        Row { data }
    }

    /// Add col_data to row
    pub fn push(&mut self, col_data: ColData) {
        self.data.push(col_data)
    }

    pub fn num_cols(&self) -> usize {
        self.data.len()
    }

    /// Just iterator of column entry
    pub fn iter(&self) -> Iter<'_, ColData> {
        self.data.iter()
    }
}

/// Frequently used method for Schema
impl Schema {
    pub fn with_capacity(cap: usize) -> Self {
        Schema {
            cols: Vec::with_capacity(cap),
        }
    }

    /// New schema from the list of columns
    pub fn from(cols: Vec<Column>) -> Self {
        Schema { cols }
    }

    /// Add column to schema
    pub fn push(&mut self, col: Column) {
        self.cols.push(col)
    }

    pub fn num_cols(&self) -> usize {
        self.cols.len()
    }

    /// Find the target column (index, type) by name
    pub fn find_column(&self, name: &str) -> Option<(usize, Type)> {
        for (idx, col) in self.cols.iter().enumerate() {
            if col.name == name {
                return Some((idx, col.col_type));
            }
        }
        None
    }

    /// Just iterator of column entry
    pub fn iter(&self) -> Iter<'_, Column> {
        self.cols.iter()
    }
}

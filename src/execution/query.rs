//! # Query Execution
//!
//! Main function to execute query language
//!

use std::slice::Iter;

use crate::{
    execution::{
        ExecErr,
        filter::{Filter, PredOp},
        project::Project,
        scan::{Scan, ScanOption},
    },
    interface::{
        parser::{ColumnList, ExprNode, LiteralExpr, OpExpr, ParseTree, RowValueNode, Stmt},
        printer::PrintableTable,
        repl,
    },
    storage::{
        ColData, KeyData, KeyType, RecData, RowData, Type, engine::StorageEngine,
        table::TableSchema,
    },
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

    // In case we are filtering by primary key:
    // can tell storage engine to go there directly
    let (key_name, key_type) = &engine
        .get_schema(&table.name)
        .map_err(|e| ExecErr::Storage(e))?
        .key;
    let key_preds = find_key_preds(&conds.preds, key_name, key_type.clone())?;

    // TODO: support multiple key predicates
    let scan_opt = ScanOption {
        key: if !key_preds.is_empty() {
            Some(key_preds[0].1)
        } else {
            None
        },
    };

    // Build execution tree from here
    // TODO: Might have to refactor it to somewhere else

    // Always start with scan
    let scan = Box::from(Scan::new(&table.name, scan_opt, engine)?);

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
            repl::output("Done");
        }
        Stmt::Delete { table } => {
            engine
                .delete_table(table.name.as_str())
                .map_err(|e| ExecErr::Storage(e))?;
            repl::output("Done");
        }
        Stmt::Describe { table } => {
            let table_schema = engine
                .get_schema(&table.name)
                .map_err(|e| ExecErr::Storage(e))?;
            let report_schema = Schema::from(vec![
                Column {
                    name: "Column".to_string(),
                    col_type: Type::String,
                },
                Column {
                    name: "Type".to_string(),
                    col_type: Type::String,
                },
                Column {
                    name: "Comment".to_string(),
                    col_type: Type::String,
                },
            ]);

            let mut report_rows: Vec<Row> = Vec::new();

            let key = &table_schema.key;
            let key_row = Row {
                data: vec![
                    ColData::String(key.0.clone()),
                    ColData::String(format!("{}", key.1.clone())),
                    ColData::String("Primary Key".to_string()),
                ],
            };
            report_rows.push(key_row);

            for col in &table_schema.vals {
                let val_row = Row {
                    data: vec![
                        ColData::String(col.0.clone()),
                        ColData::String(format!("{}", col.1.clone())),
                        ColData::String("".to_string()),
                    ],
                };
                report_rows.push(val_row);
            }

            let report = PrintableTable {
                schema: &report_schema,
                rows: report_rows,
            };

            repl::output_table(&report);
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

    repl::output("Done");
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
            //
            // Numeric types
            //
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

            //
            // Boolean type
            //

            // TODO NOW: target is boolean but provided lhs and rhs don't have to be
            // need another recursive function that resolve type without target type
            OpExpr::Eq(lhs, rhs) => {
                if !target.is_bool() {
                    return Err(ExecErr::NonBooleanType(target));
                }
                let l_data = expr_to_literal(&*lhs);
                let r_data = expr_to_literal(&*rhs);
                Ok(ColData::Bool(l_data == r_data))
            }
            OpExpr::Neq(lhs, rhs) => {
                if !target.is_bool() {
                    return Err(ExecErr::NonBooleanType(target));
                }
                let l_data = expr_to_literal(&*lhs);
                let r_data = expr_to_literal(&*rhs);
                Ok(ColData::Bool(l_data != r_data))
            }
            OpExpr::GT(lhs, rhs) => {
                if !target.is_bool() {
                    return Err(ExecErr::NonBooleanType(target));
                }
                let l_data = expr_to_literal(&*lhs);
                let r_data = expr_to_literal(&*rhs);
                Ok(ColData::Bool(l_data > r_data))
            }
            OpExpr::GTE(lhs, rhs) => {
                if !target.is_bool() {
                    return Err(ExecErr::NonBooleanType(target));
                }
                let l_data = expr_to_literal(&*lhs);
                let r_data = expr_to_literal(&*rhs);
                Ok(ColData::Bool(l_data >= r_data))
            }
            OpExpr::LT(lhs, rhs) => {
                if !target.is_bool() {
                    return Err(ExecErr::NonBooleanType(target));
                }
                let l_data = expr_to_literal(&*lhs);
                let r_data = expr_to_literal(&*rhs);
                Ok(ColData::Bool(l_data < r_data))
            }
            OpExpr::LTE(lhs, rhs) => {
                if !target.is_bool() {
                    return Err(ExecErr::NonBooleanType(target));
                }
                let l_data = expr_to_literal(&*lhs);
                let r_data = expr_to_literal(&*rhs);
                Ok(ColData::Bool(l_data <= r_data))
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

// Evaluate an expression to literal expr
// requires: All the leaves must be literal
fn expr_to_literal(expr: &ExprNode) -> LiteralExpr {
    match expr {
        ExprNode::Literal(lit) => lit.clone(),
        ExprNode::Column(_) => unimplemented!("Haven't implement column as value yet"),
        ExprNode::Op(op) => match op {
            OpExpr::Plus(lhs, rhs) => {
                let llit = expr_to_literal(lhs);
                let rlit = expr_to_literal(rhs);
                llit + rlit
            }
            OpExpr::Minus(lhs, rhs) => {
                let llit = expr_to_literal(lhs);
                let rlit = expr_to_literal(rhs);
                llit - rlit
            }
            OpExpr::Mult(lhs, rhs) => {
                let llit = expr_to_literal(lhs);
                let rlit = expr_to_literal(rhs);
                llit * rlit
            }
            OpExpr::Div(lhs, rhs) => {
                let llit = expr_to_literal(lhs);
                let rlit = expr_to_literal(rhs);
                llit / rlit
            }
            _ => unimplemented!("Have not implement numeric operator in literal evaluation yet"),
        },
    }
}

/// Helper for DQL. Parse PK related predicate.
fn find_key_preds(
    preds: &Vec<ExprNode>,
    key_name: &str,
    key_type: KeyType,
) -> Result<Vec<(PredOp, KeyData)>, ExecErr> {
    let mut res: Vec<(PredOp, KeyData)> = Vec::new();
    for pred in preds {
        match pred {
            ExprNode::Op(op) => match op {
                // NOTE: only support =, >=, > for now.
                OpExpr::Eq(lhs, rhs) | OpExpr::GT(lhs, rhs) | OpExpr::GTE(lhs, rhs) => {
                    match (&**lhs, &**rhs) {
                        (ExprNode::Column(col), ExprNode::Literal(lit))
                        | (ExprNode::Literal(lit), ExprNode::Column(col)) => {
                            if col.name == key_name {
                                res.push((
                                    op.into(),
                                    literal_to_col_data(lit.clone(), key_type.into())?
                                        .try_into()
                                        .map_err(|e| ExecErr::Storage(e))?,
                                ));
                            }
                        }
                        _ => unreachable!("Should found a column in pred"),
                    }
                }
                _ => unreachable!("Shouldn't found non-pred op in pred"),
            },
            _ => unreachable!("Shouldn't found non-op expression in pred"),
        }
    }

    Ok(res)
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

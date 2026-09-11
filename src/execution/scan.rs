//! # Scan Operator
//!
//! Execution tree node that wraps storage
//! engine's RowCursor and implement Operator.
//!

use crate::{
    execution::{
        ExecErr,
        query::{Column, Operator, Row, Schema},
    },
    storage::{
        KeyData, RowData,
        engine::StorageEngine,
        row_cursor::RowCursor,
        table::{Table, TableSchema},
    },
};

pub struct Scan<'a> {
    source: RowCursor<'a>,
    schema: Schema,
    option: ScanOption,
}

/// Option for creating a scan operator
#[derive(Clone)]
pub struct ScanOption {
    pub key: Option<KeyData>, // in case we filter with primary key, scan can use storage index
}

impl<'a> Scan<'a> {
    pub fn new(table: &'a mut Table, option: ScanOption) -> Result<Self, ExecErr> {
        let source = if option.key.is_some() {
            table.find_row_by_key(option.key.unwrap())
        } else {
            table.get_all_rows()
        };
        let schema = storage_to_exec_schema(source.schema());
        let res = Scan {
            source,
            schema,
            option,
        };
        Ok(res)
    }

    /// Helper for converting storage's RowData to exec's Row
    fn storage_to_exec_row(&self, row: RowData) -> Row {
        let mut res = Row::with_capacity(self.schema.num_cols());
        res.push(row.key.into());
        for col_val in row.vals.vals {
            res.push(col_val);
        }
        res
    }
}

impl<'a> Operator for Scan<'a> {
    fn next(&mut self) -> Result<Option<Row>, ExecErr> {
        match self.source.next() {
            Some(Ok(row)) => Ok(Some(self.storage_to_exec_row(row))),
            Some(Err(storage_err)) => Err(ExecErr::Storage(storage_err)),
            None => Ok(None),
        }
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    // TODO: might have to set cursor with scan option after rewind
    fn rewind(&mut self) {
        self.source.rewind();
    }
}

/// Map storage engine's TableSchema to execution engine's Schema
fn storage_to_exec_schema(ts: &TableSchema) -> Schema {
    let mut schema = Schema::with_capacity(ts.num_cols());

    // TODO: check again if PK is always physically first in tuple
    schema.push(Column {
        name: ts.key.0.clone(),
        col_type: ts.key.1.into(),
    }); // push key

    for (name, col_type) in &ts.vals {
        schema.push(Column {
            name: name.clone(),
            col_type: col_type.clone(),
        });
    }

    schema
}

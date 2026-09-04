//! # Row Cursor
//!
//! Interface for iterating over row in table

use crate::storage::{EngineErr, RowData, cursor::Cursor, pager::Pager, table::TableSchema};

pub struct RowCursor<'a> {
    cursor: Cursor<'a>,
    schema: &'a TableSchema,
}

impl<'a> RowCursor<'a> {
    pub fn new(pager: &'a mut Pager, root_id: u32, schema: &'a TableSchema) -> RowCursor<'a> {
        RowCursor {
            cursor: Cursor::new(pager, root_id),
            schema,
        }
    }

    pub fn schema(&self) -> &TableSchema {
        self.schema
    }
}

impl Iterator for RowCursor<'_> {
    type Item = Result<RowData, EngineErr>;
    fn next(&mut self) -> Option<Self::Item> {
        let (key, byte_vals) = self.cursor.next()?;
        Some(
            self.schema
                .decode_val(&byte_vals)
                .map(|vals| RowData { key, vals }),
        )
    }
}

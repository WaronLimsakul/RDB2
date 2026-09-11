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

    /// Set the pointed page and cell for underlying cursor
    /// Use in case the caller what to start at a specific row.
    pub fn set(mut self, page_id: u32, cell_idx: u16) -> Self {
        self.cursor.set(page_id, cell_idx);
        self
    }

    /// Rewind the cursor back to the start like it's newly created
    pub fn rewind(&mut self) {
        self.cursor.rewind()
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

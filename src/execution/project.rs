//! # Project
//!
//! executiong tree node for projection
//!

use crate::{
    execution::{
        ExecErr,
        query::{Column, Operator, Row, Schema},
    },
    interface::parser::ColumnNode,
};

pub struct Project<'a> {
    source: Box<dyn Operator + 'a>, // need lifetime because source can come from borrowed engine
    indices: Vec<usize>,            // Projected columns indexed from source's output
    schema: Schema,                 // Our output schema
}

impl<'a> Project<'a> {
    /// Create new Proeject operator. Can fail because invalid column names.
    pub fn new(
        projected: Vec<ColumnNode>,
        source: Box<dyn Operator + 'a>,
    ) -> Result<Self, ExecErr> {
        let mut schema = Schema::with_capacity(projected.len());
        let mut indices: Vec<usize> = Vec::with_capacity(projected.len());

        let source_schema = source.schema();
        for target_col in projected {
            let (idx, col) = get_col(source_schema, target_col.name.as_str())
                .ok_or_else(|| ExecErr::InvalidColName(target_col.name))?;
            schema.push(col);
            indices.push(idx);
        }

        Ok(Project {
            source,
            indices,
            schema,
        })
    }
}

impl<'a> Operator for Project<'a> {
    fn next(&mut self) -> Result<Option<Row>, ExecErr> {
        match self.source.next() {
            Ok(Some(row)) => {
                let mut projected = Row::with_capacity(self.schema.num_cols());
                for idx in &self.indices {
                    // Has to clone, because sometimes, user select same column again
                    projected.push(row.data[*idx].clone());
                }

                Ok(Some(projected))
            }
            sth => sth,
        }
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    fn rewind(&mut self) {
        self.source.rewind();
    }
}

// Find index and type entry of schema from column
fn get_col(schema: &Schema, target_col: &str) -> Option<(usize, Column)> {
    for (i, col) in schema.cols.iter().enumerate() {
        if col.name == target_col {
            return Some((i, col.clone()));
        }
    }
    None
}

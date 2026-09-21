//! # Printer
//!
//! Anything about pretty printing.
//!

use std::fmt;

use crate::execution::query::{Column, Row, Schema};

pub struct PrintableTable<'a> {
    pub schema: &'a Schema,
    pub rows: Vec<Row>,
}

impl<'a> fmt::Display for PrintableTable<'a> {
    // TODO: support other language width
    // (can't just count with .chars().count())
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let col_lens = self.get_col_lens();

        //
        // Print table head
        //

        // First line:
        for len in &col_lens {
            write!(f, "+")?;
            write!(f, "{:-<len$}", "")?;
        }
        writeln!(f, "+")?;

        // Column names:
        for (
            i,
            Column {
                table,
                table_used,
                name,
                col_type: _,
            },
        ) in self.schema.iter().enumerate()
        {
            let len = col_lens[i];
            // If table used, have to print "table.col"
            let col = match table_used {
                true => &format!("{}.{}", table.as_ref().unwrap(), name),
                false => name,
            };
            write!(f, "|{:^len$}", col)?;
        }
        writeln!(f, "|")?;

        // Second line
        for len in &col_lens {
            write!(f, "+")?;
            write!(f, "{:-<len$}", "")?;
        }
        writeln!(f, "+")?;

        //
        // Print row value
        //

        for row in &self.rows {
            for (i, col) in row.iter().enumerate() {
                let len = col_lens[i];
                write!(f, "|{:^len$}", format!("{col}"))?;
            }
            writeln!(f, "|")?;
        }

        // last line
        for len in &col_lens {
            write!(f, "+")?;
            write!(f, "{:-<len$}", "")?;
        }
        writeln!(f, "+")
    }
}

impl<'a> PrintableTable<'a> {
    /// Return length of each aligned column we want to print
    fn get_col_lens(&self) -> Vec<usize> {
        // The schema and row should align
        debug_assert!(self.rows.is_empty() || self.schema.num_cols() == self.rows[0].num_cols());

        // Initialize column length with the column name length.
        // Always +2 because I want to give leading and trailing space.
        let mut col_lens = Vec::with_capacity(self.schema.num_cols());
        for col in &self.schema.cols {
            let col_len = col.name.len()
                + match col.table_used {
                    true => col.table.as_ref().unwrap().len() + 1, // "have to print <table>.<column>",
                    false => 0,
                };
            col_lens.push(col_len + 2); // Give left and right space for col name
        }

        // Compare to row's column length
        for row in &self.rows {
            for (i, col) in row.iter().enumerate() {
                col_lens[i] = col_lens[i].max(format!("{col}").chars().count() + 2);
            }
        }

        col_lens
    }
}

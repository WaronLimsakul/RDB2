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
        for (i, Column { name, col_type: _ }) in self.schema.iter().enumerate() {
            let len = col_lens[i];
            write!(f, "|{:^len$}", name)?;
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
        debug_assert!(self.rows.is_empty() || self.schema.len() == self.rows[0].len());

        // Initialize column length with the column name length.
        // Always +2 because I want to give leading and trailing space.
        let mut col_lens = Vec::with_capacity(self.schema.len());
        for col in self.schema {
            col_lens.push(col.name.len() + 2); // Give left and right space for col name
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

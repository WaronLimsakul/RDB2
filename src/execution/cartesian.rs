use crate::execution::{
    ExecErr,
    query::{Operator, Row, Schema},
};

pub struct Cartesian<'a> {
    source_1: Box<dyn Operator + 'a>,
    source_2: Box<dyn Operator + 'a>,
    schema: Schema,

    first: Option<Row>, // Currently evaluated source_1's row
}

impl<'a> Cartesian<'a> {
    pub fn new(source_1: Box<dyn Operator + 'a>, source_2: Box<dyn Operator + 'a>) -> Self {
        let schema = concat_schema(source_1.schema(), source_2.schema());
        Cartesian {
            source_1,
            source_2,
            schema,
            first: None,
        }
    }
}

impl<'a> Operator for Cartesian<'a> {
    fn next(&mut self) -> Result<Option<Row>, ExecErr> {
        // In case we haven't initialized self.first
        if self.first.is_none() {
            self.first = match self.source_1.next()? {
                Some(row) => Some(row),
                // source_1 has nothing, None right away
                None => return Ok(None),
            }
        }

        let second = match self.source_2.next()? {
            // source_2 runs out, move source_1 and rewind source_2
            None => {
                let first = self.source_1.next()?;
                if let Some(row) = first {
                    self.first = Some(row);
                } else {
                    // source_1 run out, we're done
                    return Ok(None);
                }
                self.source_2.rewind();
                self.source_2.next()?
            }
            Some(row) => Some(row),
        };

        // Even after rewind, second still None, then
        // source_2 is nothing, None right away
        if second.is_none() {
            return Ok(None);
        }

        debug_assert!(self.first.is_some());

        // Concat 2 rows
        Ok(Some(concat_row(
            self.first.as_ref().unwrap(),
            second.unwrap(),
        )))
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    fn rewind(&mut self) {
        self.source_1.rewind();
        self.source_2.rewind();
        self.first = None;
    }
}

/// Helper for concat 2 schemas, s1 then s2
fn concat_schema(s1: &Schema, s2: &Schema) -> Schema {
    let mut res = Schema::with_capacity(s1.num_cols() + s2.num_cols());
    for col in s1.iter() {
        res.push(col.clone());
    }
    for col in s2.iter() {
        res.push(col.clone());
    }
    res
}

/// Helper for concat 2 rows, r1 then r2
fn concat_row(r1: &Row, r2: Row) -> Row {
    let mut res = Row::with_capacity(r1.num_cols() + r2.num_cols());
    for data in r1.iter() {
        res.push(data.clone());
    }
    for data in r2.data.into_iter() {
        res.push(data)
    }
    res
}

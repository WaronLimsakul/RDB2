use std::io::{BufReader, Read, Write};

use crate::storage::{
    EngineErr::{self, *},
    KeyType, RowData, TABLE_MAGIC_NUMBER, Type,
    pager::{Pager, TableSrc},
};

/// Represent user-defined row schema in order
#[derive(Debug, PartialEq)]
pub struct TableSchema {
    pub key: (String, KeyType), // type for id
    pub vals: Vec<(String, Type)>,
}

impl TableSchema {
    fn num_cols(&self) -> usize {
        1 + self.vals.len()
    }
}

/// Represent a file or table
/// change metadata: change new, try_from_src, read_header
pub struct Table {
    schema: TableSchema,
    pager: Pager,
    root_id: u32, // ID of the root node
}

impl Table {
    /// Create new table from user provided data
    pub fn new(
        schema: TableSchema,
        num_pages: u32,
        header_size: usize,
        table_src: Box<dyn TableSrc>,
        root_id: u32,
    ) -> Table {
        Table {
            schema,
            root_id,
            pager: Pager::new(table_src, num_pages, header_size),
        }
    }

    /// Create a table by reading metadata from the reader
    // It should
    // 1. Read and check the magic number
    // 2. Parse the table schema
    // 3. Read the num pages
    // 4. Get the header size from stream pos
    // 5. Return table
    pub fn try_from_src<T: TableSrc + 'static>(reader: T) -> Result<Self, EngineErr> {
        let mut br = BufReader::new(reader);

        let mut magic = [0u8; 8];
        br.read_exact(magic.as_mut_slice())
            .map_err(|e| FsErr(Box::new(e)))?;
        if magic != TABLE_MAGIC_NUMBER {
            return Err(InvalidMagicNumber);
        }

        let num_cols = read_u64(&mut br)?;
        let mut schema = TableSchema {
            key: (String::new(), KeyType::Uint), // place holder
            vals: Vec::with_capacity(num_cols),
        };

        for i in 0..num_cols {
            let col_name = read_string(&mut br)?;

            let mut col_type_byte = [0u8];
            br.read_exact(col_type_byte.as_mut_slice())
                .map_err(|e| FsErr(Box::new(e)))?;
            let col_type = Type::from_byte(col_type_byte[0]).ok_or(InvalidTypeByte)?;

            // first column is key
            if i == 0 {
                schema.key = (col_name, KeyType::try_from(col_type)?);
            } else {
                schema.vals.push((col_name, col_type));
            }
        }

        // read num pages
        let num_pages = read_u32(&mut br)?;

        // read root id
        let root_id = read_u32(&mut br)?;

        // done with the header reading, now check header size

        // gotta align with inner position first
        br.seek_relative(0).map_err(|e| FsErr(Box::new(e)))?;
        let mut src = br.into_inner();
        let header_size = src.stream_position().map_err(|e| FsErr(Box::new(e)))?;

        // let pager use underline reader instead
        return Ok(Table {
            schema,
            root_id,
            pager: Pager::new(Box::new(src), num_pages, header_size as usize),
        });
    }

    /// Write a table header to a writer. num_pages set to 0.
    /// see format in [adr file](../../docs/adr/06-root-id-and-page-count-in-file-header.md)
    /// return number of bytes written
    pub fn write_header<W: Write>(mut writer: W, schema: &TableSchema) -> Result<usize, EngineErr> {
        // build header: starts with the magic number
        let mut bytes = Vec::from(TABLE_MAGIC_NUMBER);

        // how many columns
        bytes.extend_from_slice(&schema.num_cols().to_be_bytes());

        // key column data
        bytes.extend_from_slice(&str_len(&schema.key.0)?);
        bytes.extend_from_slice(&schema.key.0.as_bytes());
        bytes.push(Type::from(schema.key.1).to_byte());

        // value column data
        for (col_name, t) in schema.vals.iter() {
            bytes.extend_from_slice(&str_len(col_name)?);
            bytes.extend_from_slice(&col_name.as_bytes());
            bytes.push(t.to_byte());
        }

        // write 2 0's in u32
        // 1. first 0 = number of page in u32
        // 2. second 0 = initial root id in u32
        bytes.extend_from_slice(&0u64.to_be_bytes());

        // write ts out
        writer.write(&bytes).map_err(|e| FsErr(Box::new(e)))
    }

    /// Inserts row to it
    // TODO NOW:
    // 1. find page to insert (traverse tree)
    // 2. insert
    // 3. if page full, split
    pub fn insert_row(&mut self, data: RowData) -> Result<(), EngineErr> {
        let root_id = self.root_id;
        let key = data.key;
        let mut cur_page = self.pager.page_mut(root_id).ok_or(PageNotExists(root_id))?;
        while !cur_page.is_leaf() {
            cur_page = cur_page.find_ptr_pos(key);
        }

        Ok(())
    }
}

/// return length of string in u16
fn str_len(s: &String) -> Result<[u8; 2], EngineErr> {
    let data = u16::try_from(s.len()).map_err(|_| InvalidStrLenght)?;
    return Ok(data.to_be_bytes());
}

/// helper function for reading usize from reader
fn read_u64<R: Read>(reader: &mut R) -> Result<usize, EngineErr> {
    let mut buf = [0u8; 8];
    reader
        .read_exact(&mut buf)
        .map_err(|e| FsErr(Box::new(e)))?;
    Ok(usize::from_be_bytes(buf))
}

/// helper function for reading usize from reader
fn read_u32<R: Read>(reader: &mut R) -> Result<u32, EngineErr> {
    let mut buf = [0u8; 4];
    reader
        .read_exact(&mut buf)
        .map_err(|e| FsErr(Box::new(e)))?;
    Ok(u32::from_be_bytes(buf))
}

/// helper function for reading u16 from reader
fn read_u16<R: Read>(reader: &mut R) -> Result<u16, EngineErr> {
    let mut buf = [0u8; 2];
    reader
        .read_exact(&mut buf)
        .map_err(|e| FsErr(Box::new(e)))?;
    Ok(u16::from_be_bytes(buf))
}

/// read rdb string from reader
fn read_string<R: Read>(reader: &mut R) -> Result<String, EngineErr> {
    let len = read_u16(reader)?;
    let mut str_bytes = vec![0u8; usize::from(len)];
    reader
        .read_exact(str_bytes.as_mut_slice())
        .map_err(|e| FsErr(Box::new(e)))?;
    String::from_utf8(str_bytes).map_err(|_| InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: rewrite this test
    // #[test]
    // fn test_read_write_header() {
    //     let schema = TableSchema {
    //         key: ("id".to_string(), KeyType::Ulong),
    //         vals: vec![
    //             ("name".to_string(), Type::String),
    //             ("age".to_string(), Type::Uint),
    //             ("retired".to_string(), Type::Bool),
    //         ],
    //     };
    //
    //     // test writing normal header
    //     let mut buf: Vec<u8> = Vec::new();
    //     write_header(&mut buf, &schema).expect("Test write failed");
    //
    //     // test reading the header
    //     let res = read_table_header(buf.as_slice()).expect("Test read failed");
    //     assert_eq!(res, schema);
    // }
}

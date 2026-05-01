use std::{collections::HashMap, fs, path::PathBuf};

use crate::storage::{
    KeyData, RowData, TABLE_FILE_EXTENSION,
    table::{Table, TableSchema},
};

use super::EngineErr::{self, *};

/// Represents lower level engine that deals with disk's files
pub struct StorageEngine {
    root_dir: PathBuf,
    tables: HashMap<String, Table>, // filename -> table abstract
}

impl StorageEngine {
    pub fn new(root: &str) -> Result<StorageEngine, EngineErr> {
        if let Ok(false) = fs::exists(root) {
            if let Err(err) = fs::create_dir(root) {
                return Err(FsErr(Box::new(err)));
            }
        }

        Ok(StorageEngine {
            root_dir: PathBuf::from(root),
            tables: HashMap::new(),
        })
    }

    /// Create new table with given name and schema
    // It should
    // 1. write the file header (error if file already exists)
    // 2. save table in tables cache
    pub fn new_table(&mut self, name: &str, schema: TableSchema) -> Result<(), EngineErr> {
        let path = self
            .root_dir
            .join(name)
            .with_extension(TABLE_FILE_EXTENSION);

        let file = fs::File::options()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|_| TableAlreadyExists(name.to_string()))?;

        let header_size = Table::write_header(&file, &schema)?;

        // assume no page and first root id is 1. I mean of course
        // TODO: Check if I should use first root id 0
        self.tables.insert(
            name.to_string(),
            Table::new(schema, 0, header_size, Box::new(file), 0),
        );
        Ok(())
    }

    /// Flush change that happen to the underline file
    pub fn flush(&self, name: &str) {
        println!("flush table {name}");
    }

    /// Insert row to target table with provided information
    /// requires: data is valid for table schema
    // 1. check if table in cache: if not, fetch it
    // 2. check if ID already exists: if so, error
    // 3. insert row to node
    pub fn insert_row(&mut self, table_name: &str, data: RowData) -> Result<(), EngineErr> {
        // fetch table metadata if not there
        if !self.tables.contains_key(table_name) {
            let file = fs::File::open(table_name).map_err(|e| FsErr(Box::new(e)))?;
            self.tables
                .insert(table_name.to_string(), Table::try_from_src(file)?);
        }

        // TODO:
        // 1. find page to insert (traverse tree)
        // 2. insert
        // 3. if page full, split
        self.tables[table_name].insert_row();

        return Ok(());
    }

    /// Find a row with that key, returns None if row not found
    pub fn find_row_by_key(key: KeyData) -> Option<RowData> {
        // TODO:
        return None;
    }

    //
    // pub fn delete(table_id: u32, node_id: u32) {
    //     println!("delete called");
    // }
    //
    // pub fn update(table_id: u32, node_id: u32) {
    //     println!("update called");
    // }
    //
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

    // TODO: test page level function
    // - insert cell
    // - read cell
    // - find ptr pos
    // - defragment
}

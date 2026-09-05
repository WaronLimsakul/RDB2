//! Storage Engine
//!
//! Whatever bad happen to the disk file, blame this guy

use std::{collections::HashMap, fs, path::PathBuf};

use crate::storage::{
    KeyData, RowData, TABLE_FILE_EXTENSION,
    row_cursor::RowCursor,
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
        let path = self.get_table_file_path(name);
        let file = fs::File::options()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|_| TableAlreadyExists(name.to_string()))?;

        let header_size = Table::write_header(&file, &schema)?;

        // assume no page and first root id is 0
        self.tables.insert(
            name.to_string(),
            Table::new(schema, 0, header_size, Box::new(file), 0),
        );

        Ok(()) // NOTE: don't have to insert any page, will do that when insert first row
    }

    /// Get schema of the target table
    pub fn get_schema(&mut self, table_name: &str) -> Result<&TableSchema, EngineErr> {
        let table = self.get_table(table_name)?;
        Ok(table.schema())
    }

    /// Flush all tables that engine has processed in this session
    pub fn flush_all(&mut self) -> Result<(), EngineErr> {
        for (_, table) in &mut self.tables {
            table.flush()?;
        }
        Ok(())
    }

    /// Flush change that happen to the underline file
    pub fn flush(&mut self, name: &str) -> Result<(), EngineErr> {
        self.tables
            .get_mut(name)
            .ok_or(TableNotFound(name.to_string()))?
            .flush()
    }

    /// Insert row to target table with provided information
    /// requires: data is valid for table schema
    // 1. check if table in cache: if not, fetch it
    // 2. check if ID already exists: if so, error
    // 3. insert row to node
    pub fn insert_row(&mut self, table_name: &str, data: RowData) -> Result<(), EngineErr> {
        let table = self.get_table_mut(table_name)?;
        table.insert_row(data)
    }

    /// Return Iterator of each row data
    pub fn get_all_rows(&mut self, table_name: &str) -> Result<RowCursor, EngineErr> {
        let table = self.get_table_mut(table_name)?;
        Ok(table.get_all_rows())
    }

    /// Find a row with that key, returns None if row not found
    pub fn find_row_by_key(
        &mut self,
        table_name: &str,
        key: KeyData,
    ) -> Result<Option<RowData>, EngineErr> {
        let table = self.get_table_mut(table_name)?;
        Ok(table.find_row_by_key(key))
    }

    /// Get table from table name, open the file and read all
    /// the metadata if table is not in cache yet.
    fn get_table(&mut self, table_name: &str) -> Result<&Table, EngineErr> {
        if !self.tables.contains_key(table_name) {
            self.open_table(table_name)?;
        }
        Ok(self.tables.get(table_name).unwrap())
    }

    /// Get mutable table from table name, open the file and read all
    /// the metadata if table is not in cache yet.
    pub fn get_table_mut(&mut self, table_name: &str) -> Result<&mut Table, EngineErr> {
        if !self.tables.contains_key(table_name) {
            self.open_table(table_name)?;
        }
        Ok(self.tables.get_mut(table_name).unwrap())
    }

    /// Open table's corresponding file, read metadata and save to cache
    // requires: table must not be in cache before.
    fn open_table(&mut self, table_name: &str) -> Result<(), EngineErr> {
        debug_assert!(!self.tables.contains_key(table_name));
        let path = self.get_table_file_path(table_name);
        let file = fs::File::options()
            .read(true)
            .write(true)
            .create(false)
            .truncate(false)
            .open(&path)
            .map_err(|_| TableNotFound(table_name.to_string()))?;

        self.tables
            .insert(String::from(table_name), Table::try_from_src(file)?);

        Ok(())
    }

    /// Return file path that the table supposed to be
    fn get_table_file_path(&self, table_name: &str) -> PathBuf {
        self.root_dir
            .join(table_name)
            .with_extension(TABLE_FILE_EXTENSION)
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
    use crate::storage::{ColData, KeyType, RecData, Type};

    fn setup_test_dir(name: &str) -> String {
        let dir = format!("/tmp/rdb2_test_{}", name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn make_schema() -> TableSchema {
        TableSchema {
            key: ("id".to_string(), KeyType::Uint),
            vals: vec![
                ("name".to_string(), Type::String),
                ("age".to_string(), Type::Uint),
            ],
        }
    }

    fn make_row(id: u32, name: &str, age: u32) -> RowData {
        RowData {
            key: KeyData::Uint(id),
            vals: RecData {
                vals: vec![ColData::String(name.to_string()), ColData::Uint(age)],
            },
        }
    }

    /// Create table, insert 3 rows, verify get_all_rows returns them in key order.
    #[test]
    fn test_happy_path() {
        let dir = setup_test_dir("happy_path");
        let mut engine = StorageEngine::new(&dir).unwrap();
        engine.new_table("test", make_schema()).unwrap();

        engine.insert_row("test", make_row(1, "Alice", 30)).unwrap();
        engine.insert_row("test", make_row(2, "Bob", 25)).unwrap();
        engine
            .insert_row("test", make_row(3, "Charlie", 35))
            .unwrap();

        let cursor = engine.get_all_rows("test").unwrap();
        let rows: Vec<RowData> = cursor.collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].key, KeyData::Uint(1));
        assert_eq!(rows[1].key, KeyData::Uint(2));
        assert_eq!(rows[2].key, KeyData::Uint(3));
    }

    /// Insert rows, then find by key — existing key returns Some, missing returns None.
    #[test]
    fn test_find_by_key() {
        let dir = setup_test_dir("find_by_key");
        let mut engine = StorageEngine::new(&dir).unwrap();
        engine.new_table("test", make_schema()).unwrap();

        engine
            .insert_row("test", make_row(10, "Alice", 30))
            .unwrap();
        engine.insert_row("test", make_row(20, "Bob", 25)).unwrap();

        let found = engine.find_row_by_key("test", KeyData::Uint(10)).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().key, KeyData::Uint(10));

        let missing = engine.find_row_by_key("test", KeyData::Uint(99)).unwrap();
        assert!(missing.is_none());
    }

    /// Insert same key twice — second insert yields Err(RowExists).
    #[test]
    fn test_duplicate_key() {
        let dir = setup_test_dir("duplicate_key");
        let mut engine = StorageEngine::new(&dir).unwrap();
        engine.new_table("test", make_schema()).unwrap();

        engine.insert_row("test", make_row(1, "Alice", 30)).unwrap();
        let err = engine.insert_row("test", make_row(1, "Bob", 25));
        assert!(matches!(err, Err(EngineErr::RowExists(_))));
    }

    /// Helper schema with a single String value column (meaty payload to force page splits)
    fn stress_schema() -> TableSchema {
        TableSchema {
            key: ("id".into(), KeyType::Uint),
            vals: vec![("data".into(), Type::String)],
        }
    }

    fn make_stress_row(id: u32, payload: &str) -> RowData {
        RowData {
            key: KeyData::Uint(id),
            vals: RecData {
                vals: vec![ColData::String(payload.into())],
            },
        }
    }

    /// Insert enough rows to force multiple b-tree splits, then verify everything.
    /// ~19 rows per leaf page with 200-byte strings, so 80 rows → ~5 leaf pages + internal nodes.
    #[test]
    fn test_split_stress() {
        let dir = setup_test_dir("split_stress");
        let mut engine = StorageEngine::new(&dir).unwrap();
        engine.new_table("test", stress_schema()).unwrap();

        let payload = "X".repeat(200);
        let num_rows = 80;

        // Insert out of key order to exercise the b-tree's binary search + splits
        for i in 0..num_rows {
            let id = if i % 2 == 1 {
                (i + 1) / 2
            } else {
                num_rows - (i / 2)
            };

            engine
                .insert_row("test", make_stress_row(id as u32, &payload))
                .expect("insert-row should succeed")
        }

        // Verify all rows are returned in key order
        let cursor = engine.get_all_rows("test").unwrap();
        let rows: Vec<RowData> = cursor.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(rows.len(), num_rows);

        for (idx, row) in rows.iter().enumerate() {
            assert_eq!(
                row.key,
                KeyData::Uint((idx + 1) as u32),
                "Rows should be in ascending key order at position {idx}"
            );
        }

        // Verify random single-key lookups
        let found = engine.find_row_by_key("test", KeyData::Uint(1)).unwrap();
        assert!(found.is_some(), "Should find key 1");

        let found = engine
            .find_row_by_key("test", KeyData::Uint(num_rows as u32))
            .unwrap();
        assert!(found.is_some(), "Should find key {num_rows}");

        let found = engine.find_row_by_key("test", KeyData::Uint(50)).unwrap();
        assert!(found.is_some(), "Should find key 50");

        let missing = engine.find_row_by_key("test", KeyData::Uint(999)).unwrap();
        assert!(missing.is_none(), "Key 999 should not exist");
    }

    /// Fresh table with no rows — get_all_rows yields empty, find returns None.
    #[test]
    fn test_empty_table() {
        let dir = setup_test_dir("empty_table");
        let mut engine = StorageEngine::new(&dir).unwrap();
        engine.new_table("test", make_schema()).unwrap();

        let cursor = engine.get_all_rows("test").unwrap();
        let rows: Vec<RowData> = cursor.collect::<Result<Vec<_>, _>>().unwrap();
        assert!(rows.is_empty());

        let found = engine.find_row_by_key("test", KeyData::Uint(1)).unwrap();
        assert!(found.is_none());
    }

    /// Flush table to disk, then open a fresh engine on the same directory
    /// and verify data persists.
    #[test]
    fn test_flush_reopen() {
        let dir = setup_test_dir("flush_reopen");

        // First session: write data and flush
        {
            let mut engine = StorageEngine::new(&dir).unwrap();
            engine.new_table("test", make_schema()).unwrap();
            engine.insert_row("test", make_row(1, "Alice", 30)).unwrap();
            engine.insert_row("test", make_row(2, "Bob", 25)).unwrap();
            engine.flush("test").unwrap();
        }

        // Second session: reload from disk
        {
            let mut engine = StorageEngine::new(&dir).unwrap();
            // Trigger table load from file by inserting a new row
            engine
                .insert_row("test", make_row(3, "Charlie", 35))
                .unwrap();

            let cursor = engine.get_all_rows("test").unwrap();
            let rows: Vec<RowData> = cursor.collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(rows.len(), 3);
            assert_eq!(rows[0].key, KeyData::Uint(1));
            assert_eq!(rows[1].key, KeyData::Uint(2));
            assert_eq!(rows[2].key, KeyData::Uint(3));
        }
    }

    // Helper for test_float_type
    fn schema_float() -> TableSchema {
        TableSchema {
            key: ("id".to_string(), KeyType::Uint),
            vals: vec![
                ("name".to_string(), Type::String),
                ("win_rate".to_string(), Type::Float),
            ],
        }
    }

    // Helper for test_float_type
    fn row_float(id: u32, name: &str, win_rate: f32) -> RowData {
        RowData {
            key: KeyData::Uint(id),
            vals: RecData {
                vals: vec![ColData::String(name.to_string()), ColData::Float(win_rate)],
            },
        }
    }

    #[test]
    fn test_float_type() {
        let dir = setup_test_dir("float_type");
        let mut engine = StorageEngine::new(&dir).unwrap();
        engine.new_table("test", schema_float()).unwrap();

        engine
            .insert_row("test", row_float(1, "Alice", 0.5))
            .unwrap();
        engine.insert_row("test", row_float(2, "Bob", 0.6)).unwrap();
        engine
            .insert_row("test", row_float(3, "Charlie", 0.4))
            .unwrap();

        let cursor = engine.get_all_rows("test").unwrap();
        let rows: Vec<RowData> = cursor.collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].vals.vals[1], ColData::Float(0.5));
        assert_eq!(rows[1].vals.vals[1], ColData::Float(0.6));
        assert_eq!(rows[2].vals.vals[1], ColData::Float(0.4));
    }
}

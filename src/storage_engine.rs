use EngineErr::*;
use std::{
    collections::HashMap, error::Error, fmt::Display, fs, io::Write,
    path::PathBuf,
};

/// Allowed row types
pub enum Type {
    Int,
    Uint,
    Long,
    Ulong,
    String,
    Bool,
}

#[derive(Debug)]
pub enum EngineErr {
    TableAlreadyExists(String),
    FsErr(Box<dyn Error>),
}

impl Display for EngineErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableAlreadyExists(name) => {
                write!(f, "Table {name} already exists.")
            }
            FsErr(err) => write!(f, "Fs error: {}", err),
        }
    }
}

impl std::error::Error for EngineErr {}

/// page size (basic disk read/write unit) in bytes
/// TODO: should depends on arch
const PAGE_SIZE: u32 = 4096;

/// 1 page = 1 b-tree node
struct Page {}

/// Represent user-defined row schema in order
/// NOTE: first one must be id and should be comparable type for now
type TableSchema = Vec<(String, Type)>;

/// Represent a file or table
struct Table {
    schema: TableSchema,
    pages: Vec<Page>,
}

/// Represent lower level engine that deals with disk's files
pub struct StorageEngine {
    root_dir: PathBuf,
    tables: HashMap<String, Table>, // filename -> table abstract
}

// TODO: see if it should be method or just fn
// TODO 2: define custom error, see if defined here or other place
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
    // 1. check if file exists
    // 2. write the file header (error if file already exists)
    // 3. save table in tables cache
    pub fn new_table(
        &mut self,
        name: &str,
        schema: TableSchema,
    ) -> Result<(), EngineErr> {
        let path = self.root_dir.join(name).with_extension("rdb");

        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|_| TableAlreadyExists(name.to_string()))?;

        let table = self.write_table_header(file, schema)?;

        self.tables.insert(String::from(name), table);
        Ok(())
    }

    /// Flush change that happen to the underline file
    pub fn flush(&self, name: &str) {
        println!("flush table {name}");
    }

    // pub fn insert(table_id: u32, node_id: u32) {
    //     println!("insert called");
    // }
    //
    // pub fn delete(table_id: u32, node_id: u32) {
    //     println!("delete called");
    // }
    //
    // pub fn update(table_id: u32, node_id: u32) {
    //     println!("update called");
    // }
    //

    /// setup table header to a new file
    /// see format in [adr file](../docs/adr/01-file-table-schema-format.md)
    fn write_table_header(
        &self,
        mut file: fs::File,
        schema: TableSchema,
    ) -> Result<Table, EngineErr> {
        match file
            .write(format!("hello, this is schema: {}", schema[0].0).as_bytes())
        {
            Err(err) => Err(FsErr(Box::new(err))),
            _ => Ok(Table {
                schema,
                pages: Vec::new(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO
}

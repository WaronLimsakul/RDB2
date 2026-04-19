use EngineErr::*;
use std::{
    collections::HashMap, error::Error, fmt::Display, fs, io::Write,
    path::PathBuf,
};

const TABLE_MAGIC_NUMBER: [u8; 8] =
    [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
const TABLE_FILE_EXTENSION: &str = "rdb";

/// page size (basic disk read/write unit) in bytes
/// TODO: should depends on arch
const PAGE_SIZE: u32 = 4096;

/// Allowed row types
// NOTE: change this -> change Display, and other impl
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Type {
    Int,
    Uint,
    Long,
    Ulong,
    String,
    Bool,
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Type::*;
        match self {
            Int => write!(f, "integer"),
            Uint => write!(f, "unsigned integer"),
            Long => write!(f, "long"),
            Ulong => write!(f, "unsigned long"),
            String => write!(f, "string"),
            Bool => write!(f, "bolean"),
        }
    }
}

impl Type {
    fn to_byte(&self) -> u8 {
        use Type::*;
        match self {
            Int => 0,
            Uint => 1,
            Long => 2,
            Ulong => 3,
            String => 4,
            Bool => 5,
        }
    }

    fn from_byte(b: u8) -> Option<Type> {
        use Type::*;
        match b {
            0 => Some(Int),
            1 => Some(Uint),
            2 => Some(Long),
            3 => Some(Ulong),
            4 => Some(String),
            5 => Some(Bool),
            _ => None,
        }
    }
}

/// Error type for engine, just display to see what to wanna say
#[derive(Debug)]
pub enum EngineErr {
    TableAlreadyExists(String),
    FsErr(Box<dyn Error>),
    UnSupportedType(Type),
    Empty(String),
}

impl Display for EngineErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableAlreadyExists(name) => {
                write!(f, "Table {name} already exists.")
            }
            FsErr(err) => write!(f, "Fs error: {}", err),
            UnSupportedType(t) => {
                write!(f, "Type {t} unsupported for the task.")
            }
            Empty(what) => write!(f, "{what} is empty."),
        }
    }
}

impl std::error::Error for EngineErr {}

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
    // 1. check if schema valid
    // 2. check if file exists
    // 3. write the file header (error if file already exists)
    // 4. save table in tables cache
    pub fn new_table(
        &mut self,
        name: &str,
        schema: TableSchema,
    ) -> Result<(), EngineErr> {
        // check empty schema
        if schema.is_empty() {
            return Err(Empty(String::from("schema")));
        }

        // first check if first element (id) is ulong
        if schema[0].1 != Type::Ulong {
            return Err(UnSupportedType(schema[0].1));
        }

        let path = self
            .root_dir
            .join(name)
            .with_extension(TABLE_FILE_EXTENSION);

        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|_| TableAlreadyExists(name.to_string()))?;

        let table = write_table_header(file, schema)?;

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
}

/// setup table header to a new file
/// see format in [adr file](../docs/adr/01-file-table-schema-format.md)
fn write_table_header(
    mut file: fs::File,
    schema: TableSchema,
) -> Result<Table, EngineErr> {
    // build header: starts with the magic number
    let mut bytes = Vec::from(TABLE_MAGIC_NUMBER);

    // how many columns
    bytes.extend_from_slice(&schema.len().to_be_bytes());

    // column data
    for (col_name, t) in schema.iter() {
        bytes.extend_from_slice(&col_name.len().to_be_bytes());
        bytes.extend_from_slice(&col_name.as_bytes());
        bytes.push(t.to_byte());
    }

    // write ts out
    match file.write(&bytes) {
        Err(err) => Err(FsErr(Box::new(err))),
        _ => Ok(Table {
            schema,
            pages: Vec::new(),
        }),
    }
}

#[cfg(test)]
mod tests {
    // TODO
}

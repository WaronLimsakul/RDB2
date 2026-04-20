use EngineErr::*;
use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs,
    io::{self, Read, Write},
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
    InvalidMagicNumber,
    InvalidUtf8,
    InvalidTypeByte,
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
            InvalidMagicNumber => write!(f, "Unmatched magic number."),
            InvalidUtf8 => write!(f, "Found invalid UTF-8."),
            InvalidTypeByte => write!(f, "Found invalid type byte."),
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
fn write_table_header<W: io::Write>(
    mut writer: W,
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
    match writer.write(&bytes) {
        Err(err) => Err(FsErr(Box::new(err))),
        _ => Ok(Table {
            schema,
            pages: Vec::new(),
        }),
    }
}

/// read table metadata from reader
// It should
// 1. check the magic number
// 2. parse all the column name and type
// 3. return table schema
fn read_table_header<R: io::Read>(reader: R) -> Result<TableSchema, EngineErr> {
    let mut br = io::BufReader::new(reader);

    let mut magic = [0u8; 8];
    br.read_exact(magic.as_mut_slice())
        .map_err(|e| FsErr(Box::new(e)))?;
    if magic != TABLE_MAGIC_NUMBER {
        return Err(InvalidMagicNumber);
    }

    let num_cols = read_usize(&mut br)?;
    let mut schema: TableSchema = Vec::with_capacity(num_cols);

    for _ in 0..num_cols {
        let str_len = read_usize(&mut br)?;
        let col_name = read_string(&mut br, str_len)?;

        let mut col_type_byte = [0u8];
        br.read_exact(col_type_byte.as_mut_slice())
            .map_err(|e| FsErr(Box::new(e)))?;
        let col_type =
            Type::from_byte(col_type_byte[0]).ok_or(InvalidTypeByte)?;

        schema.push((col_name, col_type));
    }

    return Ok(schema);
}

/// helper function for reading usize from reader
fn read_usize<R: Read>(reader: &mut R) -> Result<usize, EngineErr> {
    let mut buf = [0u8; 8];
    reader
        .read_exact(&mut buf)
        .map_err(|e| FsErr(Box::new(e)))?;
    Ok(usize::from_be_bytes(buf))
}

fn read_string<R: Read>(
    reader: &mut R,
    len: usize,
) -> Result<String, EngineErr> {
    let mut str_bytes = vec![0u8; len];
    reader
        .read_exact(str_bytes.as_mut_slice())
        .map_err(|e| FsErr(Box::new(e)))?;
    String::from_utf8(str_bytes).map_err(|_| InvalidUtf8)
}

#[cfg(test)]
mod tests {
    // TODO
}

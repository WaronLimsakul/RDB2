use EngineErr::*;
use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs,
    io::{self, BufReader, Read, Write},
    path::PathBuf,
};

const TABLE_MAGIC_NUMBER: [u8; 8] =
    [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
const TABLE_FILE_EXTENSION: &str = "rdb";

/// page size (basic disk read/write unit) in bytes
const PAGE_SIZE: u32 = 4096;

/// Allowed columns types
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

/// Column types with data
pub enum TypeData {
    Int(i32),
    Uint(u32),
    Long(i64),
    Ulong(u64),
    String(String),
    Bool(bool),
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
    InvalidStrLenght,
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
            InvalidStrLenght => write!(f, "Invalid string length."),
        }
    }
}

impl std::error::Error for EngineErr {}

/// 1 page = 1 b-tree node
struct Page {
    id: u64,
    keys: Vec<u64>,
    vals: Vec<u64>,
    is_leaf: bool,
    is_dirty: bool,
}

/// Represent user-defined row schema in order
/// First one must be ULong (id)
type TableSchema = Vec<(String, Type)>;

/// Represent a file or table
struct Table {
    schema: TableSchema,
    pages: Vec<Option<Page>>, // capable of holding empty page
}

/// Represent lower level engine that deals with disk's files
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

        write_table_header(file, &schema)?;

        self.tables.insert(
            String::from(name),
            Table {
                schema,
                pages: Vec::new(),
            },
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
    pub fn insert_row(
        &mut self,
        table_name: &str,
        data: Vec<(String, TypeData)>,
    ) -> Result<(), EngineErr> {
        // fetch table metadata if not there
        if !self.tables.contains_key(table_name) {
            let file =
                fs::File::open(table_name).map_err(|e| FsErr(Box::new(e)))?;
            self.tables.insert(
                table_name.to_string(),
                Table {
                    schema: read_table_header(file)?,
                    pages: Vec::new(),
                },
            );
        }

        // update

        return Ok(());
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

/// return length of string in u16
fn str_len(s: &String) -> Result<[u8; 2], EngineErr> {
    let data = u16::try_from(s.len()).map_err(|_| InvalidStrLenght)?;
    return Ok(data.to_be_bytes());
}

/// setup table header to a new file
/// see format in [adr file](../docs/adr/01-file-table-schema-format.md)
fn write_table_header<W: io::Write>(
    mut writer: W,
    schema: &TableSchema,
) -> Result<(), EngineErr> {
    // build header: starts with the magic number
    let mut bytes = Vec::from(TABLE_MAGIC_NUMBER);

    // how many columns
    bytes.extend_from_slice(&schema.len().to_be_bytes());

    // column data
    for (col_name, t) in schema.iter() {
        bytes.extend_from_slice(&str_len(col_name)?);
        bytes.extend_from_slice(&col_name.as_bytes());
        bytes.push(t.to_byte());
    }

    // write ts out
    match writer.write(&bytes) {
        Err(err) => Err(FsErr(Box::new(err))),
        _ => Ok(()),
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

    let num_cols = read_u64(&mut br)?;
    let mut schema: TableSchema = Vec::with_capacity(num_cols);

    for _ in 0..num_cols {
        let col_name = read_string(&mut br)?;

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
fn read_u64<R: Read>(reader: &mut R) -> Result<usize, EngineErr> {
    let mut buf = [0u8; 8];
    reader
        .read_exact(&mut buf)
        .map_err(|e| FsErr(Box::new(e)))?;
    Ok(usize::from_be_bytes(buf))
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

    #[test]
    fn test_read_write_header() {
        let schema: TableSchema = vec![
            ("id".to_string(), Type::Ulong),
            ("name".to_string(), Type::String),
            ("age".to_string(), Type::Uint),
            ("retired".to_string(), Type::Bool),
        ];

        // test writing normal header
        let mut buf: Vec<u8> = Vec::new();
        write_table_header(&mut buf, &schema).expect("Test write failed");

        // test reading the header
        let res = read_table_header(buf.as_slice()).expect("Test read failed");
        assert_eq!(res, schema);
    }
}

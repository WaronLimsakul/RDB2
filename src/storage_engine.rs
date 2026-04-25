use EngineErr::*;
use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs,
    io::{self, BufReader, Read, Write},
    path::PathBuf,
};

const TABLE_MAGIC_NUMBER: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
const TABLE_FILE_EXTENSION: &str = "rdb";

const PAGE_SIZE: usize = 4096;
const PAGE_MAGIC_NUMBER: [u8; 4] = [0x50, 0x41, 0x47, 0x45]; // "PAGE"

/// Allowed columns types
// NOTE: change this -> change all impls, and ColData
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

impl From<KeyType> for Type {
    fn from(value: KeyType) -> Self {
        match value {
            KeyType::Uint => Type::Uint,
            KeyType::Ulong => Type::Ulong,
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

/// Types that are allowed for key
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum KeyType {
    Uint,
    Ulong,
}

impl TryFrom<Type> for KeyType {
    type Error = EngineErr;
    fn try_from(value: Type) -> Result<Self, Self::Error> {
        match value {
            Type::Ulong => Ok(KeyType::Ulong),
            Type::Uint => Ok(KeyType::Uint),
            other => Err(UnSupportedType(other)),
        }
    }
}

/// Column types with data
pub enum ColData {
    Int(i32),
    Uint(u32),
    Long(i64),
    Ulong(u64),
    String(String),
    Bool(bool),
}

/// Key type with data
// NOTE: derived impl says Uint < Ulong
#[derive(PartialEq, PartialOrd, Debug, Clone, Copy)]
pub enum KeyData {
    Uint(u32),
    Ulong(u64),
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
    InvalidKeySize(u8),
    RowExists(KeyData),
    PageFull,
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
            InvalidKeySize(s) => write!(f, "Found invalid key size {s}."),
            RowExists(k) => write!(f, "Row with key {:?} already exists.", k),
            PageFull => write!(f, "Page full."),
        }
    }
}

impl std::error::Error for EngineErr {}

/// value in cell
enum CellValue {
    Internal(u32),
    Leaf(ColData),
}

/// A cell in page slot
struct Cell<'a> {
    buffer: &'a [u8],
    is_leaf: bool,
}

impl<'a> Cell<'a> {
    fn key_size(&self) -> u8 {
        self.buffer[0]
    }

    fn key(&self) -> Result<KeyData, EngineErr> {
        use KeyData::*;
        let key_size = self.key_size();
        match key_size {
            4 => Ok(Uint(u32::from_be_bytes(
                self.buffer[5..9].try_into().unwrap(),
            ))),
            8 => Ok(Ulong(u64::from_be_bytes(
                self.buffer[5..13].try_into().unwrap(),
            ))),
            s => Err(InvalidKeySize(s)),
        }
    }

    // TODO:
    // fn value(&self, is_leaf: bool) -> CellValue {
    //     if is_leaf {
    //     }
    // }
}

/// 1 page = 1 b-tree node
/// see format in [adr file](../docs/adr/03-new-b-tree-format.md)
struct Page {
    is_dirty: bool,
    buffer: [u8; PAGE_SIZE],
}

impl Page {
    /// each field offset
    const OFF_MAGIC: usize = 0;
    const OFF_ID: usize = 4;
    const OFF_FLAGS: usize = 8;
    const OFF_NUM_CELLS: usize = 9;
    const OFF_FREE_SPACE: usize = 11;
    const OFF_NEXT_NODE: usize = 13;
    const OFF_RIGHTMOST: usize = 17;
    const OFF_PTRS: usize = 21;

    // helper for reading u16
    fn read_u16(&self, pos: usize) -> u16 {
        u16::from_be_bytes(self.buffer[pos..pos + 2].try_into().unwrap())
    }

    // helper for reading u32
    fn read_u32(&self, pos: usize) -> u32 {
        u32::from_be_bytes(self.buffer[pos..pos + 4].try_into().unwrap())
    }

    /// check if page is valid using magic number
    fn is_page(&self) -> bool {
        PAGE_MAGIC_NUMBER == self.buffer[Self::OFF_MAGIC..Self::OFF_MAGIC + 4]
    }

    fn id(&self) -> u32 {
        self.read_u32(Self::OFF_ID)
    }

    fn is_leaf(&self) -> bool {
        (self.buffer[Self::OFF_FLAGS] | 0x01) == 1
    }

    fn is_root(&self) -> bool {
        (self.buffer[Self::OFF_FLAGS] | 0x02) == 1
    }

    fn num_cells(&self) -> u16 {
        self.read_u16(Self::OFF_NUM_CELLS)
    }

    fn free_space(&self) -> u16 {
        self.read_u16(Self::OFF_FREE_SPACE)
    }

    fn next_node_id(&self) -> u32 {
        self.read_u32(Self::OFF_NEXT_NODE)
    }

    /// rightmost value for internal node
    fn rightmost_val(&self) -> u32 {
        self.read_u32(Self::OFF_RIGHTMOST)
    }

    /// get cell from the pointer with target index
    fn cell(&self, index: u16) -> Cell {
        let ptr_offset = Self::OFF_PTRS + (2 * usize::from(index));
        let cell_offset = self.read_u16(ptr_offset) as usize;

        if self.is_leaf() {
            let key_size = self.read_u32(cell_offset) as usize;
            let value_size = self.read_u32(cell_offset + 4) as usize;
            Cell {
                is_leaf: self.is_leaf(),
                buffer: &self.buffer[cell_offset..cell_offset + 8 + key_size + value_size],
            }
        } else {
            let key_size = self.read_u32(cell_offset) as usize;
            Cell {
                is_leaf: self.is_leaf(),
                buffer: &self.buffer[cell_offset..cell_offset + 8 + key_size],
            }
        }
    }

    /// Find cell from provided id:
    /// - internal: return cell that caller should traverse if ask for that ID
    /// - leaf: return kv with that id or where it should be were to insert
    /// TODO: implement and check return type, + test
    fn find_cell_pos(&self, id: KeyData) -> Result<u16, EngineErr> {
        let mut l = 0;
        let mut r = self.num_cells();

        // binary search
        while l < r {
            let m = l + ((r - l) / 2);
            if self.cell(m).key()? >= id {
                r = m;
            } else {
                l = m + 1;
            }
        }

        return Ok(l);
    }

    // /// Performs physical insert cell to page, return `EngineErr::PageFull` if needed
    // /// requires: val obey schema with no id in front, (already at key)
    // ///
    // /// - internal: we have (val, key) as a cell + (right most val)
    // /// - leaf: we have (key, val) as a cell
    // fn insert_cell(&self, key: KeyData, val: CellValue) -> Result<(), EngineErr> {
    //     // TODO: free space system
    //     let target_pos = self.find_cell_pos(key)?;
    //     if self.cell(target_pos).key()? == key {
    //         return Err(RowExists(key));
    //     }
    //
    //     return Ok(false);
    // }
}

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
    // 1. write the file header (error if file already exists)
    // 2. save table in tables cache
    pub fn new_table(&mut self, name: &str, schema: TableSchema) -> Result<(), EngineErr> {
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
        data: Vec<(String, ColData)>,
    ) -> Result<(), EngineErr> {
        // fetch table metadata if not there
        if !self.tables.contains_key(table_name) {
            let file = fs::File::open(table_name).map_err(|e| FsErr(Box::new(e)))?;
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
fn write_table_header<W: io::Write>(mut writer: W, schema: &TableSchema) -> Result<(), EngineErr> {
    // build header: starts with the magic number
    let mut bytes = Vec::from(TABLE_MAGIC_NUMBER);

    // how many columns
    bytes.extend(schema.num_cols().to_be_bytes());

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
        let schema = TableSchema {
            key: ("id".to_string(), KeyType::Ulong),
            vals: vec![
                ("name".to_string(), Type::String),
                ("age".to_string(), Type::Uint),
                ("retired".to_string(), Type::Bool),
            ],
        };

        // test writing normal header
        let mut buf: Vec<u8> = Vec::new();
        write_table_header(&mut buf, &schema).expect("Test write failed");

        // test reading the header
        let res = read_table_header(buf.as_slice()).expect("Test read failed");
        assert_eq!(res, schema);
    }
}

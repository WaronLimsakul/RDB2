use EngineErr::*;
use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    fs,
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

const TABLE_MAGIC_NUMBER: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
const TABLE_FILE_EXTENSION: &str = "rdb";

const PAGE_SIZE: usize = 4096;
const PAGE_MAGIC_NUMBER: [u8; 4] = [0x50, 0x41, 0x47, 0x45]; // "PAGE"

/// Allowed columns types
// Change this -> change all impls, and ColData
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Type {
    Int,
    Uint,
    Long,
    Ulong,
    String, // size (u16) + utf-8
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

impl ColData {
    fn size(&self) -> usize {
        use ColData::*;
        match self {
            Int(_) | Uint(_) => 4,
            Long(_) | Ulong(_) => 8,
            String(s) => 2 + s.len(),
            Bool(_) => 1,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        use ColData::*;
        match self {
            Int(d) => d.to_be_bytes().to_vec(),
            Uint(d) => d.to_be_bytes().to_vec(),
            Long(d) => d.to_be_bytes().to_vec(),
            Ulong(d) => d.to_be_bytes().to_vec(),
            String(s) => {
                let len = u16::try_from(s.len()).unwrap();
                let mut buffer = Vec::with_capacity(2 + s.len());
                buffer.extend_from_slice(&len.to_be_bytes());
                buffer.extend_from_slice(&s.as_bytes());
                buffer
            }
            Bool(true) => vec![1u8],
            Bool(false) => vec![0u8],
        }
    }
}

/// Key type with data
// NOTE: derived impl says Uint < Ulong
#[derive(PartialEq, PartialOrd, Debug, Clone, Copy)]
pub enum KeyData {
    Uint(u32),
    Ulong(u64),
}

impl KeyData {
    fn size(&self) -> usize {
        use KeyData::*;
        match self {
            Uint(_) => 4,
            Ulong(_) => 8,
        }
    }

    /// Returns its representation as bytes
    fn to_bytes(&self) -> Vec<u8> {
        use KeyData::*;
        match self {
            Uint(d) => d.to_be_bytes().to_vec(),
            Ulong(d) => d.to_be_bytes().to_vec(),
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

impl CellValue {
    /// Returns its raw size with no metadata
    fn size(&self) -> usize {
        use CellValue::*;
        match self {
            Internal(_) => 4,
            Leaf(c) => c.size(),
        }
    }
}

/// A cell in page slot
struct Cell<'a> {
    buffer: &'a [u8],
    is_leaf: bool,
}

impl<'a> Cell<'a> {
    /// Creates cell from key and value
    // TODO: can have write_to_slice, that just write all this to slice
    // they provided directly
    fn to_bytes(key: KeyData, val: CellValue) -> Vec<u8> {
        use CellValue::*;
        let key_size = u8::try_from(key.size()).unwrap();
        match val {
            Leaf(record) => {
                let val_size = u32::try_from(record.size()).unwrap();
                // 1 for key_size, 4 for val_size
                let mut buffer = Vec::with_capacity(5 + val_size as usize + key_size as usize);
                buffer.push(key_size);
                buffer.extend_from_slice(&val_size.to_be_bytes());
                buffer.extend_from_slice(&key.to_bytes());
                buffer.extend_from_slice(&record.to_bytes());
                return buffer;
            }

            Internal(ptr) => {
                // 1 for key_size, 4 for ptr (u32)
                let mut buffer = Vec::with_capacity(5 + key_size as usize);
                buffer.push(key_size);
                buffer.extend_from_slice(&ptr.to_be_bytes());
                buffer.extend_from_slice(&key.to_bytes());
                return buffer;
            }
        }
    }

    /// Returns its size in bytes
    fn size(&self) -> usize {
        self.buffer.len()
    }

    /// Get key size metadata
    fn key_size(&self) -> u8 {
        self.buffer[0]
    }

    /// Get key data
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
    const OFF_TOTAL_FREE_SPACE: usize = 21;
    const OFF_PTRS: usize = 23;

    /// Read u16 from target pos
    fn read_u16(&self, pos: usize) -> u16 {
        u16::from_be_bytes(self.buffer[pos..pos + 2].try_into().unwrap())
    }
    /// Read u32 from target pos
    fn read_u32(&self, pos: usize) -> u32 {
        u32::from_be_bytes(self.buffer[pos..pos + 4].try_into().unwrap())
    }

    /// Writes src u16 to target pos
    fn write_u16(&mut self, src: u16, pos: usize) {
        self.buffer[pos..pos + 2].copy_from_slice(&src.to_be_bytes());
    }
    /// Writes src u32 to target pos
    fn write_u32(&mut self, src: u32, pos: usize) {
        self.buffer[pos..pos + 4].copy_from_slice(&src.to_be_bytes());
    }

    /// Write cell (key + val + metadata) to target pos
    fn write_cell(&mut self, key: KeyData, val: CellValue, pos: usize) {
        let cell_bytes = Cell::to_bytes(key, val);
        self.buffer[pos..pos + cell_bytes.len()].copy_from_slice(&cell_bytes);
    }

    /// check if page is valid using magic number
    fn is_page(&self) -> bool {
        PAGE_MAGIC_NUMBER == self.buffer[Self::OFF_MAGIC..Self::OFF_MAGIC + 4]
    }

    fn id(&self) -> u32 {
        self.read_u32(Self::OFF_ID)
    }
    fn set_id(&mut self, val: u32) {
        self.write_u32(val, Self::OFF_ID);
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
    fn set_num_cells(&mut self, val: u16) {
        self.write_u16(val, Self::OFF_NUM_CELLS);
    }

    fn free_space_ptr(&self) -> u16 {
        self.read_u16(Self::OFF_FREE_SPACE)
    }
    fn set_space_ptr(&mut self, val: u16) {
        self.write_u16(val, Self::OFF_FREE_SPACE);
    }

    fn total_free_space(&self) -> u16 {
        self.read_u16(Self::OFF_TOTAL_FREE_SPACE)
    }
    fn set_total_free_space(&mut self, val: u16) {
        self.write_u16(val, Self::OFF_TOTAL_FREE_SPACE);
    }

    fn next_node_id(&self) -> u32 {
        self.read_u32(Self::OFF_NEXT_NODE)
    }

    /// rightmost value for internal node
    fn rightmost_val(&self) -> u32 {
        self.read_u32(Self::OFF_RIGHTMOST)
    }

    /// Get cell from the pointer with target index
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

    /// Find pointer position from provided id:
    /// - internal: return cell that caller should traverse if ask for that ID
    /// - leaf: return kv with that id or where it should be were to insert
    fn find_ptr_pos(&self, id: KeyData) -> Result<u16, EngineErr> {
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

    /// Performs physical insert cell to page, return `EngineErr::PageFull` if needed
    /// requires: val obey schema with no id in front, (already at key)
    /// NOTE: For internal node case, don't have to change rightmost at all, we insert at most
    /// to the left of rightmost. However, need a new_internal_node function to create
    /// new internal with rightmost already there.
    fn insert_cell(&mut self, key: KeyData, val: CellValue) -> Result<(), EngineErr> {
        let front_ptr = Self::OFF_PTRS + (self.num_cells() * 2) as usize;
        let back_ptr = self.free_space_ptr() as usize;

        // 2 for a pointer and the rest for cell
        let space_needed = if self.is_leaf() {
            5 + key.size() + val.size() // 1 for key_size, 4 for val_size
        } else {
            1 + key.size() + val.size() // 1 for key_size
        };

        if back_ptr - front_ptr < space_needed {
            // in case we can defragment
            if self.total_free_space() as usize >= space_needed {
                self.defragment()?;
            } else {
                return Err(PageFull);
            }
        }

        // check if row already exists
        let ptr_pos = self.find_ptr_pos(key)?;
        if self.cell(ptr_pos).key()? == key {
            return Err(RowExists(key));
        }

        // shift pointers so we can put new pointer
        let start = ptr_pos as usize;
        let end = front_ptr;
        self.buffer.copy_within(start..end, start + 1);

        // put pointer to target ptr pos
        let cell_pos = u16::try_from(back_ptr - space_needed).unwrap();
        self.write_u16(cell_pos, ptr_pos as usize);
        // write cell to target target cell pos
        self.write_cell(key, val, cell_pos as usize);

        // update metadata
        self.set_num_cells(self.num_cells() + 1);
        self.set_space_ptr(cell_pos);
        self.set_total_free_space(self.total_free_space() - space_needed as u16);

        return Ok(());
    }

    /// Compacts the physical layout (get rid of useless gap)
    // TODO:
    fn defragment(&mut self) -> Result<(), EngineErr> {
        return Ok(());
    }

    // TODO: delete and update
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

/// Trait for a anything pager can deal with
/// (Should be file or mock file)
trait TableSrc: Read + Write + Seek {}

// A trick for making everything that have Read + Write + Seek be a TableSrc
// They call it blanket implementation.
impl<T: Read + Write + Seek> TableSrc for T {}

// TODO: evict policy
struct Pager {
    cache: HashMap<u32, Page>,
    src: Box<dyn TableSrc>,
    num_pages: u32,     // current number of pages in the file (not cache)
    header_size: usize, // file's header size
}

impl Pager {
    /// Creates new Pager with reader
    fn new(src: Box<dyn TableSrc>, num_pages: u32, header_size: usize) -> Pager {
        Pager {
            cache: HashMap::new(),
            src,
            num_pages,
            header_size,
        }
    }

    /// Get immutable page with target page id
    fn page(&mut self, id: u32) -> Option<&Page> {
        // cache miss
        if !self.cache.contains_key(&id) {
            // Page doesn't exists
            if id >= self.num_pages {
                return None;
            }
            // Page exists but not on cache
            // read from src
            let mut buffer = [0u8; PAGE_SIZE];
            let offset = self.header_size + (id as usize * PAGE_SIZE);
            self.src.seek(SeekFrom::Start(offset as u64)).ok()?;
            self.src.read_exact(&mut buffer).ok()?;
            // save to cache
            self.cache.insert(
                id,
                Page {
                    is_dirty: false,
                    buffer,
                },
            );
        }

        self.cache.get(&id)
    }

    /// Get mutable page with target page id
    fn page_mut(&mut self, id: u32) -> Option<&mut Page> {
        // cache miss
        if !self.cache.contains_key(&id) {
            // Page doesn't exists
            if id >= self.num_pages {
                return None;
            }
            // Page exists but not on cache
            // read from src
            let mut buffer = [0u8; PAGE_SIZE];
            let offset = self.header_size + (id as usize * PAGE_SIZE);
            self.src.seek(SeekFrom::Start(offset as u64)).ok()?;
            self.src.read_exact(&mut buffer).ok()?;
            // save to cache
            self.cache.insert(
                id,
                Page {
                    is_dirty: false,
                    buffer,
                },
            );
        }

        self.cache.get_mut(&id)
    }

    /// Allocates new page with header, ready-to-use
    fn new_page(&mut self) -> &Page {
        // create new page with new id
        let mut page = Page {
            is_dirty: false,
            buffer: [0u8; PAGE_SIZE],
        };
        let new_id = self.num_pages;
        page.set_id(new_id);

        // add to cache
        self.cache.insert(new_id, page);

        // update num_pages
        self.num_pages += 1;
        // return new page
        return self.cache.get(&new_id).unwrap();
    }

    /// Allocates new page with header and return mutable one
    fn new_page_mut(&mut self) -> &mut Page {
        // create new page with new id
        let mut page = Page {
            is_dirty: false,
            buffer: [0u8; PAGE_SIZE],
        };
        let new_id = self.num_pages;
        page.set_id(new_id);

        // add to cache
        self.cache.insert(new_id, page);

        // update num_pages
        self.num_pages += 1;
        // return new page
        return self.cache.get_mut(&(self.num_pages - 1)).unwrap();
    }

    /// Flush page by id, only flush if diry
    fn flush_by_id() {}
    /// Flush all dirty pages in pager
    fn flush() {}
}

/// Represents a row with data (not just schema)
type RowData = (KeyData, Vec<ColData>);

/// Represent a file or table
/// change metadata: change new, try_from_src, read_header
struct Table {
    schema: TableSchema,
    pager: Pager,
    root_id: u32, // ID of the root node
}

impl Table {
    /// Create new table from user provided data
    fn new(
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
    fn try_from_src<T: TableSrc + 'static>(reader: T) -> Result<Self, EngineErr> {
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
    /// see format in [adr file](../docs/adr/06-root-id-and-page-count-in-file-header.md)
    /// return number of bytes written
    fn write_header<W: Write>(mut writer: W, schema: &TableSchema) -> Result<usize, EngineErr> {
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
}

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

/// return length of string in u16
fn str_len(s: &String) -> Result<[u8; 2], EngineErr> {
    let data = u16::try_from(s.len()).map_err(|_| InvalidStrLenght)?;
    return Ok(data.to_be_bytes());
}

/// read table metadata from reader
// It should
// 1. check the magic number
// 2. parse all the column name and type into table schema
// 3. parse and return num_pages
fn read_table_header<R: io::Read>(reader: R) -> Result<(TableSchema, u32), EngineErr> {
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

    let num_pages = read_u32(&mut br)?;

    return Ok((schema, num_pages));
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

    // TODO: test page level function
    // - insert cell
    // - read cell
    // - find ptr pos
    // - defragment
}

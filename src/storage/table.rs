//! # File format (`.rdb` file)
//!
//! Header followed by pages (each `PAGE_SIZE` = 4096 bytes).
//!
//! ## Header
//!
//! | Offset | Size | Description |
//! |--------|------|-------------|
//! | 0      | 8    | Magic number `TABLE_MAGIC_NUMBER` = `[0x01,0x23,0x45,0x67,0x89,0xab,0xcd,0xef]` |
//! | 8      | 4    | Page count (`u32`) |
//! | 12     | 4    | Root node ID (`u32`) |
//! | 16     | 8    | Column count (`u64`; first column is always the key) |
//! | 24     | var  | Column entries (see below) |
//!
//! **Column entry** (repeat `num_cols` times):
//!
//! | Offset | Size | Description |
//! |--------|------|-------------|
//! | 0      | 2    | Name length (`u16`) |
//! | 2      | n    | Name (UTF-8, n = name length) |
//! | 2+n    | 1    | Column type `u8`: 0=Int, 1=Uint, 2=Long, 3=Ulong, 4=String, 5=Bool |
//!
//! ## Column value encoding (`ColData::to_bytes`)
//!
//! | Type | Encoding |
//! |------|----------|
//! | Int  | 4 bytes big-endian `i32` |
//! | Uint | 4 bytes big-endian `u32` |
//! | Long | 8 bytes big-endian `i64` |
//! | Ulong| 8 bytes big-endian `u64` |
//! | String | 2 bytes length (`u16`) + UTF-8 bytes |
//! | Bool | 1 byte (0 or 1) |
//!
//! NOTE: if change the format, please change
//! 1. Table::try_from_src
//! 2. Table::write_header

use std::io::{BufReader, Read, SeekFrom, Write};

use crate::storage::{
    EngineErr::{self, *},
    KeyData, KeyType, MAX_RECORD_SIZE, PAGE_SIZE, RecData, RowData, TABLE_MAGIC_NUMBER, Type,
    cell::CellValue,
    node::Page,
    pager::{Pager, TableSrc},
    row_cursor::RowCursor,
};

/// Represent user-defined row schema in order
#[derive(Debug, PartialEq)]
pub struct TableSchema {
    pub key: (String, KeyType), // type for id
    pub vals: Vec<(String, Type)>,
}

impl TableSchema {
    /// New empty table schema
    pub fn new() -> Self {
        TableSchema {
            key: ("".to_string(), KeyType::Uint),
            vals: Vec::new(),
        }
    }

    /// Set new key name and type
    pub fn set_key(&mut self, name: String, key_type: KeyType) {
        self.key = (name, key_type);
    }

    /// Add value type
    pub fn add_val_type(&mut self, name: String, col_type: Type) {
        self.vals.push((name, col_type));
    }

    /// Decoder raw bytes to RecData using the schema
    pub fn decode_val(&self, bytes: &[u8]) -> Result<RecData, EngineErr> {
        let mut vals = Vec::with_capacity(self.vals.len());
        let mut offset = 0;

        for (_, col_type) in &self.vals {
            let (res, n_bytes_read) = col_type.decode(&bytes[offset..])?;
            vals.push(res);
            offset += n_bytes_read;
        }
        Ok(RecData { vals })
    }

    /// Get column type from column name
    pub fn get_type(&self, col: &str) -> Option<Type> {
        if col == self.key.0 {
            return Some(self.key.1.into());
        }

        for (c, t) in self.vals.iter() {
            if c == col {
                return Some(t.clone());
            }
        }
        None
    }

    pub fn num_cols(&self) -> usize {
        1 + self.vals.len()
    }
}

/// All table header offsets
const OFF_TABLE_MAGIC_NUMBER: usize = 0;
const OFF_TABLE_NUM_PAGES: usize = 8;
const OFF_TABLE_ROOT_NODE_ID: usize = 12;
const OFF_TABLE_NUM_COLUMNS: usize = 16;
const OFF_TABLE_COLUMN_ENTRIES: usize = 24;

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

    /// Return table's schema
    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }

    /// Create a table by reading metadata from the reader
    // It should
    // 1. Read and check the magic number
    // 2. Read the num pages
    // 3. Read root node id
    // 4. Parse the table schema
    // 5. Get the header size from stream pos
    // 6. Return table
    pub fn try_from_src<T: TableSrc + 'static>(reader: T) -> Result<Self, EngineErr> {
        let mut br = BufReader::new(reader);

        // check magic number
        let mut magic = [0u8; 8];
        br.read_exact(magic.as_mut_slice())
            .map_err(|e| FsErr(Box::new(e)))?;
        if magic != TABLE_MAGIC_NUMBER {
            return Err(InvalidMagicNumber);
        }

        // read num pages
        let num_pages = read_u32(&mut br)?;
        // read root id
        let root_id = read_u32(&mut br)?;

        // read num columns
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

        use std::io::Seek;
        // done with the header reading, now check header size
        let header_size = br.stream_position().map_err(|e| FsErr(Box::new(e)))?;
        let src = br.into_inner(); // get TableSrc out

        // let pager use underline reader instead
        Ok(Table {
            schema,
            root_id,
            pager: Pager::new(Box::new(src), num_pages, header_size as usize),
        })
    }

    /// Write a table header to a writer. num_pages set to 0.
    /// see format in [adr file](../../docs/adr/06-root-id-and-page-count-in-file-header.md)
    /// return number of bytes written
    pub fn write_header<W: Write>(mut writer: W, schema: &TableSchema) -> Result<usize, EngineErr> {
        // build header: starts with the magic number
        let mut bytes = Vec::from(TABLE_MAGIC_NUMBER);

        // write 2 0's in u32
        // 1. first 0 = number of page in u32
        // 2. second 0 = initial root id in u32
        bytes.extend_from_slice(&0u64.to_be_bytes());

        // how many columns
        bytes.extend_from_slice(&schema.num_cols().to_be_bytes());

        // key column data
        bytes.extend_from_slice(&str_len(&schema.key.0)?);
        bytes.extend_from_slice(schema.key.0.as_bytes());
        bytes.push(Type::from(schema.key.1).to_byte());

        // value column data
        for (col_name, t) in schema.vals.iter() {
            bytes.extend_from_slice(&str_len(col_name)?);
            bytes.extend_from_slice(col_name.as_bytes());
            bytes.push(t.to_byte());
        }

        // write ts out
        writer.write(&bytes).map_err(|e| FsErr(Box::new(e)))
    }

    /// Inserts row to the table
    // 1. find page to insert (traverse tree)
    // 2. insert
    // 3. if page full, split
    // requires: for now, not allow row size to exceed MAX_RECORD_SIZE
    pub fn insert_row(&mut self, data: RowData) -> Result<(), EngineErr> {
        // TODO(overflow): implement overflow page
        assert!(data.size() <= MAX_RECORD_SIZE);

        // insert a page if table empty
        if self.pager.num_pages() == 0 {
            let root = self.pager.new_page(true, true);
            self.root_id = root.id();
        }

        // traverse the tree
        let key = data.key;
        let path = self.traverse_to_key(key)?;
        let cur_page = self.pager.page_mut(*path.last().unwrap()).unwrap();

        let res = cur_page.insert_cell(key, CellValue::Leaf(&data.vals.to_bytes()));
        match res {
            Ok(_) => Ok(()),
            Err(PageFull) => {
                self.split_insert_leaf(path, data)?;
                Ok(())
            }
            e => e,
        }
    }

    /// Return cursor that iterator over all rows
    pub fn get_all_rows(&mut self) -> RowCursor<'_> {
        self.new_row_cursor()
    }

    /// Return the row cursor that points to the row with target key
    /// NOTE: if call .next(), will still return next row.
    pub fn find_row_by_key(&mut self, key: KeyData) -> RowCursor<'_> {
        if self.pager.num_pages() == 0 {
            // Just empty rowcursor
            return self.new_row_cursor();
        }

        let path = self.traverse_to_key(key).unwrap();
        let (page_id, cell_idx) = {
            let page = self.pager.page(*path.last().unwrap()).unwrap();
            (page.id(), page.search_cell_leaf(key))
        };
        self.new_row_cursor().set(page_id, cell_idx)
    }

    /// Flush all the change that happen to table to disk
    pub fn flush(&mut self) -> Result<(), EngineErr> {
        // TODO(minor): may consider having num_pages_changed and root_node_changed fields
        let num_pages = self.pager.num_pages();
        let root_node_id = self.root_id;
        let table_src = &mut self.pager.src;

        table_src
            .seek(SeekFrom::Start(OFF_TABLE_NUM_PAGES as u64))
            .map_err(|e| FsErr(Box::new(e)))?;
        table_src
            .write(&num_pages.to_be_bytes())
            .map_err(|e| FsErr(Box::new(e)))?;

        // NOTE: Uncomment if root node id does not come after this num_pages
        // table_src
        //     .seek(SeekFrom::Start(OFF_TABLE_ROOT_NODE_ID as u64))
        //     .map_err(|e| FsErr(Box::new(e)))?;

        table_src
            .write(&root_node_id.to_be_bytes())
            .map_err(|e| FsErr(Box::new(e)))?;

        self.pager.flush()
    }

    /// Traverse from root to child that contain keydata, or where it should be if inserted.
    /// Return the traversal path if possible.
    fn traverse_to_key(&mut self, key: KeyData) -> Result<Vec<u32>, EngineErr> {
        // starts from root
        let root_id = self.root_id;
        let mut cur_page = self.pager.page(root_id).ok_or(PageNotExists(root_id))?;
        let mut path = vec![root_id]; // traverse path

        // traverse the tree
        while !cur_page.is_leaf() {
            let child_id = cur_page.internal_get_child_id(key);
            cur_page = self.pager.page(child_id).ok_or(PageNotExists(child_id))?; // go to child node
            path.push(child_id); // update traverse path
        }

        Ok(path)
    }

    /// Splits the last node in traverse path and then insert
    /// data record to this layer and parent appropriately. Split parent if need so.
    /// requires: the last node in traverse path must be a leaf
    ///
    /// Implementation:
    /// 1. Allocating new node
    /// 2. Transfer half elements of old node to it
    /// 3. Add first key and pointer (of new node) to parent
    fn split_insert_leaf(&mut self, path: Vec<u32>, data: RowData) -> Result<(), EngineErr> {
        let target_id = *path.last().unwrap();
        debug_assert!(self.pager.page(target_id).unwrap().is_leaf());
        let (new_page_id, new_page_first_key) = self.split_page(target_id, true)?;
        let old_page_last_key = self
            .pager
            .page(target_id)
            .unwrap()
            .last_cell()
            .unwrap()
            .key();

        if data.key == new_page_first_key || data.key == old_page_last_key {
            return Err(RowExists(data.key));
        }

        // Decide which node to insert to
        // Put to right node iff it's more than leftmost row of right node
        if data.key < new_page_first_key {
            let old_page = self.pager.page_mut(target_id).unwrap();
            old_page
                .insert_cell(data.key, CellValue::Leaf(&data.vals.to_bytes()))
                .unwrap(); // Shouldn't be page full
        } else {
            let new_page = self.pager.page_mut(new_page_id).unwrap();
            new_page
                .insert_cell(data.key, CellValue::Leaf(&data.vals.to_bytes()))
                .unwrap(); // Shouldn't be page full
        }
        self.insert_to_parent(path, new_page_first_key, CellValue::Internal(new_page_id))
    }

    /// Splits the last node in traverse path and then insert
    /// data to this layer and parent appropriately. Split parent if need so.
    /// requires: the last node in traverse path must be internal node
    ///
    /// Implementation
    /// 1. Allocating new node
    /// 2. Transfer half elements of old node to it
    /// 3. Move first key of the new node to parent instead.
    fn split_insert_internal(
        &mut self,
        path: Vec<u32>,
        key: KeyData,
        val: CellValue, // Must be CellValue::Internal
    ) -> Result<(), EngineErr> {
        debug_assert!(
            matches!(val, CellValue::Internal(_)),
            "split_insert_internal() called with Leaf CellValue"
        );
        let target_id = *path.last().unwrap();
        let (new_page_id, new_page_leftmost_key) = self.split_page(target_id, false)?;
        let new_page = self.pager.page_mut(new_page_id).unwrap();

        // If key to insert is not less than leftmost of new page
        // then we just insert to new page normally.
        if key >= new_page_leftmost_key {
            new_page.insert_cell(key, val).unwrap(); // shouldn't be page full right?

            self.insert_to_parent(
                path,
                new_page_leftmost_key,
                CellValue::Internal(new_page_id),
            )
        } else {
            // if key < new_page_leftmost_key. Then the key, val we want to insert
            // is the leftmost child. Must be treated specially.
            let new_page_lefmost_child_id = new_page.leftmost_child();
            // set target val to leftmost
            if let CellValue::Internal(val_u32) = val {
                new_page.set_leftmost_child(val_u32);
            }
            // the thing we think is leftmost is not leftmost anymore
            // so insert it back normally.
            new_page
                .insert_cell(
                    new_page_leftmost_key,
                    CellValue::Internal(new_page_lefmost_child_id),
                )
                .unwrap(); // shouldn't be page full

            self.insert_to_parent(path, key, CellValue::Internal(new_page_id))
        }
    }

    /// Splits a page with given id and return (new page id, its left most key)
    /// NOTE:
    /// 1. split the page equally and bias to left page (left page got more cells)
    /// 2. have to return leftmost key here because in case it's internal, we won't have the left
    ///    most key data integrated in there
    fn split_page(&mut self, id: u32, is_leaf: bool) -> Result<(u32, KeyData), EngineErr> {
        // Take the left page out of cache so we can modify it easily
        let mut left = self.pager.take_page(id).ok_or(PageNotExists(id))?;
        let mut right = Page::new(
            true, 0, /*place holder id*/
            is_leaf, false, /*is_root never true*/
        );

        right.set_next_node_id(left.next_node_id());
        // cell index to start copying to right node
        let mut start_right = (left.num_cells() + 1) / 2;
        // will be returned
        let right_node_leftmost_key = left.cell(start_right).key();

        // In case its internal, the first cell of right node will be in the left most child header
        if !is_leaf && let CellValue::Internal(child_id) = left.cell(start_right).value() {
            right.set_leftmost_child(child_id);
            start_right += 1; // don't have to transfer this cell anymore
        }

        let mut space_transferred = 0;
        for i in start_right..left.num_cells() {
            let cell = left.cell(i);
            // know that this won't be page full
            // because it's just half of the target page
            _ = right.insert_cell(cell.key(), cell.value());
            space_transferred += cell.size() + 2; // + 2 because the pointer is also freed
        }

        // Give right page to pager
        let right_id = self.pager.reg_page(right);

        // Update left node metadata
        left.set_next_node_id(right_id);
        left.set_num_cells((left.num_cells() + 1) / 2);
        left.set_total_free_space(left.total_free_space() + space_transferred as u16);
        // return left page back
        self.pager.reg_page_with_id(left, left.id());

        Ok((right_id, right_node_leftmost_key))
    }

    /// Helper for split_insert_leaf and split_insert_internal
    /// Insert key and node id to parent (the second last node in path)
    /// Create a new root if no parent found
    fn insert_to_parent(
        &mut self,
        mut path: Vec<u32>,
        key: KeyData,
        val: CellValue,
    ) -> Result<(), EngineErr> {
        debug_assert!(
            !path.is_empty(),
            "insert_to_parent() called with empty path"
        );

        let child_id = path.pop().unwrap();

        if path.is_empty() {
            debug_assert!(
                self.pager.page(child_id).unwrap().is_root(),
                "insert_to_parent(): first node in traverse path is not parent"
            );
            // unroot the old root
            self.pager.page_mut(child_id).unwrap().set_is_root(false);
            // create a new internal node root
            let root = self.pager.new_page_mut(false, true);
            // new root's leftmost child is the old root
            root.set_leftmost_child(child_id);
            // set root
            self.root_id = root.id();
            // insert
            root.insert_cell(key, val)
        } else {
            let parent_id = *path.last().unwrap();
            let parent = self.pager.page_mut(parent_id).unwrap();
            let val_backup = val.clone();
            match parent.insert_cell(key, val) {
                Err(EngineErr::PageFull) => self.split_insert_internal(path, key, val_backup),
                Err(err) => Err(err),
                _ => Ok(()),
            }
        }
    }

    /// Helper to create new row cursor start from first cell of the table
    fn new_row_cursor(&mut self) -> RowCursor {
        RowCursor::new(&mut self.pager, self.root_id, &self.schema)
    }
}

/// return length of string in u16
fn str_len(s: &str) -> Result<[u8; 2], EngineErr> {
    let data = u16::try_from(s.len()).map_err(|_| InvalidStrLenght)?;
    Ok(data.to_be_bytes())
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

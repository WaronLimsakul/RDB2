//! # Cell format (one entry in a page slot)
//!
//! Cell pointer in page header points here (offset from page start).
//!
//! ## Leaf cell
//!
//! | Offset | Size | Description |
//! |--------|------|-------------|
//! | 0      | 1    | key_size (`u8`, must be 4 or 8) |
//! | 1      | 4    | val_size (`u32`, record length in bytes) |
//! | 5      | ks   | Key bytes (`KeyData::to_bytes`, ks = key_size) |
//! | 5+ks   | vs   | Record bytes (`ColData::to_bytes`, vs = val_size) |
//!
//! ## Internal cell
//!
//! | Offset | Size | Description |
//! |--------|------|-------------|
//! | 0      | 1    | key_size (`u8`, must be 4 or 8) |
//! | 1      | 4    | child_ptr (`u32`, node ID to traverse to) |
//! | 5      | ks   | Key bytes (`KeyData::to_bytes`, ks = key_size) |
//!
//! NOTE: if change key_size and val_size: please change
//! 1. CellValue impl
//! 2. Cell impl
//! 3. Page::cell_no_check()
//! 4. Page::insert_cell()

use crate::storage::KeyData;

#[derive(Clone)]
/// value in cell
pub enum CellValue<'a> {
    Internal(u32),
    Leaf(&'a [u8]),
}

impl<'a> CellValue<'a> {
    /// Returns its raw size with no metadata
    pub fn size(&self) -> usize {
        use CellValue::*;
        match self {
            Internal(_) => 4,
            Leaf(c) => c.len(),
        }
    }
}

/// A cell in page slot
pub struct Cell<'a> {
    buffer: &'a [u8],
    is_leaf: bool,
}

impl<'a> Cell<'a> {
    /// create new Cell with provided value directly
    pub fn new(buffer: &'a [u8], is_leaf: bool) -> Self {
        Self { buffer, is_leaf }
    }

    /// Creates cell from key and value
    // TODO: can have write_to_slice, that just write all this to slice
    // they provided directly
    pub fn to_bytes(key: KeyData, val: CellValue) -> Vec<u8> {
        use CellValue::*;
        let key_size = u8::try_from(key.size()).unwrap();
        match val {
            Leaf(record_bytes) => {
                let val_size = u32::try_from(record_bytes.len()).unwrap();
                // 1 for key_size, 4 for val_size
                let mut buffer = Vec::with_capacity(5 + val_size as usize + key_size as usize);
                buffer.push(key_size);
                buffer.extend_from_slice(&val_size.to_be_bytes());
                buffer.extend_from_slice(&key.to_bytes());
                buffer.extend_from_slice(record_bytes);
                buffer
            }

            Internal(ptr) => {
                // 1 for key_size, 4 for ptr (u32)
                let mut buffer = Vec::with_capacity(5 + key_size as usize);
                buffer.push(key_size);
                buffer.extend_from_slice(&ptr.to_be_bytes());
                buffer.extend_from_slice(&key.to_bytes());
                buffer
            }
        }
    }

    // Get the new copy bytes representation of the cell
    pub fn get_buffer(&self) -> Vec<u8> {
        let mut res: Vec<u8> = Vec::new();
        res.extend_from_slice(self.buffer);
        res
    }

    /// Returns value according to node type it is in
    /// - Internal: id of node you can traverse
    /// - Leaf: record data in BYTES (cell doesn't know the schema so it's
    /// caller responsibility to interpret these bytes)
    pub fn value(&self) -> CellValue<'a> {
        if self.is_leaf {
            let record_bytes = &self.buffer[5 + self.key_size() as usize..];
            CellValue::Leaf(record_bytes)
        } else {
            // the "value" in the internal node entry is just child ptr
            let child_ptr = u32::from_be_bytes(self.buffer[1..5].try_into().unwrap());
            CellValue::Internal(child_ptr)
        }
    }

    /// Return own bytes of value in cell
    pub fn val_bytes(&self) -> Vec<u8> {
        match self.value() {
            CellValue::Internal(child_ptr) => child_ptr.to_be_bytes().to_vec(),
            CellValue::Leaf(bytes) => bytes.to_vec(),
        }
    }

    /// Returns its size in bytes
    pub fn size(&self) -> usize {
        self.buffer.len()
    }

    /// Get key size metadata
    fn key_size(&self) -> u8 {
        let size = self.buffer[0];
        debug_assert!(size == 4 || size == 8, "Cell key size not 4 or 8");
        size
    }

    /// Get key data
    pub fn key(&self) -> KeyData {
        use super::KeyData::*;
        let key_size = self.key_size();
        match key_size {
            4 => Uint(u32::from_be_bytes(self.buffer[5..9].try_into().unwrap())),
            // this must be 8
            _ => Ulong(u64::from_be_bytes(self.buffer[5..13].try_into().unwrap())),
        }
    }

    /// Return 'value' size of the cell according to node type it's in
    /// - Internal: always 4 bytes cuz it's just node id
    /// - Leaf: see [arch](../../docs/adr/06-root-id-and-page-count-in-file-header.md)
    fn value_size(&self) -> u32 {
        if self.is_leaf {
            u32::from_be_bytes(self.buffer[2..6].try_into().unwrap())
        } else {
            4
        }
    }
}

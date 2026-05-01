use crate::storage::{ColData, EngineErr, KeyData};

/// value in cell
pub enum CellValue {
    Internal(u32),
    Leaf(ColData),
}

impl CellValue {
    /// Returns its raw size with no metadata
    pub fn size(&self) -> usize {
        use CellValue::*;
        match self {
            Internal(_) => 4,
            Leaf(c) => c.size(),
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

    // TODO NOW!:
    // return value according to node type it's in
    /// Returns value according to node type it is in
    /// - Internal: id of node you can traverse
    /// - Leaf: record data
    pub fn value(&self) -> CellValue {}
}

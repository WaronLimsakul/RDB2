use crate::storage::{ColData, EngineErr, KeyData, PAGE_MAGIC_NUMBER, PAGE_SIZE};

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
        use super::KeyData::*;
        let key_size = self.key_size();
        match key_size {
            4 => Ok(Uint(u32::from_be_bytes(
                self.buffer[5..9].try_into().unwrap(),
            ))),
            8 => Ok(Ulong(u64::from_be_bytes(
                self.buffer[5..13].try_into().unwrap(),
            ))),
            s => Err(EngineErr::InvalidKeySize(s)),
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
pub struct Page {
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

    pub fn new(is_dirty: bool, buffer: [u8; PAGE_SIZE]) -> Self {
        Page { is_dirty, buffer }
    }

    /// returns the bytes representation
    pub fn bytes(&self) -> &[u8] {
        &self.buffer
    }
    /// returns if the page is dirty
    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }
    /// set page to be dirty
    fn make_dirty(&mut self) {
        self.is_dirty = true;
    }

    /// Read u16 from target pos
    fn read_u16(&self, pos: usize) -> u16 {
        u16::from_be_bytes(self.buffer[pos..pos + 2].try_into().unwrap())
    }
    /// Read u32 from target pos
    fn read_u32(&self, pos: usize) -> u32 {
        u32::from_be_bytes(self.buffer[pos..pos + 4].try_into().unwrap())
    }

    /// Writes src u16 to target pos and set page to be dirty
    fn write_u16(&mut self, src: u16, pos: usize) {
        self.buffer[pos..pos + 2].copy_from_slice(&src.to_be_bytes());
        self.make_dirty();
    }
    /// Writes src u32 to target pos and set page to be dirty
    fn write_u32(&mut self, src: u32, pos: usize) {
        self.buffer[pos..pos + 4].copy_from_slice(&src.to_be_bytes());
        self.make_dirty();
    }
    /// Write cell (key + val + metadata) to target pos and set page to be dirty
    fn write_cell(&mut self, key: KeyData, val: CellValue, pos: usize) {
        let cell_bytes = Cell::to_bytes(key, val);
        self.buffer[pos..pos + cell_bytes.len()].copy_from_slice(&cell_bytes);
        self.make_dirty();
    }

    /// write page magic number
    pub fn set_is_page(&mut self) {
        self.write_u32(u32::from_be_bytes(PAGE_MAGIC_NUMBER), Self::OFF_MAGIC);
    }
    /// check if page is valid using magic number
    pub fn is_page(&self) -> bool {
        PAGE_MAGIC_NUMBER == self.buffer[Self::OFF_MAGIC..Self::OFF_MAGIC + 4]
    }

    pub fn id(&self) -> u32 {
        self.read_u32(Self::OFF_ID)
    }
    pub fn set_id(&mut self, val: u32) {
        self.write_u32(val, Self::OFF_ID);
    }

    pub fn is_leaf(&self) -> bool {
        (self.buffer[Self::OFF_FLAGS] | 0x01) == 1
    }

    pub fn is_root(&self) -> bool {
        (self.buffer[Self::OFF_FLAGS] | 0x02) == 1
    }

    pub fn num_cells(&self) -> u16 {
        self.read_u16(Self::OFF_NUM_CELLS)
    }
    pub fn set_num_cells(&mut self, val: u16) {
        self.write_u16(val, Self::OFF_NUM_CELLS);
    }

    pub fn free_space_ptr(&self) -> u16 {
        self.read_u16(Self::OFF_FREE_SPACE)
    }
    pub fn set_space_ptr(&mut self, val: u16) {
        self.write_u16(val, Self::OFF_FREE_SPACE);
    }

    pub fn total_free_space(&self) -> u16 {
        self.read_u16(Self::OFF_TOTAL_FREE_SPACE)
    }
    pub fn set_total_free_space(&mut self, val: u16) {
        self.write_u16(val, Self::OFF_TOTAL_FREE_SPACE);
    }

    pub fn next_node_id(&self) -> u32 {
        self.read_u32(Self::OFF_NEXT_NODE)
    }

    /// rightmost value for internal node
    pub fn rightmost_val(&self) -> u32 {
        self.read_u32(Self::OFF_RIGHTMOST)
    }

    /// Get cell from the pointer with target index
    pub fn cell(&self, index: u16) -> Cell {
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
    pub fn insert_cell(&mut self, key: KeyData, val: CellValue) -> Result<(), EngineErr> {
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
                return Err(EngineErr::PageFull);
            }
        }

        // check if row already exists
        let ptr_pos = self.find_ptr_pos(key)?;
        if self.cell(ptr_pos).key()? == key {
            return Err(EngineErr::RowExists(key));
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

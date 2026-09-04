//! # Page format (4096 bytes = 1 B-tree node)
//!
//! Slotted page: header at front, cells grow from the back.
//!
//! ## Header (23 bytes)
//!
//! | Offset | Size | Description |
//! |--------|------|-------------|
//! | 0      | 4    | Magic number `PAGE_MAGIC_NUMBER` = `[0x50,0x41,0x47,0x45]` |
//! | 4      | 4    | Node ID |
//! | 8      | 1    | Flags: bit 7 = is_leaf, bit 8 = is_root |
//! | 9      | 2    | Number of cells |
//! | 11     | 2    | Free space pointer — offset of first free byte from the back |
//! | 13     | 4    | Next sibling node ID |
//! | 17     | 4    | Leftmost child node ID (internal only; unused for leaf) |
//! | 21     | 2    | Total free space (needed because deletion fragments space) |
//! | 23     | 2n   | Cell pointer array — `n` entries of `u16` byte offsets |
//!
//! Front of free space = `23 + num_cells * 2`.
//! Back of free space = `free_space_ptr`.
//! Space available = `back_ptr - front_ptr`.
//! If `back_ptr - front_ptr < space_needed` and `total_free_space` is enough, defragment.
//! Otherwise page is full.
//!
//! Cells are appended starting from `PAGE_SIZE` backward.
//! See `cell.rs` for cell byte format.
//!

use crate::storage::{
    EngineErr, FOUR_BYTES_ZERO, KeyData, PAGE_HEADER_SIZE, PAGE_MAGIC_NUMBER, PAGE_SIZE,
    cell::{Cell, CellValue},
};

const NULL_NODE_ID: u32 = u32::MAX; // to annotate there is NO node

const LEFTMOST_CHILD_CELL_IDX: u16 = u16::MAX; // to represent the left most val of internal node

/// 1 page = 1 b-tree node
/// see format in [arch](../../docs/adr/06-root-id-and-page-count-in-file-header.md)
#[derive(Clone, Copy)]
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
    const OFF_LEFTMOST_CHILD: usize = 17;
    const OFF_TOTAL_FREE_SPACE: usize = 21;
    const OFF_PTRS: usize = 23;

    /// Create new page from provided metadata
    pub fn new(is_dirty: bool, id: u32, is_leaf: bool, is_root: bool) -> Self {
        let mut page = Page {
            is_dirty,
            buffer: [0u8; PAGE_SIZE],
        };
        page.set_next_node_id(NULL_NODE_ID);
        page.set_is_page(true);
        page.set_id(id);
        page.set_is_leaf(is_leaf);
        page.set_is_root(is_root);
        page.set_space_ptr(PAGE_SIZE as u16);
        page.set_total_free_space((PAGE_SIZE - PAGE_HEADER_SIZE) as u16);
        page
    }

    /// Return Page from provided raw buffer
    pub fn new_from_buffer(is_dirty: bool, buffer: [u8; PAGE_SIZE]) -> Self {
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

    /// Read u8 from target pos
    fn read_u8(&self, pos: usize) -> u8 {
        u8::from_be_bytes(self.buffer[pos..pos + 1].try_into().unwrap())
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

    /// Return pointer (offset) to the cell with target index
    fn cell_ptr(&self, index: u16) -> u16 {
        let ptr_offset = Self::OFF_PTRS + (2 * usize::from(index));
        self.read_u16(ptr_offset)
    }
    /// Set cell ptr (at index idx) value to val
    fn set_cell_ptr(&mut self, idx: u16, val: u16) {
        let ptr_offset = Self::OFF_PTRS + (2 * usize::from(idx));
        self.write_u16(val, ptr_offset);
    }

    /// Set/unset page magic number
    pub fn set_is_page(&mut self, is_page: bool) {
        let data = match is_page {
            true => PAGE_MAGIC_NUMBER,
            false => FOUR_BYTES_ZERO,
        };
        self.write_u32(u32::from_be_bytes(data), Self::OFF_MAGIC);
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
        (self.buffer[Self::OFF_FLAGS] & 0x01) == 1
    }
    pub fn set_is_leaf(&mut self, val: bool) {
        if val {
            self.buffer[Self::OFF_FLAGS] |= 0x01;
        } else {
            self.buffer[Self::OFF_FLAGS] &= 0b11111110;
        }
    }

    /// Do we even need this?
    pub fn is_root(&self) -> bool {
        (self.buffer[Self::OFF_FLAGS] & 0x02) == 2
    }
    pub fn set_is_root(&mut self, val: bool) {
        if val {
            self.buffer[Self::OFF_FLAGS] |= 0x02;
        } else {
            self.buffer[Self::OFF_FLAGS] &= 0b11111101;
        }
    }

    pub fn num_cells(&self) -> u16 {
        self.read_u16(Self::OFF_NUM_CELLS)
    }
    pub fn set_num_cells(&mut self, val: u16) {
        self.write_u16(val, Self::OFF_NUM_CELLS);
    }

    /// Offset to the free space we can put cell
    pub fn free_space_ptr(&self) -> u16 {
        self.read_u16(Self::OFF_FREE_SPACE)
    }
    pub fn set_space_ptr(&mut self, val: u16) {
        self.write_u16(val, Self::OFF_FREE_SPACE);
    }
    /// Reset space ptr back to its initial value (PAGE_SIZE)
    pub fn reset_space_ptr(&mut self) {
        self.set_space_ptr(PAGE_SIZE as u16);
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
    pub fn set_next_node_id(&mut self, val: u32) {
        self.write_u32(val, Self::OFF_NEXT_NODE);
    }

    /// Left most child id for internal node.
    /// Since we think of cell is (key | ptr)
    /// Then the we will need the ptr with no key on the left most position.
    pub fn leftmost_child(&self) -> u32 {
        self.read_u32(Self::OFF_LEFTMOST_CHILD)
    }
    pub fn set_leftmost_child(&mut self, val: u32) {
        self.write_u32(val, Self::OFF_LEFTMOST_CHILD);
    }

    /// Use cell() to return last cell if exists
    pub fn last_cell(&self) -> Option<Cell> {
        let num_cells = self.num_cells();
        if num_cells > 0 {
            Some(self.cell(self.num_cells() - 1))
        } else {
            None
        }
    }

    /// Get cell from the pointer with pointer index.
    /// Pointer index := first cell in the page has index 0, then 1, so on ....
    /// requires:
    /// 1. the index must be valid (< num_cells)
    /// 2. cannot get cell of index LEFTMOST_CHILD_CELL_IDX since it is not
    /// a real cell. Use method .leftmost_child() to get its value instead.
    pub fn cell(&self, index: u16) -> Cell {
        debug_assert!(
            index != LEFTMOST_CHILD_CELL_IDX,
            "cell() called with LEFTMOST_CHILD_CELL_IDX"
        );

        debug_assert!(
            index < self.num_cells(),
            "cell(): Index {index} not in node"
        );

        self.cell_no_check(index)
    }

    /// Try to get cell from the pointer with pointer index.
    /// Pointer index := first cell in the page has index 0, then 1, so on ....
    /// Returns None if index not in the node.
    pub fn try_cell(&self, index: u16) -> Option<Cell> {
        debug_assert!(
            index != LEFTMOST_CHILD_CELL_IDX,
            "try_cell() called with LEFTMOST_CHILD_CELL_IDX"
        );

        let num_cells = self.num_cells();
        if index >= self.num_cells() {
            return None;
        }

        Some(self.cell_no_check(index))
    }

    /// Core logic for cell() with no safely check, so that cell() can have debug_assert
    fn cell_no_check(&self, index: u16) -> Cell {
        let cell_offset = self.cell_ptr(index) as usize;

        if self.is_leaf() {
            let key_size = self.read_u8(cell_offset) as usize;
            let value_size = self.read_u32(cell_offset + 1) as usize;
            Cell::new(
                // see cell.rs doc
                &self.buffer[cell_offset..cell_offset + 5 + key_size + value_size],
                self.is_leaf(),
            )
        } else {
            let key_size = self.read_u8(cell_offset) as usize;
            Cell::new(
                // see cell.rs doc
                &self.buffer[cell_offset..cell_offset + 5 + key_size],
                self.is_leaf(),
            )
        }
    }

    /// Search a ptr index of cell that caller should traverse if ask for that ID
    /// Requires: must be call on internal node
    /// NOTE: if it's the left most node, returns LEFTMOST_CHILD_CELL_IDX
    fn search_cell_internal(&self, key: KeyData) -> u16 {
        debug_assert!(
            !self.is_leaf(),
            "search_cell_internal() called in leaf node"
        );

        let mut l = 0;
        let mut r = self.num_cells();

        // binary search
        while l < r {
            let m = l + ((r - l) / 2);
            if self.cell(m).key() > key {
                r = m;
            } else {
                l = m + 1;
            }
        }

        return if l > 0 {
            l - 1
        } else {
            LEFTMOST_CHILD_CELL_IDX
        };
    }

    /// Find pointer position from provided id. The pointer point to
    /// cell with that id or where it should be were to insert
    fn search_cell_leaf(&self, key: KeyData) -> u16 {
        let mut l = 0;
        let mut r = self.num_cells();

        // binary search
        while l < r {
            let m = l + ((r - l) / 2);
            if self.cell(m).key() >= key {
                r = m;
            } else {
                l = m + 1;
            }
        }

        l
    }

    /// Returns id of the node that we suppose to traverse
    /// in order to find the provided key.
    /// requires: the node must be internal to be called
    pub fn internal_get_child_id(&self, key: KeyData) -> u32 {
        // shouldn't be used with leaf node
        debug_assert!(!self.is_leaf(), "traverse() called with leaf node");

        let ptr = self.search_cell_internal(key);

        // know from debug_assert that it should be Internal
        if ptr == LEFTMOST_CHILD_CELL_IDX {
            return self.leftmost_child();
        } else if let CellValue::Internal(val) = self.cell(ptr).value() {
            return val;
        } else {
            panic!("Shouldn't happen.");
        }
    }

    /// Return raw bytes value of cell that contains target key, if not found None
    /// requires: The node must be leaf node
    pub fn leaf_get_cell_by_key(&self, key: KeyData) -> Option<Vec<u8>> {
        debug_assert!(
            self.is_leaf(),
            "leaf_get_cell_by_key() called by internal node"
        );

        // Find position
        let ptr = self.search_cell_leaf(key);
        let cell = self.try_cell(ptr)?;

        if let CellValue::Leaf(val) = cell.value()
            // it can be just close value
            && cell.key() == key
        {
            Some(val.to_vec())
        } else {
            None
        }
    }

    /// Performs physical insert cell to page, return `EngineErr::PageFull` if needed
    /// requires: val obey schema with no id in front, (already at key)
    /// NOTE: For internal node case, don't have to change rightmost at all, we insert at most
    /// to the left of rightmost. However, need a new_internal_node function to create
    /// new internal with rightmost already there.
    pub fn insert_cell(&mut self, key: KeyData, val: CellValue) -> Result<(), EngineErr> {
        let front_ptr = Self::OFF_PTRS + (self.num_cells() * 2) as usize;
        let mut back_ptr = self.free_space_ptr() as usize;

        // 2 for a pointer and the rest for cell
        let cell_space_needed = if self.is_leaf() {
            5 + key.size() + val.size() // 1 for key_size, 4 for val_size
        } else {
            1 + key.size() + val.size() // 1 for key_size
        };
        let total_space_needed = cell_space_needed + 2; // 2 for ptr

        if back_ptr - front_ptr < total_space_needed {
            // in case we can defragment
            if self.total_free_space() as usize >= total_space_needed {
                self.defragment()?;
                back_ptr = self.free_space_ptr() as usize;
            } else {
                return Err(EngineErr::PageFull);
            }
        }

        // can call search_cell_leaf even if we are in internal node
        // we just want to know a place to insert thing
        let ptr_pos = self.search_cell_leaf(key);
        // in case there is row in place we want to put
        if let Some(cell) = self.try_cell(ptr_pos) {
            // check if row already exists
            if cell.key() == key {
                return Err(EngineErr::RowExists(key));
            }
            // shift pointers so we can put new pointer
            let start = Self::OFF_PTRS + (ptr_pos * 2) as usize;
            let end = front_ptr;
            self.buffer.copy_within(start..end, start + 2);
        }

        // put pointer to target ptr pos
        let cell_pos = u16::try_from(back_ptr - cell_space_needed).unwrap();
        self.set_cell_ptr(ptr_pos, cell_pos);
        // write cell to target target cell pos
        self.write_cell(key, val, cell_pos as usize);

        // update metadata
        self.set_num_cells(self.num_cells() + 1);
        self.set_space_ptr(cell_pos);

        self.set_total_free_space(self.total_free_space() - (total_space_needed as u16));

        Ok(())
    }

    /// Compacts the physical layout (get rid of useless gap)
    // 1. Get all the cells in vector?
    // 2. Copy back to node back-to-back?
    fn defragment(&mut self) -> Result<(), EngineErr> {
        let mut buffer: Vec<Vec<u8>> = Vec::with_capacity(self.num_cells() as usize);

        for i in 0..self.num_cells() {
            buffer.push(self.cell(i).get_buffer());
        }

        self.reset_space_ptr();

        let mut cur_space_ptr = self.free_space_ptr() as usize;
        for i in (0..self.num_cells()).rev() {
            let cell = &buffer[i as usize];
            let new_space_ptr = cur_space_ptr - cell.len();
            self.buffer[new_space_ptr..cur_space_ptr].copy_from_slice(cell);
            self.set_cell_ptr(i, new_space_ptr as u16);
            cur_space_ptr = new_space_ptr;
        }

        self.set_space_ptr(cur_space_ptr as u16);

        Ok(())
    }

    // TODO: delete and update
}

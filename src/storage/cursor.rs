//! # Cursor
//!
//! Interface for traversing the b-tree (internal).
//! Note that it gives you raw bytes. For iterating over RowData, see row_cursor

use crate::storage::{KeyData, cell::CellValue, node::Page, pager::Pager};

pub struct Cursor<'a> {
    pager: &'a mut Pager,
    page_id: u32,           // current page id, will be root id when created
    cell_idx: u16,          // current cell idx
    cur_page: Option<Page>, // own page (will return to pager after done using)
    init: bool,             // for the first next() called, have to find the leaf node
}

impl<'a> Cursor<'a> {
    pub fn new(pager: &'a mut Pager, root_id: u32) -> Cursor<'a> {
        Cursor {
            pager,
            page_id: root_id,
            cell_idx: 0,
            cur_page: None,
            init: false,
        }
    }
}

impl Iterator for Cursor<'_> {
    type Item = (KeyData, Vec<u8>); // Return key and value bytes
    fn next(&mut self) -> Option<Self::Item> {
        // Never init, have to find the first leaf node
        if !self.init {
            let mut page = self.pager.page(self.page_id)?;
            debug_assert!(
                page.is_root(),
                "Cursor::next(): provided initial page is not root"
            ); // requires this to be root
            while !page.is_leaf() {
                if let CellValue::Internal(child_id) = page.cell(0).value() {
                    page = self.pager.page(child_id)?;
                } else {
                    panic!("Cursor::next(): Internal node shouldn't have leaf cell value");
                }
            }
            self.page_id = page.id();
            self.init = true;
        }

        // Have no page = just got to this new page
        if self.cur_page.is_none() {
            // take first page
            self.cur_page = self.pager.take_page(self.page_id);
            if self.cur_page.is_none() {
                return None; // have no page, done
            }
        }

        // Prepare the thing we want to return
        debug_assert!(
            self.cur_page.is_some(),
            "Cursor::next(): cur_page shouldn't be None at this point"
        );
        let page = self.cur_page.unwrap();
        let cell = page.cell(self.cell_idx);
        let key = cell.key();
        let val = cell.val_bytes();

        // Increment counter (or go to next page)
        self.cell_idx += 1;
        if self.cell_idx >= page.num_cells() {
            let next_page_id = page.next_node_id();
            self.pager.reg_page_with_id(page, self.page_id); // return back the page
            self.page_id = next_page_id;
            self.cell_idx = 0;
            self.cur_page = None;
        } else {
            self.cur_page = Some(page);
        }

        // Return what we prepared
        Some((key, val))
    }
}

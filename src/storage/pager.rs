//! # Pager
//!
//! Cache layer for disk's b-tree page

use std::{
    collections::{HashMap, HashSet},
    io::{Read, Seek, SeekFrom, Write},
};

use crate::storage::{
    EngineErr::{self, *},
    PAGE_SIZE,
    node::{NULL_NODE_ID, Page},
};

/// Trait for a anything pager can deal with
/// (Should be file or mock file)
pub trait TableSrc: Read + Write + Seek {}

// A trick for making everything that have Read + Write + Seek be a TableSrc
// They call it blanket implementation.
impl<T: Read + Write + Seek> TableSrc for T {}

// TODO: evict policy
pub struct Pager {
    cache: HashMap<u32, Page>,
    pub src: Box<dyn TableSrc>,
    taken: HashSet<u32>, // all the taken pages ID that is not registered back

    // current number of pages in the file (not cache)
    // NOTE: this is how big the file is, not how many live pages are
    num_pages: u32,

    header_size: usize, // file's header size
    free_page: u32,     // first free page ID
}

impl Pager {
    /// Creates new Pager with reader
    pub fn new(
        src: Box<dyn TableSrc>,
        num_pages: u32,
        header_size: usize,
        free_page: u32,
    ) -> Pager {
        Pager {
            cache: HashMap::new(),
            taken: HashSet::new(),
            src,
            num_pages,
            header_size,
            free_page,
        }
    }

    pub fn num_pages(&self) -> u32 {
        self.num_pages
    }

    pub fn free_page(&self) -> u32 {
        self.free_page
    }

    /// Get immutable page with target page id
    pub fn page(&mut self, id: u32) -> Option<&Page> {
        debug_assert!(!self.taken.contains(&id), "Page id {id} is taken.");

        // cache miss
        if !self.cache.contains_key(&id) {
            // Page doesn't exists
            if id >= self.num_pages {
                return None;
            }
            // Page exists but not on cache
            // read from src
            let mut buffer = [0u8; PAGE_SIZE];
            self.src
                .seek(SeekFrom::Start(self.node_offset(id)))
                .unwrap();
            self.src.read_exact(&mut buffer).unwrap();

            // save to cache
            self.cache.insert(id, Page::new_from_buffer(false, buffer));
        }

        self.cache.get(&id)
    }

    /// Get mutable page with target page id
    pub fn page_mut(&mut self, id: u32) -> Option<&mut Page> {
        debug_assert!(!self.taken.contains(&id), "Page id {id} is taken.");

        // cache miss
        if !self.cache.contains_key(&id) {
            // Page doesn't exists
            if id >= self.num_pages {
                return None;
            }
            // Page exists but not on cache
            // read from src
            let mut buffer = [0u8; PAGE_SIZE];
            self.src.seek(SeekFrom::Start(self.node_offset(id))).ok()?;
            self.src.read_exact(&mut buffer).ok()?;
            // save to cache
            self.cache.insert(id, Page::new_from_buffer(false, buffer));
        }

        self.cache.get_mut(&id)
    }

    /// Allocates new page with header, update its metadata and return it.
    pub fn new_page(&mut self, is_leaf: bool, is_root: bool) -> &Page {
        let new_id = self.allocate_page(is_leaf, is_root);
        self.cache.get(&new_id).unwrap()
    }

    /// Allocates new page with header, update its metadata and return it in mutable.
    pub fn new_page_mut(&mut self, is_leaf: bool, is_root: bool) -> &mut Page {
        let new_id = self.allocate_page(is_leaf, is_root);
        self.cache.get_mut(&new_id).unwrap()
    }

    // Allocate new page, use free page list if possible, and return new page id
    fn allocate_page(&mut self, is_leaf: bool, is_root: bool) -> u32 {
        // See if we can reuse the free pages
        let new_id = match self.free_page_list_pop() {
            // Free pages available
            Some(new_id) => {
                // In case that free page is still in the cache,
                // have to clear it before insert.
                self.cache.remove(&new_id);
                new_id
            }
            // No free page availabe
            None => {
                let new_id = self.num_pages;
                // update num_pages
                self.num_pages += 1;
                new_id
            }
        };

        // Create new page
        let page = Page::new(true, new_id, is_leaf, is_root);

        // add to cache
        self.cache.insert(new_id, page);

        new_id
    }

    #[allow(dead_code)] // might use later
    /// Flush page by id, only flush if dirty
    pub fn flush_by_id(&mut self, id: u32) -> Result<(), EngineErr> {
        if !self.cache.contains_key(&id) {
            return Err(PageNotExists(id));
        }

        let page = &self.cache[&id];
        // if not dirty, we're done
        if !page.is_dirty() {
            return Ok(());
        }

        // write to target offset
        self.src
            .seek(SeekFrom::Start(self.node_offset(id)))
            .map_err(|e| FsErr(Box::new(e)))?;
        self.src
            .write_all(page.bytes())
            .map_err(|e| FsErr(Box::new(e)))?;
        Ok(())
    }

    /// Flush all the page changes to file
    pub fn flush(&mut self) -> Result<(), EngineErr> {
        // All pages that are taken but not retured are all free:
        // Insert to the free page list
        self.taken_pages().into_iter().for_each(|id| {
            self.free_page_list_insert(id);
        });
        self.taken.clear(); // Clear all taken pages

        // Save all dirty pages to file
        for (id, page) in self.cache.iter() {
            if !page.is_dirty() {
                continue;
            }
            self.src
                .seek(SeekFrom::Start(self.node_offset(*id)))
                .map_err(|e| FsErr(Box::new(e)))?;
            self.src
                .write_all(page.bytes())
                .map_err(|e| FsErr(Box::new(e)))?;
        }
        Ok(())
    }

    /// calculates offset from start of the file to node
    fn node_offset(&self, id: u32) -> u64 {
        (self.header_size + ((id as usize) * PAGE_SIZE)) as u64
    }

    /// Registers new page into pager and return its assigned ID
    pub fn reg_page(&mut self, mut page: Page) -> u32 {
        let new_id = self.num_pages;
        page.set_is_page(true);
        page.set_id(new_id);

        // add to cache
        self.cache.insert(new_id, page);

        // update num_pages
        self.num_pages += 1;
        // return new page
        new_id
    }

    /// Register the page to pager using target id
    /// NOTE: this method will override the page with target id if exists
    /// Requires: only call this after using take_page(id) to take it
    pub fn reg_page_with_id(&mut self, mut page: Page, id: u32) {
        page.set_is_page(true);
        page.set_id(id);

        // add to cache
        self.cache.insert(id, page);
        // remove from taken list if it's taken
        self.taken.remove(&id);
    }

    /// Take ownership of the page with target id
    /// NOTE: to return it back, use reg_page_with_id()
    pub fn take_page(&mut self, id: u32) -> Option<Page> {
        debug_assert!(!self.taken.contains(&id), "Page id {id} already taken.");

        match self.cache.remove(&id) {
            Some(page) => {
                self.taken.insert(id);
                Some(page)
            }
            None => None,
        }
    }

    /// List all the taken but not returned page IDs
    pub fn taken_pages(&self) -> Vec<u32> {
        self.taken.iter().map(|r| *r).collect()
    }

    /// Insert the page with target id to free page list
    /// requires: must be called when flush()
    fn free_page_list_insert(&mut self, id: u32) {
        let old_head = self.free_page;
        // Update free page head pointer
        self.free_page = id;

        // Create newly added free page
        // NOTE: can't call self.page_mut because id is still consider taken
        let mut new_free_page = Page::new(true, id, true /*is_leaf shouldn't matter*/, false);
        // Set it up
        new_free_page.set_is_free_page(true);
        new_free_page.set_next_node_id(old_head);
        debug_assert!(!self.cache.contains_key(&id));
        // Add to cache because it's a valid 'free' page
        self.cache.insert(id, new_free_page);
    }

    /// If there is free page, delete the first free page we can find
    /// from free page list, set it to live page, and return its id.
    fn free_page_list_pop(&mut self) -> Option<u32> {
        if self.free_page == NULL_NODE_ID {
            return None;
        }
        let new_free_page = self.page(self.free_page).unwrap().next_node_id();
        let res = self.free_page;
        self.free_page = new_free_page;
        Some(res)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, io::Cursor};

    use super::*;
    use crate::storage::PAGE_SIZE;

    /// In-memory file: `Cursor<Vec<u8>>` is Read + Write + Seek, so the
    /// blanket impl already makes it a `TableSrc`.
    fn test_pager() -> Pager {
        Pager::new(
            Box::new(Cursor::new(vec![0u8; PAGE_SIZE * 64])),
            0,
            0,
            NULL_NODE_ID,
        )
    }

    /// Free two pages, flush (which is what puts them on the free list), then
    /// allocate again: the same ids come back and the file does not grow.
    #[test]
    fn free_list_reuses_freed_ids() {
        let mut pager = test_pager();
        let ids: Vec<u32> = (0..4).map(|_| pager.new_page(true, false).id()).collect();
        assert_eq!(ids, vec![0, 1, 2, 3]);

        // 1 and 2 die in a merge and are never registered back
        for id in [1u32, 2] {
            pager.take_page(id).unwrap();
        }
        pager.flush().unwrap();

        let mut reused: Vec<u32> = (0..2).map(|_| pager.new_page(true, false).id()).collect();
        reused.sort();
        assert_eq!(reused, vec![1, 2]);
        assert_eq!(pager.num_pages(), 4, "reuse must not extend the file");
    }

    /// A merged-away leaf still has a live sibling named in its on-disk
    /// `next_node_id`. the same field the free list reuses as its link. If the
    /// list ever reads that stale pointer as a link, the next allocation hands
    /// out a page that is still in the tree.
    #[test]
    fn free_list_never_hands_out_a_live_page() {
        let mut pager = test_pager();
        let ids: Vec<u32> = (0..4).map(|_| pager.new_page(true, false).id()).collect();

        // Each page names page 0 as its sibling, and page 0 stays live.
        for id in &ids {
            pager.page_mut(*id).unwrap().set_next_node_id(ids[0]);
        }
        pager.flush().unwrap();
        let mut live: HashSet<u32> = ids.iter().copied().collect();

        for id in [1u32, 2] {
            pager.take_page(id).unwrap();
            live.remove(&id);
        }
        pager.flush().unwrap();

        // One more allocation than there are freed pages, so the list gets
        // fully drained and has to fall through to fresh file space.
        for _ in 0..3 {
            let id = pager.new_page(true, false).id();
            assert!(live.insert(id), "page {id} handed out while still live");
        }
    }
}

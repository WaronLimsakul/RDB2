//! # Pager
//!
//! Cache layer for disk's b-tree page

use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom, Write},
};

use crate::storage::{
    EngineErr::{self, *},
    PAGE_HEADER_SIZE, PAGE_SIZE,
    node::Page,
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
    num_pages: u32,     // current number of pages in the file (not cache)
    header_size: usize, // file's header size
}

impl Pager {
    /// Creates new Pager with reader
    pub fn new(src: Box<dyn TableSrc>, num_pages: u32, header_size: usize) -> Pager {
        Pager {
            cache: HashMap::new(),
            src,
            num_pages,
            header_size,
        }
    }

    pub fn num_pages(&self) -> u32 {
        self.num_pages
    }

    /// Get immutable page with target page id
    pub fn page(&mut self, id: u32) -> Option<&Page> {
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

    /// Allocates new page with header, update its metadata and return it
    pub fn new_page(&mut self, is_leaf: bool, is_root: bool) -> &Page {
        // create new page with new id
        let new_id = self.num_pages;
        let page = Page::new(true, new_id, is_leaf, is_root);

        // add to cache
        self.cache.insert(new_id, page);

        // update num_pages
        self.num_pages += 1;
        // return new page
        return self.cache.get(&new_id).unwrap();
    }

    /// Allocates new page with header, update its metadata and return it in mutable
    pub fn new_page_mut(&mut self, is_leaf: bool, is_root: bool) -> &mut Page {
        // create new page with new id
        let new_id = self.num_pages;
        let page = Page::new(true, new_id, is_leaf, is_root);

        // add to cache
        self.cache.insert(new_id, page);

        // update num_pages
        self.num_pages += 1;
        // return new page
        return self.cache.get_mut(&new_id).unwrap();
    }

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

    /// Flush all dirty pages in pager
    pub fn flush(&mut self) -> Result<(), EngineErr> {
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
        return new_id;
    }

    /// Register the page to pager using target id
    /// NOTE: this method will override the page with target id if exists
    /// Requires: only call this after using take_page(id) to take it
    pub fn reg_page_with_id(&mut self, mut page: Page, id: u32) {
        page.set_is_page(true);
        page.set_id(id);

        // add to cache
        self.cache.insert(id, page);
    }

    /// Take ownership of the page with target id
    /// Requires: the caller must register it back using reg_page_with_id(id)
    pub fn take_page(&mut self, id: u32) -> Option<Page> {
        self.cache.remove(&id)
    }
}

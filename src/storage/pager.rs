use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom, Write},
};

use crate::storage::{PAGE_SIZE, node::Page};

/// Trait for a anything pager can deal with
/// (Should be file or mock file)
pub trait TableSrc: Read + Write + Seek {}

// A trick for making everything that have Read + Write + Seek be a TableSrc
// They call it blanket implementation.
impl<T: Read + Write + Seek> TableSrc for T {}

// TODO: evict policy
pub struct Pager {
    cache: HashMap<u32, Page>,
    src: Box<dyn TableSrc>,
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
            let offset = self.header_size + (id as usize * PAGE_SIZE);
            self.src.seek(SeekFrom::Start(offset as u64)).ok()?;
            self.src.read_exact(&mut buffer).ok()?;
            // save to cache
            self.cache.insert(id, Page::new(false, buffer));
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
            let offset = self.header_size + (id as usize * PAGE_SIZE);
            self.src.seek(SeekFrom::Start(offset as u64)).ok()?;
            self.src.read_exact(&mut buffer).ok()?;
            // save to cache
            self.cache.insert(id, Page::new(false, buffer));
        }

        self.cache.get_mut(&id)
    }

    /// Allocates new page with header, ready-to-use
    pub fn new_page(&mut self) -> &Page {
        // create new page with new id
        let mut page = Page::new(false, [0u8; PAGE_SIZE]);
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
    pub fn new_page_mut(&mut self) -> &mut Page {
        // create new page with new id
        let mut page = Page::new(false, [0u8; PAGE_SIZE]);
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
    pub fn flush_by_id() {}
    /// Flush all dirty pages in pager
    pub fn flush() {}
}

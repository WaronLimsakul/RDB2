use std::collections::HashMap;

pub enum Type {
    INT,
    UINT,
    LONG,
    ULONG,
    STRING,
    BOOL,
}

struct Table {
    schema: Vec<(String, Type)>,
}

pub struct StorageEngine {
    root_dir: String,
    tables: HashMap<String, Table>, // filename -> table abstract
}

// TODO: see if it should be method or just fn
// TODO 2: define custom error, see if defined here or other place
impl StorageEngine {
    pub fn new_table(name: String) {
        println!("new_table called");
    }

    pub fn insert(table_id: u32, node_id: u32) {
        println!("insert called");
    }
    pub fn delete(table_id: u32, node_id: u32) {
        println!("delete called");
    }
    pub fn update(table_id: u32, node_id: u32) {
        println!("update called");
    }
}

#[cfg(test)]
mod tests {
    // TODO
}

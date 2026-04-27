use crate::storage::{KeyType, engine::StorageEngine, table::TableSchema};

mod storage;

fn main() {
    println!("Aight, let's get started");
    let mut engine = StorageEngine::new("./db/").unwrap();

    match engine.new_table(
        "test4",
        TableSchema {
            key: (String::from("id"), KeyType::Ulong),
            vals: vec![],
        },
    ) {
        Ok(_) => println!("Done"),
        Err(err) => println!("new_table: {err}"),
    };
    engine.flush("test");
}

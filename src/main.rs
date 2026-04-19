use crate::storage_engine::{StorageEngine, Type};

mod storage_engine;

fn main() {
    println!("Aight, let's get started");
    let mut engine = StorageEngine::new("./db/").unwrap();

    match engine.new_table("test4", vec![(String::from("id"), Type::Ulong)]) {
        Ok(_) => println!("Done"),
        Err(err) => println!("new_table: {err}"),
    };
    engine.flush("test");
}

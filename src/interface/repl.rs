use std::io::Write;

use crate::interface::{Cmd, CmdType, MetaCmd};

pub fn get_input() -> Cmd {
    let raw = get_raw_input();

    let cmd_type = if raw.chars().nth(0).unwrap() == '.' {
        let meta_cmd = match &raw[1..] {
            "quit" => MetaCmd::Quit,
            "exit" => MetaCmd::Quit,
            _ => {
                panic!("Meta command {} not found", String::from(raw));
            } // TODO: handle it better
        };

        CmdType::Meta(meta_cmd)
    } else {
        CmdType::DQL // TODO: parse it seriously here
    };

    return Cmd { cmd_type, raw };
}

pub fn get_raw_input() -> String {
    print!("> ");
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    return String::from(input.trim_end());
}

pub fn output(msg: &str) {
    println!("{msg}");
}

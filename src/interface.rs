mod parser;
pub mod repl;

pub enum CmdType {
    Meta(MetaCmd),
    DDL,
    DML,
    DQL,
}

pub enum MetaCmd {
    Quit,
}

pub struct Cmd {
    pub cmd_type: CmdType,
    pub raw: String,
}

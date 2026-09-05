//! # Storage (engine)
//!
//! Contains abstraction for RDB2 storage engine

mod cell;
mod cursor;
pub mod engine;
mod node;
mod pager;
pub mod row_cursor;
pub mod table;

use EngineErr::*;
use std::{error::Error, fmt::Display};

use crate::storage::engine::StorageEngine;

const TABLE_MAGIC_NUMBER: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
const TABLE_FILE_EXTENSION: &str = "rdb";

const PAGE_SIZE: usize = 4096;
const PAGE_MAGIC_NUMBER: [u8; 4] = [0x50, 0x41, 0x47, 0x45]; // "PAGE"
const FOUR_BYTES_ZERO: [u8; 4] = [0u8; 4];
const PAGE_HEADER_SIZE: usize = 23;

const MAX_RECORD_SIZE: usize = PAGE_SIZE / 8;

/// Allowed columns types
// Change this -> change
// 1. all impls
// 2. ColData
// 3. TableSchema impl
// 4. parser::parse_column_type
// 5. query::litera_to_column_data
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Type {
    Int,
    Uint,
    Long,
    Ulong,
    String, // size (u16) + utf-8
    Bool,
    Float,
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Type::*;
        match self {
            Int => write!(f, "integer"),
            Uint => write!(f, "unsigned integer"),
            Long => write!(f, "long"),
            Ulong => write!(f, "unsigned long"),
            String => write!(f, "string"),
            Bool => write!(f, "bolean"),
            Float => write!(f, "float"),
        }
    }
}

impl From<KeyType> for Type {
    fn from(value: KeyType) -> Self {
        match value {
            KeyType::Uint => Type::Uint,
            KeyType::Ulong => Type::Ulong,
        }
    }
}

impl Type {
    /// Convert type to byte representation
    fn to_byte(&self) -> u8 {
        use Type::*;
        match self {
            Int => 0,
            Uint => 1,
            Long => 2,
            Ulong => 3,
            String => 4,
            Bool => 5,
            Float => 6,
        }
    }

    /// Try to convert a byte flag to type
    fn from_byte(b: u8) -> Option<Type> {
        use Type::*;
        match b {
            0 => Some(Int),
            1 => Some(Uint),
            2 => Some(Long),
            3 => Some(Ulong),
            4 => Some(String),
            5 => Some(Bool),
            6 => Some(Float),
            _ => None,
        }
    }

    /// Decode bytes data into data of that type. Return the ColData and number of bytes read
    fn decode(&self, bytes: &[u8]) -> Result<(ColData, usize), EngineErr> {
        match self {
            Type::Int => Ok((
                ColData::Int(i32::from_be_bytes(bytes[0..4].try_into().unwrap())),
                4,
            )),
            Type::Uint => Ok((
                ColData::Uint(u32::from_be_bytes(bytes[0..4].try_into().unwrap())),
                4,
            )),
            Type::Long => Ok((
                ColData::Long(i64::from_be_bytes(bytes[0..8].try_into().unwrap())),
                8,
            )),
            Type::Ulong => Ok((
                ColData::Ulong(u64::from_be_bytes(bytes[0..8].try_into().unwrap())),
                8,
            )),
            Type::String => {
                let len = u16::from_be_bytes(bytes[0..2].try_into().unwrap()) as usize;
                let s = String::from_utf8(bytes[2..2 + len].to_vec())
                    .map_err(|_| EngineErr::InvalidUtf8)?;
                Ok((ColData::String(s), 2 + len))
            }
            Type::Bool => Ok((ColData::Bool(bytes[0] == 1), 1)),
            Type::Float => Ok((
                (ColData::Float(f32::from_be_bytes(bytes[0..4].try_into().unwrap()))),
                4,
            )),
        }
    }
}

/// Types that are allowed for key
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum KeyType {
    Uint,
    Ulong,
}

impl TryFrom<Type> for KeyType {
    type Error = EngineErr;
    fn try_from(value: Type) -> Result<Self, Self::Error> {
        match value {
            Type::Ulong => Ok(KeyType::Ulong),
            Type::Uint => Ok(KeyType::Uint),
            other => Err(UnSupportedType(other)),
        }
    }
}

impl Display for KeyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Type::from(*self))
    }
}

/// Column types with data
#[derive(Clone, Debug, PartialEq)]
pub enum ColData {
    Int(i32),
    Uint(u32),
    Long(i64),
    Ulong(u64),
    String(String),
    Bool(bool),
    Float(f32),
}

impl ColData {
    fn size(&self) -> usize {
        use ColData::*;
        match self {
            Int(_) | Uint(_) | Float(_) => 4,
            Long(_) | Ulong(_) => 8,
            String(s) => 2 + s.len(),
            Bool(_) => 1,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        use ColData::*;
        match self {
            Int(d) => d.to_be_bytes().to_vec(),
            Uint(d) => d.to_be_bytes().to_vec(),
            Long(d) => d.to_be_bytes().to_vec(),
            Ulong(d) => d.to_be_bytes().to_vec(),
            String(s) => {
                let len = u16::try_from(s.len()).unwrap();
                let mut buffer = Vec::with_capacity(2 + s.len());
                buffer.extend_from_slice(&len.to_be_bytes());
                buffer.extend_from_slice(s.as_bytes());
                buffer
            }
            Bool(true) => vec![1u8],
            Bool(false) => vec![0u8],
            Float(f) => f.to_be_bytes().to_vec(),
        }
    }

    /// serialize its bytes representation into vector v
    fn serialize_into(&self, v: &mut Vec<u8>) {
        use ColData::*;
        match self {
            Int(d) => v.extend_from_slice(&d.to_be_bytes()),
            Uint(d) => v.extend_from_slice(&d.to_be_bytes()),
            Long(d) => v.extend_from_slice(&d.to_be_bytes()),
            Ulong(d) => v.extend_from_slice(&d.to_be_bytes()),
            String(s) => {
                let len = u16::try_from(s.len()).unwrap();
                v.extend_from_slice(&len.to_be_bytes());
                v.extend_from_slice(s.as_bytes());
            }
            Bool(true) => v.push(1u8),
            Bool(false) => v.push(0u8),
            Float(f) => v.extend_from_slice(&f.to_be_bytes()),
        };
    }
}

impl Display for ColData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ColData::*;
        match self {
            Int(v) => write!(f, "{v}"),
            Uint(v) => write!(f, "{v}"),
            Long(v) => write!(f, "{v}"),
            Ulong(v) => write!(f, "{v}"),
            String(v) => write!(f, "{v}"),
            Bool(v) => write!(f, "{v}"),
            Float(v) => write!(f, "{v}"),
        }
    }
}

/// Key type with data
// NOTE: derived impl says Uint < Ulong
#[derive(PartialEq, PartialOrd, Debug, Clone, Copy)]
pub enum KeyData {
    Uint(u32),
    Ulong(u64),
}

impl KeyData {
    fn size(&self) -> usize {
        use KeyData::*;
        match self {
            Uint(_) => 4,
            Ulong(_) => 8,
        }
    }

    /// Returns its representation as bytes
    fn to_bytes(&self) -> Vec<u8> {
        use KeyData::*;
        match self {
            Uint(d) => d.to_be_bytes().to_vec(),
            Ulong(d) => d.to_be_bytes().to_vec(),
        }
    }
}

impl From<KeyData> for ColData {
    fn from(value: KeyData) -> Self {
        match value {
            KeyData::Uint(v) => ColData::Uint(v),
            KeyData::Ulong(v) => ColData::Ulong(v),
        }
    }
}

impl TryFrom<ColData> for KeyData {
    type Error = EngineErr;
    fn try_from(value: ColData) -> Result<Self, Self::Error> {
        match value {
            ColData::Uint(v) => Ok(KeyData::Uint(v)),
            ColData::Ulong(v) => Ok(KeyData::Ulong(v)),
            _ => Err(EngineErr::ValueConversion(value)),
        }
    }
}

/// Error type for engine, just display to see what to wanna say
#[derive(Debug)]
pub enum EngineErr {
    TableAlreadyExists(String),
    TableNotFound(String),
    FsErr(Box<dyn Error>),
    UnSupportedType(Type),
    Empty(String),
    InvalidMagicNumber,
    InvalidUtf8,
    InvalidTypeByte,
    InvalidStrLenght,
    InvalidKeySize(u8),
    RowExists(KeyData),
    PageFull,
    PageNotExists(u32),
    KeyTypeConversion(Type, KeyType), // Can't convert `Type` to the `KeyType`
    ValueConversion(ColData),         // Can't convert this `ColData` to whatever
}

impl Display for EngineErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableAlreadyExists(name) => {
                write!(f, "Table {name} already exists.")
            }
            FsErr(err) => write!(f, "Fs error: {}", err),
            UnSupportedType(t) => {
                write!(f, "Type {t} unsupported for the task.")
            }
            Empty(what) => write!(f, "{what} is empty."),
            InvalidMagicNumber => write!(f, "Unmatched magic number."),
            InvalidUtf8 => write!(f, "Found invalid UTF-8."),
            InvalidTypeByte => write!(f, "Found invalid type byte."),
            InvalidStrLenght => write!(f, "Invalid string length."),
            InvalidKeySize(s) => write!(f, "Found invalid key size {s}."),
            RowExists(k) => write!(f, "Row with key {:?} already exists.", k),
            PageFull => write!(f, "Page full."),
            PageNotExists(id) => write!(f, "Page id {id} doesn't exists."),
            TableNotFound(table) => write!(f, "Table {table} not found."),
            KeyTypeConversion(from, to) => write!(f, "Error converting {from} type to {to} type"),
            ValueConversion(value) => write!(f, "Cannot convert from value {value}"),
        }
    }
}

impl std::error::Error for EngineErr {}

/// Record Data := Represent the row data except key
pub struct RecData {
    pub vals: Vec<ColData>,
}

impl RecData {
    pub fn size(&self) -> usize {
        self.vals.iter().map(|col_data| col_data.size()).sum()
    }
    /// Return bytes representation of record data
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.size());
        for col in &self.vals {
            col.serialize_into(&mut v);
        }
        v
    }
}

/// Represents a row with data (not just schema)
pub struct RowData {
    pub key: KeyData,
    pub vals: RecData,
}

impl RowData {
    pub fn size(&self) -> usize {
        self.key.size() + self.vals.size()
    }
}

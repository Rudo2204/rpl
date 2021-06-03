pub mod error;
pub mod qbittorrent;
pub mod torrent_parser;
pub mod util;

pub use crate::librpl::error as _;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RplFile<'a> {
    filename: &'a str,
    length: i64,
    downloaded: bool,
    chunk: i32,
}

impl<'a> RplFile<'a> {
    fn new(filename: &'a str, length: i64, downloaded: bool, chunk: i32) -> Self {
        Self {
            filename,
            length,
            downloaded,
            chunk,
        }
    }
}

pub trait RplChunk<'a> {
    fn chunks(&'a mut self) -> Result<HashMap<&PathBuf, RplFile<'a>>, error::Error>;
}

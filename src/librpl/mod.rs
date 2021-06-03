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

#[derive(Debug)]
pub struct RplFileShort<'a> {
    filename: &'a str,
    length: i64,
}

pub struct Queue<'a> {
    chunk: i32,
    files: Vec<RplFileShort<'a>>,
}

pub trait RplDownload<'a, T>
where
    T: RplChunk<'a>,
{
    //fn build_queue(&'a mut self) -> Result<Vec<Queue<'a>>, error::Error>;
    fn download(&'a mut self) -> Result<(), error::Error>;
}

pub fn build_queue<'a>(datamap: HashMap<&PathBuf, RplFile<'a>>) {
    let mut current_chunk = 0;
    //let queue: Vec<Queue> = Vec::new();
    loop {
        let tmp = find_keys_for_chunk(&datamap, current_chunk);
        if tmp.is_empty() {
            break;
        } else {
            println!("{:#?}", tmp);
            current_chunk += 1;
        }
    }
}

fn find_keys_for_chunk<'a>(
    datamap: &'a HashMap<&PathBuf, RplFile<'a>>,
    chunk: i32,
) -> Vec<RplFileShort<'a>> {
    datamap
        .iter()
        .filter_map(|(_key, val)| {
            if val.chunk == chunk {
                Some(RplFileShort {
                    filename: val.filename,
                    length: val.length,
                })
            } else {
                None
            }
        })
        .collect()
}

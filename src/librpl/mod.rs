pub mod error;
pub mod qbittorrent;
pub mod torrent_parser;
pub mod util;

use humansize::{file_size_opts, FileSize};
use lava_torrent::torrent::v1::Torrent;
use log::info;
use std::collections::HashMap;
use std::path::PathBuf;

pub use crate::librpl::error as _;

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

pub struct Queue {
    chunk: i32,
    total_size: i64,
    no_files: i32,
}

impl Queue {
    fn new(chunk: i32, total_size: i64, no_files: i32) -> Self {
        Self {
            chunk,
            total_size,
            no_files,
        }
    }

    fn info(&self) {
        let avg = self.total_size / self.no_files as i64;
        info!(
            "Chunk {} has {} files with total size of {}. Average size per file is {}.",
            self.chunk,
            self.no_files,
            self.total_size
                .file_size(file_size_opts::BINARY)
                .expect("File size is a negative number?"),
            avg.file_size(file_size_opts::BINARY)
                .expect("File size is a negative number?"),
        )
    }
}

pub trait RplDownload<'a, T>
where
    T: RplChunk<'a>,
{
    fn download_torrent(&'a mut self, data: Torrent) -> Result<(), error::Error>;
}

pub fn build_queue<'a>(datamap: HashMap<&PathBuf, RplFile<'a>>, torrent: Torrent) -> Vec<Queue> {
    let mut current_chunk = 0;
    let mut total_size: i64 = 0;
    let mut queue: Vec<Queue> = Vec::new();
    let mut files = 0;
    for f in torrent.files.unwrap() {
        let file = datamap
            .get_key_value(&f.path)
            .expect("Could not find file in data map")
            .1;

        if file.chunk < 0 {
            continue;
        } else if file.chunk != current_chunk {
            queue.push(Queue::new(current_chunk, total_size, files));
            files = 0;
            total_size = 0;
            current_chunk += 1;
        }
        files += 1;
        total_size += file.length;
    }

    queue
}

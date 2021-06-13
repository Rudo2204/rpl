pub mod error;
pub mod qbittorrent;
pub mod rclone;
pub mod torrent_parser;
pub mod util;

use async_trait::async_trait;
use derive_getters::Getters;
use humansize::{file_size_opts, FileSize};
use lava_torrent::torrent::v1::Torrent;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub use crate::librpl::error as _;
pub use crate::librpl::qbittorrent::QbitConfig;
pub use crate::librpl::rclone::RcloneClient;

pub trait RplClient {}
pub trait RplPackConfig {}

#[derive(Debug)]
pub struct RplFile<'a> {
    filename: &'a str,
    length: i64,
    chunk: i32,
}

impl<'a> RplFile<'a> {
    fn new(filename: &'a str, length: i64, chunk: i32) -> Self {
        Self {
            filename,
            length,
            chunk,
        }
    }
}

pub trait RplChunk<'a> {
    fn chunks(&'a mut self) -> Result<HashMap<&PathBuf, RplFile<'a>>, error::Error>;
}

pub struct Job {
    chunk: i32,
    total_size: i64,
    no_files: i32,
}

impl Job {
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

#[async_trait]
pub trait RplLeech<'a, T, P, C>
where
    T: RplChunk<'a>,
    P: RplPackConfig,
    C: RplClient,
{
    async fn leech_torrent(
        &'a mut self,
        data: Torrent,
        config: P,
        torrent_client: C,
        upload_client: RcloneClient,
        seed: SeedSettings,
        skip: u32,
    ) -> Result<(), error::Error>;
}

pub struct Queue {
    no_all_files: i32,
    job: Vec<Job>,
}

impl Queue {
    fn new(no_all_files: i32, job: Vec<Job>) -> Self {
        Self { no_all_files, job }
    }
}

pub fn build_queue(datamap: HashMap<&PathBuf, RplFile<'_>>, torrent: Torrent) -> Queue {
    let mut current_chunk = 1;
    let mut total_size: i64 = 0;
    let mut job: Vec<Job> = Vec::new();
    let mut files = 0;
    let mut no_all_files: i32 = 0;

    for f in torrent.files.unwrap() {
        no_all_files += 1;

        let file = datamap
            .get_key_value(&f.path)
            .expect("Could not find file in data map")
            .1;

        if file.chunk < 0 {
            continue;
        } else if file.chunk != current_chunk {
            job.push(Job::new(current_chunk, total_size, files));
            files = 0;
            total_size = 0;
            current_chunk += 1;
        }
        files += 1;
        total_size += file.length;
    }

    // finish off last chunk
    job.push(Job::new(current_chunk, total_size, files));
    Queue::new(no_all_files, job)
}

pub trait RplUpload {
    fn upload(&self, client: &RcloneClient, no_jobs: usize) -> Result<(), error::Error>;
}

#[derive(Serialize, Deserialize, Getters)]
pub struct SeedSettings {
    seed_enable: bool,
    seed_path: String,
    seed_wait: u32,
}

impl Default for SeedSettings {
    fn default() -> Self {
        Self {
            seed_enable: false,
            seed_path: String::from(""),
            seed_wait: 0,
        }
    }
}

impl SeedSettings {
    pub fn new(seed_enable: bool, seed_path: String, seed_wait: u32) -> Self {
        Self {
            seed_enable,
            seed_path,
            seed_wait,
        }
    }
}

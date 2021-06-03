use humansize::{file_size_opts, FileSize};
use lava_torrent::torrent::v1::Torrent;
use log::{debug, warn};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::librpl::error;
use crate::librpl::RplChunk;
use crate::librpl::RplFile;

pub struct TorrentPack<'a> {
    pub max_size_allow: i64,
    pub torrent: Torrent,
    pub downloaded_file: Option<HashMap<&'a PathBuf, RplFile<'a>>>,
}

impl<'a> TorrentPack<'a> {
    pub fn new(torrent: Torrent) -> Self {
        Self {
            max_size_allow: 0,
            torrent,
            downloaded_file: None,
        }
    }

    pub fn max_size(mut self, size: i64) -> Self {
        self.max_size_allow = size;
        self
    }

    pub fn is_private(&self) -> bool {
        self.torrent.is_private()
    }

    pub fn info_hash(&self) -> String {
        self.torrent.info_hash()
    }

    pub fn get_pack_size_human(&self) -> String {
        self.torrent
            .length
            .file_size(file_size_opts::BINARY)
            .expect("File size is a negative number?")
    }
}

impl<'a> RplChunk<'a> for TorrentPack<'a> {
    fn chunks(&'a mut self) -> Result<HashMap<&PathBuf, RplFile<'a>>, error::Error> {
        let file_vecs = match &self.torrent.files {
            Some(vecs) => vecs,
            None => return Err(error::Error::EmptyTorrent),
        };

        let mut empty_hashmap = HashMap::new();
        let downloaded = match &mut self.downloaded_file {
            Some(vecs) => vecs,
            None => &mut empty_hashmap,
        };

        let mut files_in_downloaded = downloaded.len();
        let files_in_pack = file_vecs.len();

        let mut chunks: HashMap<&PathBuf, RplFile> = HashMap::new();
        let mut current_chunk: i32 = 0;

        while files_in_downloaded != files_in_pack {
            let mut current_sum_size: i64 = 0;
            for (index, file) in file_vecs.into_iter().enumerate() {
                if downloaded.contains_key(&file.path) {
                    continue;
                } else {
                    if file.length > self.max_size_allow {
                        downloaded.insert(
                            &file.path,
                            RplFile::new(file.path.to_str().unwrap(), file.length, false, -1),
                        );

                        warn!(
                            "File {} has size {} which is larger than maximum size allowed {}. This file will be skipped.",
                            file.path
                                .to_str()
                                .expect("Could not convert file path to str"),
                            file.length.file_size(file_size_opts::BINARY).unwrap(),
                            self.max_size_allow
                                .file_size(file_size_opts::BINARY)
                                .unwrap()
                        );
                    // last file case
                    } else if index + 1 == files_in_pack {
                        downloaded.insert(
                            &file.path,
                            RplFile::new(&file.path.to_str().unwrap(), file.length, true, -1),
                        );

                        if current_sum_size + file.length > self.max_size_allow {
                            chunks.insert(
                                &file.path,
                                RplFile::new(
                                    &file.path.to_str().unwrap(),
                                    file.length,
                                    true,
                                    current_chunk,
                                ),
                            );
                            current_chunk += 1;
                            current_sum_size = 0;
                        }

                        chunks.insert(
                            &file.path,
                            RplFile::new(
                                &file.path.to_str().unwrap(),
                                file.length,
                                true,
                                current_chunk,
                            ),
                        );
                    } else if current_sum_size + file.length <= self.max_size_allow {
                        debug!(
                            "Added file {} size {} index {} chunk {}",
                            file.path.to_str().unwrap(),
                            file.length,
                            index,
                            current_chunk,
                        );
                        downloaded.insert(
                            &file.path,
                            RplFile::new(&file.path.to_str().unwrap(), file.length, true, -1),
                        );

                        chunks.insert(
                            &file.path,
                            RplFile::new(
                                &file.path.to_str().unwrap(),
                                file.length,
                                true,
                                current_chunk,
                            ),
                        );
                        current_sum_size += file.length;
                    } else {
                        chunks.insert(
                            &file.path,
                            RplFile::new(
                                &file.path.to_str().unwrap(),
                                file.length,
                                true,
                                current_chunk,
                            ),
                        );
                        current_chunk += 1;
                        current_sum_size = 0;
                        debug!(
                            "Added file {} size {} index {} chunk {}",
                            file.path.to_str().unwrap(),
                            file.length,
                            index,
                            current_chunk,
                        );
                        downloaded.insert(
                            &file.path,
                            RplFile::new(&file.path.to_str().unwrap(), file.length, true, -1),
                        );
                        chunks.insert(
                            &file.path,
                            RplFile::new(
                                &file.path.to_str().unwrap(),
                                file.length,
                                true,
                                current_chunk,
                            ),
                        );

                        current_sum_size += file.length;
                    }

                    files_in_downloaded = downloaded.len();
                }
            }
        }
        Ok(chunks)
    }
}

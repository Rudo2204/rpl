use humansize::{file_size_opts, FileSize};
use lava_torrent::torrent::v1::Torrent;
use log::{debug, error, warn};
use std::collections::HashMap;

use crate::librpl::error;
use crate::librpl::RplChunk;
use crate::librpl::RplFile;

pub fn get_largest_filesize(torrent: Torrent) -> i64 {
    match torrent.files {
        None => torrent.length,
        Some(vec_files) => {
            vec_files
                .iter()
                .max_by(|x, y| x.length.cmp(&y.length))
                .expect("Could not find file with largest fliesize")
                .length
        }
    }
}

pub struct TorrentPack {
    max_size_allow: i64,
    pub torrent: Torrent,
    ignore_warning: bool,
}

impl TorrentPack {
    pub fn new(torrent: Torrent, ignore_warning: bool) -> Self {
        Self {
            max_size_allow: 0,
            torrent,
            ignore_warning,
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

    pub fn get_max_size_chunk_human(&self) -> String {
        self.max_size_allow
            .file_size(file_size_opts::BINARY)
            .expect("File size is a negative number?")
    }
}

impl<'a> RplChunk<'a> for TorrentPack {
    fn chunks(&'a mut self) -> Result<HashMap<&str, RplFile<'a>>, error::Error> {
        let mut chunks: HashMap<&str, RplFile> = HashMap::new();
        let file_vecs;
        match &self.torrent.files {
            Some(vecs) => file_vecs = vecs,
            None => {
                warn!("This torrent \"pack\" has only 1 file");
                let path = &self.torrent.name;
                let size = self.torrent.length;
                if size > self.max_size_allow {
                    if self.ignore_warning {
                        warn!(
                            "File `{}` has size {} which is larger than maximum size allowed {}. This file will be skipped.",
                            path,
                            size.file_size(file_size_opts::BINARY).unwrap(),
                            self.max_size_allow
                                .file_size(file_size_opts::BINARY)
                                .unwrap()
                        );
                        chunks.insert(path, RplFile::new(path, size, -1));
                        return Ok(chunks);
                    } else {
                        error!(
                            "File `{}` has size {} which is larger than maximum size allowed {}. If you want to ignore this file, rerun the program with -f/--force",
                            path,
                            size.file_size(file_size_opts::BINARY).unwrap(),
                            self.max_size_allow
                                .file_size(file_size_opts::BINARY)
                                .unwrap()
                        );

                        return Err(error::Error::MaxSizeAllowedTooSmall);
                    }
                } else {
                    chunks.insert(path, RplFile::new(&path, size, 1));
                    return Ok(chunks);
                }
            }
        };

        let files_in_pack = file_vecs.len();

        let mut current_chunk: i32 = 1;

        let mut current_sum_size: i64 = 0;
        for (index, file) in file_vecs.iter().enumerate() {
            if file.length > self.max_size_allow {
                chunks.insert(
                    file.path.to_str().unwrap(),
                    RplFile::new(file.path.to_str().unwrap(), file.length, -1),
                );

                if self.ignore_warning {
                    warn!(
                            "File `{}` has size {} which is larger than maximum size allowed {}. This file will be skipped.",
                            file.path
                                .to_str()
                                .expect("Could not convert file path to str"),
                            file.length.file_size(file_size_opts::BINARY).unwrap(),
                            self.max_size_allow
                                .file_size(file_size_opts::BINARY)
                                .unwrap()
                        );
                } else {
                    error!(
                            "File `{}` has size {} which is larger than maximum size allowed {}. If you want to ignore this file, rerun the program with -f/--force",
                            file.path
                                .to_str()
                                .expect("Could not convert file path to str"),
                            file.length.file_size(file_size_opts::BINARY).unwrap(),
                            self.max_size_allow
                                .file_size(file_size_opts::BINARY)
                                .unwrap()
                        );

                    return Err(error::Error::MaxSizeAllowedTooSmall);
                }
            // last file case
            } else if index + 1 == files_in_pack {
                if current_sum_size + file.length > self.max_size_allow {
                    current_chunk += 1;
                }

                chunks.insert(
                    file.path.to_str().unwrap(),
                    RplFile::new(&file.path.to_str().unwrap(), file.length, current_chunk),
                );
                debug!(
                    "Added {} size {} index {} chunk {}",
                    file.path.to_str().unwrap(),
                    file.length,
                    index,
                    current_chunk,
                );
            } else if current_sum_size + file.length <= self.max_size_allow {
                debug!(
                    "Added {} size {} index {} chunk {}",
                    file.path.to_str().unwrap(),
                    file.length,
                    index,
                    current_chunk,
                );
                chunks.insert(
                    file.path.to_str().unwrap(),
                    RplFile::new(&file.path.to_str().unwrap(), file.length, current_chunk),
                );
                current_sum_size += file.length;
            } else {
                current_chunk += 1;
                current_sum_size = 0;
                current_sum_size += file.length;
                debug!(
                    "Added {} size {} index {} chunk {}",
                    file.path.to_str().unwrap(),
                    file.length,
                    index,
                    current_chunk,
                );
                chunks.insert(
                    file.path.to_str().unwrap(),
                    RplFile::new(&file.path.to_str().unwrap(), file.length, current_chunk),
                );
            }
        }

        Ok(chunks)
    }
}

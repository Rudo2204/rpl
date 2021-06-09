#![allow(dead_code)]
#![allow(unused_imports)]

use async_trait::async_trait;
use derive_builder::Builder;
use derive_getters::Getters;
use indicatif::{ProgressBar, ProgressStyle};
use lava_torrent::torrent::v1::Torrent;
use log::{debug, info};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::convert::TryInto;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::time::{sleep, Duration};

use crate::librpl::rclone::RcloneClient;
use crate::librpl::torrent_parser::TorrentPack;
use crate::librpl::RplUpload;
use crate::librpl::{build_queue, error, Job, RplChunk, RplClient, RplLeech, RplPackConfig};

#[derive(Deserialize, Serialize)]
enum TorrentFilter {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "active")]
    Active,
}

#[derive(Debug, Deserialize, Clone)]
pub enum State {
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "missingFiles")]
    MissingFiles,
    #[serde(rename = "uploading")]
    Uploading,
    #[serde(rename = "pausedUP")]
    PausedUP,
    #[serde(rename = "queuedUP")]
    QueuedUP,
    #[serde(rename = "stalledUP")]
    StalledUP,
    #[serde(rename = "checkingUP")]
    CheckingUP,
    #[serde(rename = "forcedUP")]
    ForcedUP,
    #[serde(rename = "allocating")]
    Allocating,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "metaDL")]
    MetaDL,
    #[serde(rename = "pausedDL")]
    PausedDL,
    #[serde(rename = "queuedDL")]
    QueuedDL,
    #[serde(rename = "stalledDL")]
    StalledDL,
    #[serde(rename = "checkingDL")]
    CheckingDL,
    #[serde(rename = "forcedDL")]
    ForceDL,
    #[serde(rename = "checkingResumeData")]
    CheckingResumeData,
    #[serde(rename = "moving")]
    Moving,
    #[serde(rename = "unknown")]
    Unknown,
}

#[derive(Debug, Deserialize, Getters)]
pub struct QbitTorrentInfo {
    added_on: u32,
    amount_left: u64,
    auto_tmm: bool,
    category: String,
    completed: i64,
    completion_on: u32,
    dl_limit: i64,
    dlspeed: u64,
    downloaded: i64,
    downloaded_session: i64,
    eta: i64,
    // will sometimes error if this is not option
    f_l_piece_prio: Option<bool>,
    force_start: bool,
    hash: String,
    last_activity: i64,
    magnet_uri: String,
    max_ratio: f64,
    max_seeding_time: i64,
    name: String,
    num_complete: i64,
    num_incomplete: i64,
    num_leechs: i64,
    num_seeds: i64,
    priority: i64,
    progress: f64,
    ratio: f64,
    ratio_limit: f64,
    save_path: String,
    seeding_time_limit: i64,
    seen_complete: i64,
    seq_dl: bool,
    size: i64,
    state: State,
    super_seeding: bool,
    tags: String,
    time_active: i64,
    total_size: i64,
    tracker: String,
    up_limit: i64,
    uploaded: i64,
    uploaded_session: i64,
    upspeed: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Builder, Default)]
#[builder(setter(into, strip_option))]
pub struct QbitTorrent {
    #[builder(default)]
    urls: Option<String>,
    #[builder(default)]
    torrents: Option<Vec<u8>>,
    #[builder(default)]
    savepath: Option<String>,
    #[builder(default)]
    cookie: Option<String>,
    #[builder(default)]
    skip_checking: Option<String>,
    #[builder(default)]
    paused: Option<String>,
    #[builder(default)]
    root_folder: Option<String>,
    #[builder(default)]
    rename: Option<String>,
    #[builder(default)]
    #[serde(rename = "upLimit")]
    upload_limit: Option<i64>,
    #[builder(default)]
    #[serde(rename = "dlLimit")]
    download_limit: Option<i64>,
}

pub struct QbitConfig {
    cookie: String,
    address: String,
    client: reqwest::Client,
}

impl RplClient for QbitConfig {}
impl RplPackConfig for QbitTorrent {}

impl QbitConfig {
    pub async fn new(username: &str, password: &str, address: &str) -> Result<Self, error::Error> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Referer", address.parse()?);

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        let response = client
            .get(&format!(
                "{}/api/v2/auth/login?username={}&password={}",
                address, username, password
            ))
            .send()
            .await?;

        let headers = match response.headers().get("set-cookie") {
            Some(header) => header,
            None => return Err(error::Error::MissingHeaders),
        };

        let cookie_str = headers.to_str()?;
        let cookie_header = match cookie_str.find(';') {
            Some(index) => index,
            None => return Err(error::Error::MissingCookie),
        };

        let cookie = match cookie_str.get(0..cookie_header) {
            Some(cookie) => cookie,
            None => return Err(error::Error::SliceError),
        };

        Ok(Self {
            cookie: cookie.to_string(),
            address: address.to_string(),
            client,
        })
    }

    pub async fn application_version(&self) -> Result<String, error::Error> {
        let res = self
            .client
            .get(&format!("{}/api/v2/app/version", self.address))
            .send()
            .await?
            .text()
            .await?;
        Ok(res)
    }

    fn make_headers(&self) -> Result<reqwest::header::HeaderMap, error::Error> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("cookie", self.cookie.parse()?);
        Ok(headers)
    }

    pub async fn add_new_torrent(&self, data: QbitTorrent) -> Result<(), error::Error> {
        let res = self
            .client
            .post(&format!("{}/api/v2/torrents/add", self.address))
            .multipart(data.build_form())
            .headers(self.make_headers()?)
            .send()
            .await?;

        match res.error_for_status() {
            Ok(_) => {
                debug!("Sleeping 500ms for qbittorrent to add the torrent...");
                sleep(Duration::from_millis(500)).await;
                Ok(())
            }
            Err(e) => Err(error::Error::from(e)),
        }
    }

    pub async fn set_priority(
        &self,
        hash: &str,
        files: &str,
        priority: u8,
    ) -> Result<(), error::Error> {
        let form = Form::new()
            .text("hash", hash.to_string())
            .text("id", files.to_string())
            .text("priority", priority.to_string());
        let res = self
            .client
            .post(&format!("{}/api/v2/torrents/filePrio", self.address))
            .multipart(form)
            .headers(self.make_headers()?)
            .send()
            .await?;

        match res.error_for_status() {
            Ok(_) => Ok(()),
            Err(e) => Err(error::Error::from(e)),
        }
    }

    pub async fn resume_torrent(&self, hash: &str) -> Result<(), error::Error> {
        let form = Form::new().text("hashes", hash.to_string());

        let res = self
            .client
            .post(&format!("{}/api/v2/torrents/resume", self.address))
            .multipart(form)
            .headers(self.make_headers()?)
            .send()
            .await?;

        match res.error_for_status() {
            Ok(_) => {
                debug!("Sleeping 1s for qbittorrent to resume the torrent...");
                sleep(Duration::from_millis(1000)).await;
                Ok(())
            }
            Err(e) => Err(error::Error::from(e)),
        }
    }

    pub async fn delete_torrent(&self, hash: &str, delete_files: bool) -> Result<(), error::Error> {
        let form = Form::new()
            .text("hashes", hash.to_string())
            .text("deleteFiles", delete_files.to_string());

        let res = self
            .client
            .post(&format!("{}/api/v2/torrents/delete", self.address))
            .multipart(form)
            .headers(self.make_headers()?)
            .send()
            .await?;

        match res.error_for_status() {
            Ok(_) => {
                debug!("Sleeping 500ms for qbittorrent to delete the torrent...");
                sleep(Duration::from_millis(500)).await;
                Ok(())
            }
            Err(e) => Err(error::Error::from(e)),
        }
    }

    pub async fn get_torrent_info(&self, hash: &str) -> Result<QbitTorrentInfo, error::Error> {
        let res = self
            .client
            .get(&format!(
                "{}/api/v2/torrents/info?hashes={}&limit=1",
                self.address, hash
            ))
            .headers(self.make_headers()?)
            .send()
            .await?
            .bytes()
            .await?;

        let all_torrents: Vec<QbitTorrentInfo> = serde_json::from_slice(&res)?;
        let ret_torrent = all_torrents.into_iter().next();
        match ret_torrent {
            Some(torrent_info) => Ok(torrent_info),
            None => Err(error::Error::QbitEmptyTorrentInfo),
        }
    }
}

impl QbitTorrent {
    fn build_form(self) -> Form {
        let mut form = Form::new();
        form = match self.urls {
            Some(urls) => form.text("urls", urls),
            None => form,
        };
        form = match self.torrents {
            Some(torrents) => form.part("torrents", Part::bytes(torrents).file_name("")),
            None => form,
        };
        form = match self.savepath {
            Some(savepath) => form.text("savepath", savepath),
            None => form,
        };
        form = match self.cookie {
            Some(cookie) => form.text("cookie", cookie),
            None => form,
        };
        form = match self.skip_checking {
            Some(skip_checking) => form.text("skip_checking", skip_checking),
            None => form,
        };
        form = match self.paused {
            Some(paused) => form.text("paused", paused),
            None => form,
        };
        form = match self.root_folder {
            Some(root_folder) => form.text("root_folder", root_folder),
            None => form,
        };
        form = match self.rename {
            Some(rename) => form.text("rename", rename),
            None => form,
        };
        form = match self.upload_limit {
            Some(upload_limit) => form.text("upLimit", upload_limit.to_string()),
            None => form,
        };
        form = match self.download_limit {
            Some(download_limit) => form.text("dlLimit", download_limit.to_string()),
            None => form,
        };
        form
    }

    pub fn url(mut self, url: String) -> Self {
        self.urls = Some(url);
        self
    }

    pub fn torrents(mut self, torrent: Torrent) -> Self {
        self.torrents = Some(
            torrent
                .encode()
                .expect("Could not encode Torrent to bencode. Is torrent file corrupted?"),
        );
        self
    }

    pub fn save_path(mut self, path: PathBuf) -> Self {
        self.savepath = Some(String::from(
            path.to_str().expect("Could not convert save path PathBuf"),
        ));
        self
    }

    pub fn skip_hash_checking(mut self, skip: bool) -> Self {
        self.skip_checking = match skip {
            true => Some(String::from("true")),
            false => Some(String::from("false")),
        };
        self
    }

    pub fn paused(mut self, paused: bool) -> Self {
        self.paused = match paused {
            true => Some(String::from("true")),
            false => Some(String::from("false")),
        };
        self
    }
}

#[async_trait]
impl<'a> RplLeech<'a, TorrentPack<'a>, QbitTorrent, QbitConfig> for TorrentPack<'a> {
    async fn leech_torrent(
        &'a mut self,
        torrent: Torrent,
        config: QbitTorrent,
        torrent_client: QbitConfig,
        upload_client: RcloneClient,
        seed: bool,
        seed_path: &'a str,
    ) -> Result<(), error::Error> {
        let hash = self.info_hash();

        info!(
            "The pack size is {}, maximum size per chunk is {}. Private torrent = {}.",
            &self.get_pack_size_human(),
            &self.get_max_size_chunk_human(),
            &self.is_private()
        );
        info!(
            "Qbittorrent App Version: {}",
            torrent_client.application_version().await?
        );

        let chunks = self.chunks()?;
        let queue = build_queue(chunks, torrent);
        let no_all_files = queue.no_all_files;
        let jobs = queue.job;
        let no_jobs = jobs.len();

        let mut offset = 0;

        for job in jobs {
            job.info();
            torrent_client.add_new_torrent(config.clone()).await?;
            let disable_others = &job.disable_others(offset, no_all_files);
            match disable_others {
                Some(disable_string) => {
                    torrent_client
                        .set_priority(&hash, disable_string, 0)
                        .await?;
                }
                None => (),
            }
            info!("Downloading chunk {}/{}", job.chunk, no_jobs);
            job.download(&torrent_client, &hash).await?;
            info!("Finished downloading chunk {}/{}", job.chunk, no_jobs);
            info!("Uploading chunk {}/{}", job.chunk, no_jobs);
            job.upload(&upload_client)?;
            info!("Finished uploading chunk {}/{}", job.chunk, no_jobs);

            torrent_client.delete_torrent(&hash, true).await?;

            offset += job.no_files;
        }

        if seed {
            info!("Adding the torrent back to qbittorrent for seeding through rclone's mount");
            let seed_config = config
                .skip_hash_checking(true)
                .save_path(PathBuf::from_str(seed_path).unwrap());
            torrent_client.add_new_torrent(seed_config).await?;
            torrent_client.resume_torrent(&hash).await?;
        }

        Ok(())
    }
}

#[async_trait]
trait RplQbit {
    fn disable_others(&self, offset: i32, no_all_files: i32) -> Option<String>;
    async fn download(&self, client: &QbitConfig, hash: &str) -> Result<(), error::Error>;
}

#[async_trait]
impl RplQbit for Job {
    fn disable_others(&self, offset: i32, no_all_files: i32) -> Option<String> {
        let mut disable_others = String::new();
        for i in 0..no_all_files {
            if i < offset || i >= offset + self.no_files {
                disable_others.push_str(&format!("{} | ", i));
            }
        }
        disable_others.truncate(disable_others.len() - 3);
        match disable_others.is_empty() {
            false => Some(disable_others),
            true => None,
        }
    }

    async fn download(&self, client: &QbitConfig, hash: &str) -> Result<(), error::Error> {
        client.resume_torrent(hash).await?;
        let size = self.total_size as u64;

        let pb = ProgressBar::new(size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} {msg} [{elapsed_precise}] [{bar:30.cyan/blue}] {bytes}/{total_bytes} [{binary_bytes_per_sec}] ({eta})")
            .progress_chars("#>-"));

        loop {
            let current_info = client.get_torrent_info(hash).await?;
            let state = current_info.state();
            match state {
                State::Error | State::PausedDL => return Err(error::Error::QbitTorrentErrored),
                State::Downloading => {
                    pb.set_message(format!("Downloading chunk {}", self.chunk));
                    pb.set_position(size - current_info.amount_left);
                }
                State::StalledDL => {
                    pb.set_message(format!("[Stalled] Downloading chunk {}", self.chunk));
                    pb.set_position(size - current_info.amount_left);
                }
                State::PausedUP | State::StalledUP | State::Uploading | State::QueuedUP => {
                    return Ok(())
                }
                _ => return Err(error::Error::QbitTorrentUnimplementedState),
            }

            sleep(Duration::from_millis(1000)).await;
        }
    }
}

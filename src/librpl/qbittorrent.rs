#![allow(dead_code)]
#![allow(unused_imports)]

use async_trait::async_trait;
use derive_builder::Builder;
use lava_torrent::torrent::v1::Torrent;
use log::{debug, info};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

use crate::librpl::torrent_parser::TorrentPack;
use crate::librpl::{build_queue, error, Job, RplChunk, RplClient, RplDownload, RplPackConfig};

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
        let cookie_header = match cookie_str.find(";") {
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

    pub async fn resume_torrent(&self, hash: String) -> Result<(), error::Error> {
        let form = Form::new().text("hash", hash);

        let res = self
            .client
            .post(&format!("{}/api/v2/torrents/resume", self.address))
            .multipart(form)
            .headers(self.make_headers()?)
            .send()
            .await?;

        match res.error_for_status() {
            Ok(_) => Ok(()),
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
impl<'a> RplDownload<'a, TorrentPack<'a>, QbitTorrent, QbitConfig> for TorrentPack<'a> {
    async fn download_torrent(
        &'a mut self,
        torrent: Torrent,
        config: QbitTorrent,
        client: QbitConfig,
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
            client.application_version().await?
        );

        let chunks = self.chunks()?;
        let queue = build_queue(chunks, torrent);
        let no_all_files = queue.no_all_files;
        let jobs = queue.job;

        let mut offset = 0;

        for job in jobs {
            job.info();
            client.add_new_torrent(config.clone()).await?;
            client
                .set_priority(&hash, &job.disable_others(offset, no_all_files), 0)
                .await?;
            info!("Downloading chunk {}", job.chunk);
            sleep(Duration::from_secs(5)).await;
            info!("Finished chunk {}", job.chunk);

            client.delete_torrent(&hash, false).await?;

            offset += job.no_files;
        }
        Ok(())
    }
}

trait RplQbit {
    fn disable_others(&self, offset: i32, no_all_files: i32) -> String;
}

impl RplQbit for Job {
    fn disable_others(&self, offset: i32, no_all_files: i32) -> String {
        let mut disable_others = String::new();
        for i in 0..no_all_files {
            if i < offset || i >= offset + self.no_files {
                disable_others.push_str(&format!("{} | ", i));
            }
        }
        disable_others.truncate(disable_others.len() - 3);
        disable_others
    }
}

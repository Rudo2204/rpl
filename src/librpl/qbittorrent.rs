#![allow(dead_code)]
#![allow(unused_imports)]

use crate::librpl::error;
use derive_builder::Builder;
use lava_torrent::torrent::v1::Torrent;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Builder, Default)]
#[builder(setter(into, strip_option))]
pub struct TorrentDownload {
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

    pub async fn add_new_torrent(&self, data: TorrentDownload) -> Result<(), error::Error> {
        let res = self
            .client
            .post(&format!("{}/api/v2/torrents/add", self.address))
            .multipart(data.build_form())
            .headers(self.make_headers()?)
            .send()
            .await?;

        match res.error_for_status() {
            Ok(_) => Ok(()),
            Err(e) => Err(error::Error::from(e)),
        }
    }

    pub async fn set_priority(
        &self,
        hash: String,
        files: String,
        priority: u8,
    ) -> Result<(), error::Error> {
        let form = Form::new()
            .text("hash", hash)
            .text("id", files)
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
}

impl TorrentDownload {
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

    pub async fn download(self, qbit: &QbitConfig) -> Result<(), error::Error> {
        qbit.add_new_torrent(self).await
    }

    pub fn url(mut self, url: String) -> Self {
        self.urls = Some(url);
        self
    }

    pub fn torrents(mut self, torrent: Torrent) -> Self {
        self.torrents = Some(
            torrent
                .encode()
                .expect("Could not encode Torrent to bencode Vec u8"),
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

#![allow(dead_code)]
#![allow(unused_imports)]

use crate::librpl::error;

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
}

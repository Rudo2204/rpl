use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{ChildStderr, Command, Stdio};

use crate::librpl::error;

// rclone copy --stats 1s --use-json-log --verbose --bwlimit 50k <src> <dst> 3>&1 2>&3- | jq
#[derive(Debug, Deserialize)]
struct RcloneCopyResp {
    level: String,
    msg: String,
    source: String,
    stats: RcloneStatsResp,
    // Can be deser to Chrono::Datetime using Datetime::parse_from_rfc3339(self.time).unwrap();
    time: String,
}

#[derive(Debug, Deserialize)]
struct RcloneStatsResp {
    bytes: u64,
    checks: u32,
    #[serde(rename = "deletedDirs")]
    deleted_dirs: u32,
    deletes: u32,
    #[serde(rename = "elapsedTime")]
    elapsed_time: f32,
    errors: u32,
    eta: Option<u64>,
    #[serde(rename = "fatalError")]
    fatal_error: bool,
    renames: u32,
    #[serde(rename = "retryError")]
    retry_error: bool,
    speed: f32,
    #[serde(rename = "totalBytes")]
    total_bytes: u64,
    #[serde(rename = "totalChecks")]
    total_checks: u32,
    #[serde(rename = "totalTransfers")]
    total_transfers: u64,
    #[serde(rename = "transferTime")]
    transfer_time: f32,
    transferring: Option<Vec<RcloneTransferring>>,
    transfers: u32,
}

#[derive(Debug, Deserialize)]
struct RcloneTransferring {
    name: String,
    size: u64,
    bytes: Option<u64>,
    eta: Option<u64>,
    group: Option<String>,
    percentage: Option<u8>,
    speed: Option<f32>,
    #[serde(rename = "speedAvg")]
    speed_avg: Option<f32>,
}

#[derive(Debug)]
pub struct RcloneClient {
    source: PathBuf,
    destination: String,
    transfers: u16,
}

impl RcloneClient {
    pub fn new(source: PathBuf, destination: String, transfers: u16) -> Self {
        Self {
            source,
            destination,
            transfers,
        }
    }

    fn build_stderr_capture(&self) -> Result<ChildStderr, error::Error> {
        let stderr = Command::new("rclone")
            .arg("copy")
            .arg("--verbose")
            .arg("--stats")
            .arg("1s")
            .arg("--use-json-log")
            .arg("--transfers")
            .arg(self.transfers.to_string())
            // TODO: check this unwrap to make it safe
            .arg(&self.source.to_str().unwrap())
            // TODO: check this dst to make it safe
            .arg(&self.destination)
            .stderr(Stdio::piped())
            .spawn()?
            .stderr;

        match stderr {
            Some(stderr) => Ok(stderr),
            None => Err(error::Error::RcloneStderrCaptureError),
        }
    }

    pub fn upload(&self) -> Result<(), error::Error> {
        let stderr = self.build_stderr_capture()?;
        let reader = BufReader::new(stderr);

        reader
            .lines()
            .filter_map(|line| line.ok())
            .filter(|line| line.find("ETA").is_some())
            .for_each(|line| {
                let resp: RcloneCopyResp = serde_json::from_str(&line).unwrap();
                println!("{:?}", resp);
            });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rather expensive upload test with hardcoded values
    // only run manually in development
    #[test]
    #[ignore]
    fn upload() {
        let rclone_client = RcloneClient::new(
            PathBuf::from(
                shellexpand::full("~/rclone uploadme")
                    .expect("Could not find the correct path to saved data")
                    .into_owned(),
            ),
            String::from("gdrive:/rpl_test"),
            4,
        );

        rclone_client.upload().unwrap();
    }

    #[test]
    fn deser() {
        let limiter_json = r#"{"level":"info","msg":"Starting bandwidth limiter at 5MBytes/s","source":"accounting/token_bucket.go:95","time":"2021-06-07T08:38:21.80782+07:00"}"#;

        match limiter_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(limiter_json).unwrap();
            }
            None => {
                println!("Found no ETA in limiter_json, skipping");
            }
        }

        let waiting_json = r#"{"level":"info","msg":"\nTransferred:   \t         0 / 0 Bytes, -, 0 Bytes/s, ETA -\nTransferred:            0 / 1, 0%\nElapsed time:         5.3s\nTransferring:\n *                                   brazjson.7z: transferring\n\n","source":"accounting/stats.go:417","stats":{"bytes":0,"checks":0,"deletedDirs":0,"deletes":0,"elapsedTime":5.320457947,"errors":0,"eta":null,"fatalError":false,"renames":0,"retryError":false,"speed":0,"totalBytes":0,"totalChecks":0,"totalTransfers":1,"transferTime":3.419050329,"transferring":[{"name":"brazjson.7z","size":14067793}],"transfers":0},"time":"2021-06-07T08:38:27.083348+07:00"}"#;

        match waiting_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(waiting_json).unwrap();
            }
            None => {
                println!("Found no ETA in waiting_json, skipping");
            }
        }

        let transferring_json = r#"{"level":"info","msg":"\nTransferred:   \t   11.996M / 13.416 MBytes, 89%, 390.982 kBytes/s, ETA 3s\nTransferred:            0 / 1, 0%\nElapsed time:        33.3s\nTransferring:\n *                                   brazjson.7z: 89% /13.416M, 6.991M/s, 0s\n\n","source":"accounting/stats.go:417","stats":{"bytes":12578816,"checks":0,"deletedDirs":0,"deletes":0,"elapsedTime":33.319626717,"errors":0,"eta":3,"fatalError":false,"renames":0,"retryError":false,"speed":400366.9062339992,"totalBytes":14067793,"totalChecks":0,"totalTransfers":1,"transferTime":31.418221147,"transferring":[{"bytes":12578816,"eta":0,"group":"global_stats","name":"brazjson.7z","percentage":89,"size":14067793,"speed":6519462.239440185,"speedAvg":7330192.215314117}],"transfers":0},"time":"2021-06-07T08:38:55.082601+07:00"}"#;

        match transferring_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(transferring_json).unwrap();
            }
            None => {
                println!("Found no ETA in transferring_json, skipping");
            }
        }

        let finished_json = r#"{"level":"info","msg":"\nTransferred:   \t   13.416M / 13.416 MBytes, 100%, 433.291 kBytes/s, ETA 0s\nTransferred:            1 / 1, 100%\nElapsed time:        33.6s\n\n","source":"accounting/stats.go:417","stats":{"bytes":14067793,"checks":0,"deletedDirs":0,"deletes":0,"elapsedTime":33.607828572,"errors":0,"eta":0,"fatalError":false,"renames":0,"retryError":false,"speed":443690.55219237105,"totalBytes":14067793,"totalChecks":0,"totalTransfers":1,"transferTime":31.706316329,"transfers":1},"time":"2021-06-07T08:38:55.370816+07:00"}"#;

        match finished_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(finished_json).unwrap();
            }
            None => {
                println!("Found no ETA in finished_json, skipping");
            }
        }
    }
}

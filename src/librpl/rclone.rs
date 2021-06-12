use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::convert::TryInto;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{ChildStderr, Command, Stdio};

use crate::librpl::error;
use crate::librpl::{Job, RplUpload};

// rclone copy --stats 1s --use-json-log --verbose <src> <dst> 3>&1 2>&3- | tee -a log
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
    pub bytes: u64,
    checks: u32,
    #[serde(rename = "deletedDirs")]
    deleted_dirs: u32,
    deletes: u32,
    #[serde(rename = "elapsedTime")]
    elapsed_time: f32,
    errors: u32,
    pub eta: Option<u64>,
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
    variant: String,
    source: PathBuf,
    destination: String,
    transfers: u16,
    drive_chunk_size: u16,
}

impl RplUpload for Job {
    fn upload(&self, client: &RcloneClient) -> Result<(), error::Error> {
        let stderr = client.build_stderr_capture()?;
        let reader = BufReader::new(stderr);

        let pb = ProgressBar::new(
            self.total_size
                .try_into()
                .expect("Torrent size is negative?"),
        );
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} {msg} [{elapsed_precise}] [{bar:30.cyan/blue}] {bytes}/{total_bytes} [{binary_bytes_per_sec}] ({eta})")
            .progress_chars("#>-"));

        pb.set_message(format!("Waiting for {}", client.variant));

        reader
            .lines()
            .filter_map(|line| line.ok())
            .filter(|line| line.contains("ETA"))
            .for_each(|line| {
                let resp: RcloneCopyResp = serde_json::from_str(&line).unwrap();
                if let Some(_eta) = resp.stats.eta {
                    pb.set_message(format!("Uploading chunk {}", self.chunk));
                    pb.set_position(resp.stats.bytes);
                }
            });

        Ok(())
    }
}

impl RcloneClient {
    pub fn new(
        variant: String,
        source: PathBuf,
        destination: String,
        transfers: u16,
        drive_chunk_size: u16,
    ) -> Self {
        Self {
            variant,
            source,
            destination,
            transfers,
            drive_chunk_size,
        }
    }

    // TODO: implement a trait instead of hardcoding for qbittorrent like this
    fn build_stderr_capture(&self) -> Result<ChildStderr, error::Error> {
        let stderr = Command::new(self.variant.to_owned())
            .arg("copy")
            .arg("--exclude")
            .arg("*.parts")
            .arg("--verbose")
            .arg("--stats")
            .arg("1s")
            .arg("--use-json-log")
            .arg("--transfers")
            .arg(self.transfers.to_string())
            .arg("--drive_chunk_size")
            .arg(self.drive_chunk_size.to_string())
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deser() {
        let limiter_json = r#"{"level":"info","msg":"Starting bandwidth limiter at 5MBytes/s","source":"accounting/token_bucket.go:95","time":"2021-06-07T08:38:21.80782+07:00"}"#;

        match limiter_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(limiter_json).unwrap();
            }
            None => (),
        }

        let waiting_json = r#"{"level":"info","msg":"\nTransferred:   \t         0 / 0 Bytes, -, 0 Bytes/s, ETA -\nTransferred:            0 / 1, 0%\nElapsed time:         5.3s\nTransferring:\n *                                   brazjson.7z: transferring\n\n","source":"accounting/stats.go:417","stats":{"bytes":0,"checks":0,"deletedDirs":0,"deletes":0,"elapsedTime":5.320457947,"errors":0,"eta":null,"fatalError":false,"renames":0,"retryError":false,"speed":0,"totalBytes":0,"totalChecks":0,"totalTransfers":1,"transferTime":3.419050329,"transferring":[{"name":"brazjson.7z","size":14067793}],"transfers":0},"time":"2021-06-07T08:38:27.083348+07:00"}"#;

        match waiting_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(waiting_json).unwrap();
            }
            None => (),
        }

        let transferring_json = r#"{"level":"info","msg":"\nTransferred:   \t   11.996M / 13.416 MBytes, 89%, 390.982 kBytes/s, ETA 3s\nTransferred:            0 / 1, 0%\nElapsed time:        33.3s\nTransferring:\n *                                   brazjson.7z: 89% /13.416M, 6.991M/s, 0s\n\n","source":"accounting/stats.go:417","stats":{"bytes":12578816,"checks":0,"deletedDirs":0,"deletes":0,"elapsedTime":33.319626717,"errors":0,"eta":3,"fatalError":false,"renames":0,"retryError":false,"speed":400366.9062339992,"totalBytes":14067793,"totalChecks":0,"totalTransfers":1,"transferTime":31.418221147,"transferring":[{"bytes":12578816,"eta":0,"group":"global_stats","name":"brazjson.7z","percentage":89,"size":14067793,"speed":6519462.239440185,"speedAvg":7330192.215314117}],"transfers":0},"time":"2021-06-07T08:38:55.082601+07:00"}"#;

        match transferring_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(transferring_json).unwrap();
            }
            None => (),
        }

        let finished_json = r#"{"level":"info","msg":"\nTransferred:   \t   13.416M / 13.416 MBytes, 100%, 433.291 kBytes/s, ETA 0s\nTransferred:            1 / 1, 100%\nElapsed time:        33.6s\n\n","source":"accounting/stats.go:417","stats":{"bytes":14067793,"checks":0,"deletedDirs":0,"deletes":0,"elapsedTime":33.607828572,"errors":0,"eta":0,"fatalError":false,"renames":0,"retryError":false,"speed":443690.55219237105,"totalBytes":14067793,"totalChecks":0,"totalTransfers":1,"transferTime":31.706316329,"transfers":1},"time":"2021-06-07T08:38:55.370816+07:00"}"#;

        match finished_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(finished_json).unwrap();
            }
            None => (),
        }
    }
}

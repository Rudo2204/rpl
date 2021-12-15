use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{ChildStderr, Command, Stdio};

use crate::librpl::error;
use crate::librpl::{Job, RplUpload};

// rclone copy --stats 1s --use-json-log --verbose <src> <dst> 3>&1 2>&3- | tee -a log
#[derive(Debug, Serialize, Deserialize)]
struct RcloneCopyResp {
    level: Option<String>,
    msg: Option<String>,
    source: Option<String>,
    stats: Option<RcloneStatsResp>,
    // Can be deser to Chrono::Datetime using Datetime::parse_from_rfc3339(self.time).unwrap();
    time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RcloneStatsResp {
    bytes: u64,
    checks: Option<u32>,
    #[serde(rename = "deletedDirs")]
    deleted_dirs: Option<u32>,
    deletes: Option<u32>,
    #[serde(rename = "elapsedTime")]
    elapsed_time: Option<f64>,
    errors: Option<u32>,
    eta: Option<f64>,
    #[serde(rename = "fatalError")]
    fatal_error: Option<bool>,
    renames: Option<u32>,
    #[serde(rename = "retryError")]
    retry_error: Option<bool>,
    speed: Option<f64>,
    #[serde(rename = "totalBytes")]
    total_bytes: Option<u64>,
    #[serde(rename = "totalChecks")]
    total_checks: Option<u32>,
    #[serde(rename = "totalTransfers")]
    total_transfers: Option<u64>,
    #[serde(rename = "transferTime")]
    transfer_time: Option<f64>,
    transferring: Option<Vec<RcloneTransferring>>,
    transfers: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RcloneTransferring {
    name: Option<String>,
    size: Option<u64>,
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
    pub variant: String,
    source: PathBuf,
    destination: String,
    transfers: u16,
    drive_chunk_size: u16,
    extra_custom_flags: Vec<String>,
}

impl RplUpload for Job {
    fn upload(&self, client: &RcloneClient, no_jobs: usize) -> Result<(), error::Error> {
        let stderr = client.build_stderr_capture(&client.extra_custom_flags)?;
        let reader = BufReader::new(stderr);

        let pb = ProgressBar::new(
            self.total_size
                .try_into()
                .expect("Torrent size is negative?"),
        );
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} {msg} [{elapsed_precise}] [{bar:20.cyan/blue}] {bytes}/{total_bytes} [{binary_bytes_per_sec}] ({eta})")
            .progress_chars("#>-"));

        pb.set_message(format!("Waiting for {}", client.variant));

        reader
            .lines()
            .filter_map(|line| line.ok())
            .filter(|line| line.contains("ETA"))
            .for_each(|line| {
                let resp: RcloneCopyResp = serde_json::from_str(&line).unwrap();
                if let Some(stats) = resp.stats {
                    if let Some(speed) = stats.speed {
                        if speed > 0f64 {
                            pb.set_message(format!("Uploading chunk {}/{}", self.chunk, no_jobs));
                            pb.set_position(stats.bytes);
                        }
                    }
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
        extra_custom_flags: Vec<String>,
    ) -> Self {
        Self {
            variant,
            source,
            destination,
            transfers,
            drive_chunk_size,
            extra_custom_flags,
        }
    }

    // TODO: implement a trait instead of hardcoding for qbittorrent like this
    fn build_stderr_capture(&self, extra_args: &[String]) -> Result<ChildStderr, error::Error> {
        let stderr = Command::new(self.variant.to_owned())
            .arg("copy")
            .arg("--exclude")
            .arg("*.parts")
            .arg("--exclude")
            .arg("*.!qB")
            .arg("--verbose")
            .arg("--stats")
            .arg("1s")
            .arg("--use-json-log")
            .arg("--transfers")
            .arg(self.transfers.to_string())
            .arg("--drive-chunk-size")
            .arg(format!("{}M", self.drive_chunk_size))
            .args(extra_args)
            .arg(&self.source.to_str().unwrap())
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

        let gclone_json = r#"{"level":"info","msg":"\nTransferred:   \t         0 / 26.353 MBytes, 0%, 0 Bytes/s, ETA -\nTransferred:            0 / 6, 0%, 0.00 Files/s\nElapsed time:         1.4s\nTransferring:\n * MP3-daily-2021-June-11…-keep_looking_down.mp3:  0% /8.424M, 0/s, -\n * MP3-daily-2021-June-11…king_down-web-2021.m3u:  0% /96, 0/s, -\n * MP3-daily-2021-June-11…king_down-web-2021.nfo:  0% /813, 0/s, -\n * MP3-daily-2021-June-11…own-web-2021-cover.jpg:  0% /1.567M, 0/s, -\n\n","source":"accounting/stats.go:388","stats":{"bytes":0,"checks":0,"deletes":0,"elapsedTime":1.479027341,"errors":0,"fatalError":false,"renames":0,"retryError":false,"speed":0,"transferring":[{"bytes":0,"eta":null,"group":"global_stats","name":"MP3-daily-2021-June-11-Synthpop/Ritual_Veil-Keep_Looking_Down-WEB-2021-AMOK/00-ritual_veil-keep_looking_down-web-2021-cover.jpg","percentage":0,"size":1643197,"speed":0,"speedAvg":0},{"bytes":0,"eta":null,"group":"global_stats","name":"MP3-daily-2021-June-11-Synthpop/Ritual_Veil-Keep_Looking_Down-WEB-2021-AMOK/00-ritual_veil-keep_looking_down-web-2021.m3u","percentage":0,"size":96,"speed":0,"speedAvg":0},{"bytes":0,"eta":null,"group":"global_stats","name":"MP3-daily-2021-June-11-Synthpop/Ritual_Veil-Keep_Looking_Down-WEB-2021-AMOK/00-ritual_veil-keep_looking_down-web-2021.nfo","percentage":0,"size":813,"speed":0,"speedAvg":0},{"bytes":0,"eta":null,"group":"global_stats","name":"MP3-daily-2021-June-11-Synthpop/Ritual_Veil-Keep_Looking_Down-WEB-2021-AMOK/01-ritual_veil-keep_looking_down.mp3","percentage":0,"size":8833095,"speed":0,"speedAvg":0}],"transfers":0},"time":"2021-06-13T01:00:52.115318+07:00"}"#;

        match gclone_json.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(gclone_json).unwrap();
            }
            None => (),
        }

        let gclone_finish = r#"{"level":"info","msg":"\nTransferred:   \t   26.353M / 26.353 MBytes, 100%, 3.640 MBytes/s, ETA 0s\nTransferred:            6 / 6, 100%, 0.83 Files/s\nElapsed time:         7.2s\n\n","source":"accounting/stats.go:388","stats":{"bytes":27633266,"checks":0,"deletes":0,"elapsedTime":7.240487058,"errors":0,"fatalError":false,"renames":0,"retryError":false,"speed":3816492.69982025,"transfers":6},"time":"2021-06-13T01:00:57.876773+07:00"}"#;

        match gclone_finish.find("ETA") {
            Some(_pos) => {
                let _resp: RcloneCopyResp = serde_json::from_str(gclone_finish).unwrap();
            }
            None => (),
        }
    }
}

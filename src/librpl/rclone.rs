use chrono::serde::ts_milliseconds;
use chrono::DateTime;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{ChildStderr, Command, Stdio};

pub use crate::librpl::error;

// rclone copy --stats 1s --use-json-log --verbose --bwlimit 50k <src> <dst> 3>&1 2>&3- | jq
#[derive(Debug, Deserialize)]
struct RcloneCopyResp {
    level: String,
    msg: String,
    source: String,
    stats: RcloneStatsResp,
    #[serde(with = "ts_milliseconds")]
    time: DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct RcloneStatsResp {
    bytes: u64,
    checks: u32,
    deleted_dirs: u32,
    deletes: u32,
    elapsed_time: u64,
    errors: u32,
    eta: Option<u64>,
    fatal_error: bool,
    renames: u32,
    retry_error: bool,
    speed: f32,
    total_bytes: u64,
    total_checks: u32,
    total_transfers: u64,
    transfer_time: f32,
    transfering: Option<Vec<RcloneTransferring>>,
    transfers: u32,
}

#[derive(Debug, Deserialize)]
struct RcloneTransferring {
    name: String,
    size: u64,
    bytes: u64,
    eta: Option<u64>,
    group: String,
    percentage: u8,
    speed: f32,
    speed_avg: f32,
}

pub struct RcloneClient {
    source: PathBuf,
    destination: String,
    transfers: u16,
}

impl RcloneClient {
    fn new(source: PathBuf, destination: String, transfers: u16) -> Self {
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
            .arg("--stats 1s")
            .arg("--use-json-log")
            .arg(format!("--transfers {}", self.transfers))
            // TODO: check this unwrap to make it safe
            .arg(&self.source.to_str().unwrap())
            // TODO: check this dst to make it safe
            .arg(&self.destination)
            .stderr(Stdio::piped())
            .spawn()?
            .stderr
            .take();

        match stderr {
            Some(stderr) => Ok(stderr),
            None => Err(error::Error::RcloneStderrCaptureError),
        }
    }

    fn upload(&self) -> Result<(), error::Error> {
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

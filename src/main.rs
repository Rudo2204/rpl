use anyhow::Result;
use chrono::{Local, Utc};
//use clap::{crate_authors, crate_description, crate_version, App, AppSettings, Arg};
use derive_getters::Getters;
use fern::colors::{Color, ColoredLevelConfig};
use fs2::FileExt;
use log::{debug, LevelFilter};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{stdout, Read, Write};
use std::path::{Path, PathBuf};

mod librpl;
use librpl::util;

use librpl::error;
use librpl::qbittorrent::{QbitConfig, QbitTorrent};
use librpl::rclone::RcloneClient;
use librpl::torrent_parser::TorrentPack;
use librpl::RplLeech;

pub const PROGRAM_NAME: &str = "rpl";

use lava_torrent::torrent::v1::Torrent;

fn setup_logging(verbosity: u64, chain: bool) -> Result<()> {
    let colors_line = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::Blue)
        .trace(Color::BrightBlack); // this is the same as the background color

    let mut base_config = fern::Dispatch::new();

    base_config = match verbosity {
        0 => base_config
            .level(LevelFilter::Warn)
            .level_for(PROGRAM_NAME, LevelFilter::Warn),
        1 => base_config
            .level(LevelFilter::Info)
            .level_for(PROGRAM_NAME, LevelFilter::Info),
        2 => base_config
            .level(LevelFilter::Info)
            .level_for(PROGRAM_NAME, LevelFilter::Debug),
        _3_or_more => base_config.level(LevelFilter::Trace),
    };

    // Separate file config so we can include year, month and day (UTC) in file logs
    let log_file_path =
        util::get_data_dir("", "", PROGRAM_NAME)?.join(format!("{}.log", PROGRAM_NAME));
    let file_config = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{date} {colored_level} {colored_target} > {colored_message}",
                date = Utc::now().format("%Y-%m-%dT%H:%M:%SUTC"),
                colored_level = format_args!(
                    "\x1B[{}m{}\x1B[0m",
                    colors_line.get_color(&record.level()).to_fg_str(),
                    record.level()
                ),
                colored_target = format_args!("\x1B[95m{}\x1B[0m", record.target()),
                colored_message = format_args!(
                    "\x1B[{}m{}\x1B[0m",
                    colors_line.get_color(&record.level()).to_fg_str(),
                    message
                ),
            ))
        })
        .chain(fern::log_file(log_file_path)?);

    // For stdout output we will just output local %H:%M:%S
    let stdout_config = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{date} {colored_level} > {colored_message}",
                date = Local::now().format("%H:%M:%S"),
                colored_level = format_args!(
                    "\x1B[{}m{}\x1B[0m",
                    colors_line.get_color(&record.level()).to_fg_str(),
                    record.level()
                ),
                colored_message = format_args!(
                    "\x1B[{}m{}\x1B[0m",
                    colors_line.get_color(&record.level()).to_fg_str(),
                    message
                ),
            ))
        })
        .chain(stdout());

    if chain {
        base_config
            .chain(file_config)
            .chain(stdout_config)
            .apply()?;
    } else {
        base_config.chain(stdout_config).apply()?;
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Getters)]
struct Config {
    rpl: RplConfig,
    qbittorrent: RplQbitConfig,
    rclone: RplRcloneConfig,
}

#[derive(Serialize, Deserialize, Getters)]
struct RplConfig {
    max_size_percentage: u8,
    max_size: String,
    torrent_client: String,
    rclone_client: String,
    save_path: String,
    remote_path: String,
    ignore_warning: bool,
}

#[derive(Serialize, Deserialize, Getters)]
struct RplQbitConfig {
    username: String,
    password: String,
    address: String,
}

#[derive(Serialize, Deserialize, Getters)]
struct RplRcloneConfig {
    transfers: u16,
}

impl Default for Config {
    fn default() -> Self {
        let stock_config = r#"[rpl]
# rpl will use this percentage of available disk space as max_size
# value range: 1-100, or 0 to use max_size value instead
max_size_percentage = 0
# max_size value allowed, if max_size_percentage is a positive number
# then this field will have no effect
max_size = "5 GiB"
# only qbittorrent is available atm
torrent_client = "qbittorrent"
# rclone or any rclone's variant (fclone, gclone) used for uploading
rclone_client = "rclone"
# temporary data from pack will be saved to here
# this directory should be dedicated for rpl
save_path = ""
# rclone remote path for uploading. Example: "nugu:/rpl"
remote_path = ""
# Skip files that have size larger than max_size
ignore_warning = false

[qbittorrent]
# default username of qbittorrent WEB UI
username = "admin"
# default password of qbittorrent WEB UI
password = "adminadmin"
# default address of qbittorrent WEB UI
address = "http://localhost:8080"

[rclone]
# default transfers of rclone
transfers = 4"#;

        let config = Config::from_config(stock_config);
        config
    }
}

impl Config {
    fn from_config(config_string: &str) -> Self {
        let config: Config = toml::from_str(config_string).expect("Could not parse config file");
        config
    }

    fn write_config(&self) {
        let config_dir = util::get_conf_dir("", "", PROGRAM_NAME).unwrap();
        let mut file = OpenOptions::new().write(true).open(config_dir).unwrap();
        writeln!(file, "{}", toml::to_string(&self).unwrap())
            .expect("Could not write config to file, maybe there is a permission error?");
    }

    fn save_path_invalid(&self) -> bool {
        let save_path = &self.rpl.save_path;
        if save_path.is_empty() {
            return true;
        } else {
            let path = Path::new(save_path);
            if !path.exists() {
                debug!("{} does not exist. I will create it now", path.display());
                fs::create_dir_all(path).unwrap();
            }
            return false;
        }
    }

    fn remote_path_invalid(&self) -> bool {
        self.rpl.remote_path.is_empty()
    }

    fn max_size_percentage_used(&self) -> Result<bool, error::Error> {
        // Can't use match here because https://github.com/rust-lang/rust/issues/37854
        let tmp = self.rpl.max_size_percentage;
        return if tmp == 0 {
            Ok(false)
        } else if tmp > 0 && tmp <= 100 {
            Ok(true)
        } else {
            Err(error::Error::InvalidMaxSizePercentage)
        };
    }

    fn max_size_allow_invalid(&self) -> Result<bool, error::Error> {
        let _size = parse_size::parse_size(&self.rpl.max_size()).unwrap();
        Ok(false)
    }
}

fn get_rpl_config() -> Result<Config, error::Error> {
    let conf_file = util::get_conf_dir("", "", PROGRAM_NAME).unwrap();

    let config: Config;
    if !conf_file.exists() {
        util::create_proj_conf("", "", PROGRAM_NAME).unwrap();
        config = Config::default();
        config.write_config();
    } else {
        let s = fs::read_to_string(&conf_file).unwrap();
        config = Config::from_config(&s);
    }

    if config.save_path_invalid()
        || config.remote_path_invalid()
        || config.max_size_percentage_used().is_err()
        || config.max_size_allow_invalid().is_err()
    {
        return Err(error::Error::InvalidRplConfig);
    }

    Ok(config)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let chain = true;
    let verbosity: u64 = 1; //matches.occurrences_of("verbose");
    let max_size_allow: i64 = (1.3_f32 * (u32::pow(1024, 3) as f32)) as i64;
    let data_dir = util::get_data_dir("", "", PROGRAM_NAME)?;
    util::create_data_dir(&data_dir)?;

    setup_logging(verbosity, chain)?;
    let log_file_path =
        util::get_data_dir("", "", PROGRAM_NAME)?.join(format!("{}.log", PROGRAM_NAME));
    let log_file = File::open(log_file_path)?;
    log_file.lock_exclusive()?;
    debug!("-----Logger is initialized. Starting main program!-----");

    let _config = get_rpl_config()?;

    let mut torrent_file =
        File::open("[ReinForce] Maoujou de Oyasumi (BDRip 1920x1080 x264 FLAC).torrent")?;
    let mut raw_torrent = Vec::new();
    torrent_file.read_to_end(&mut raw_torrent)?;

    let mut pack_config = TorrentPack::new(Torrent::read_from_bytes(&raw_torrent).unwrap(), false)
        .max_size(max_size_allow);

    let addr = "http://localhost:7070";
    let qbit = QbitConfig::new("", "", addr).await?;

    let torrent_config = QbitTorrent::default()
        .torrents(Torrent::read_from_bytes(&raw_torrent).unwrap())
        .paused(true)
        .save_path(PathBuf::from(
            shellexpand::full("~/Videos/")
                .expect("Could not find the correct path to save data")
                .into_owned(),
        ));

    let rclone_client = RcloneClient::new(
        String::from("rclone"),
        PathBuf::from(
            shellexpand::full("~/rclone uploadme")
                .expect("Could not find the correct path to saved data")
                .into_owned(),
        ),
        String::from("gdrive:/rpl_test"),
        4,
    );

    pack_config
        .leech_torrent(
            Torrent::read_from_bytes(&raw_torrent).unwrap(),
            torrent_config,
            qbit,
            rclone_client,
        )
        .await?;

    debug!("-----Everything is finished!-----");
    log_file.unlock()?;
    Ok(())
}

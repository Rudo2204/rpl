use anyhow::Result;
use chrono::{Local, Utc};
use clap::{
    crate_authors, crate_description, crate_version, value_t, App, AppSettings, Arg, ArgMatches,
};
use derive_getters::Getters;
use fern::colors::{Color, ColoredLevelConfig};
use fs2::FileExt;
use humansize::{file_size_opts, FileSize};
use lava_torrent::torrent::v1::Torrent;
use log::{debug, error, info, warn, LevelFilter};
use parse_size::parse_size;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{stdout, Read, Write};
use std::path::{Path, PathBuf};

mod librpl;
use librpl::util;

use librpl::error;
use librpl::qbittorrent::{QbitConfig, QbitTorrent};
use librpl::rclone::RcloneClient;
use librpl::torrent_parser::{get_largest_filesize, TorrentPack};
use librpl::{RplLeech, SeedSettings};

pub const PROGRAM_NAME: &str = "rpl";
const STOCK_CONFIG: &str = r#"[rpl]
# rpl will use this percentage of available disk space as max_size
# value range: 1-100, or 0 to use max_size value instead (recommended)
max_size_percentage = 0
# maximum size per chunk allowed for rpl. The bigger the value, the faster the download speed
# if max_size_percentage is > 0 then this field will have no effect
max_size = "5 GiB"
# only qbittorrent is available at the moment
torrent_client = "qbittorrent"
# rclone or other rclone's variants (fclone, gclone, xclone) used for uploading
upload_client = "rclone"
# [REQUIRED] temporary data from pack will be saved to here
# this directory should be dedicated for rpl
save_path = ""
# [REQUIRED] rclone remote path for uploading. Example: "nugu:/rpl"
remote_path = ""
# Force rpl to skip files that have size larger than max_size
ignore_warning = false

[seed_settings]
# set to true to seed the torrent through rclone's mount after rpl finishes
seed_enable = false
# set the rclone's mount path (remote mount path should be the same as remote_path)
# Example: `rclone mount ... nugu:/rpl ~/mount` then seed_path should be `~/mount`
seed_path = ""
# number of seconds to wait for rclone's mount to refresh the mount point
# should be at least equal or bigger than the value of --poll-interval
seed_wait = 20

[qbittorrent]
# username of qbittorrent Web UI
username = "admin"
# password of qbittorrent Web UI
password = "adminadmin"
# address of qbittorrent Web UI
address = "http://localhost:8080"
# upload_limit for torrents added (0 for unlimited) (unit: bytes/second)
upload_limit = 0
# download_limit for torrents added (0 for unlimited) (unit: bytes/second)
download_limit = 0

[rclone]
# default transfers of rclone
transfers = 8
# default drive chunk size (unit is MiB)
# Note: with default rpl's setting (transfers = 8, drive_chunk_size = 64M)
# rclone will consume 8*64 = 512 MiB of RAM when uploading
drive_chunk_size = 64
# by default rpl uses this command to upload the files
# rclone copy --exclude "*.parts" --exclude "*.!qB" --verbose --stats 1s \
# --use-json-log --transfers 8 --drive-chunk-size 64M <save_path> <remote_path>
# you can add more custom flags here, but do not override rpl's flags.
# the flags and their args must be in the correct order, like see example below
extra_custom_flags = ["--exclude", "RARBG_DO_NOT_MIRROR.exe"]"#;

fn setup_logging(verbosity: u64, chain: bool, log_path: Option<&str>) -> Result<Option<&str>> {
    let colors_line = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::Blue)
        .trace(Color::BrightBlack); // this is the same as the background color

    let mut base_config = fern::Dispatch::new();

    base_config = match verbosity {
        0 => base_config.level(LevelFilter::Warn),
        1 => base_config.level(LevelFilter::Info),
        2 => base_config.level(LevelFilter::Debug),
        _3_or_more => base_config.level(LevelFilter::Trace),
    };

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
        // Separate file config so we can include year, month and day (UTC) in file logs
        let log_file_path = PathBuf::from(
            shellexpand::full(log_path.unwrap())
                .expect("Could not find the correct path to log data")
                .into_owned(),
        );
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

        base_config
            .chain(file_config)
            .chain(stdout_config)
            .apply()?;
    } else {
        base_config.chain(stdout_config).apply()?;
    }

    Ok(log_path)
}

#[derive(Serialize, Deserialize, Getters)]
struct Config {
    rpl: RplConfig,
    qbittorrent: RplQbitConfig,
    seed_settings: SeedSettings,
    rclone: RplRcloneConfig,
}

#[derive(Serialize, Deserialize, Getters)]
struct RplConfig {
    max_size_percentage: u8,
    max_size: String,
    torrent_client: String,
    upload_client: String,
    save_path: String,
    remote_path: String,
    ignore_warning: bool,
}

struct RplRunningConfig {
    max_size: u64,
    //torrent_client: String,
    upload_client: String,
    save_path: String,
    remote_path: String,
    ignore_warning: bool,
}

impl RplRunningConfig {
    fn new(
        max_size: u64,
        //torrent_client: String,
        upload_client: String,
        save_path: String,
        remote_path: String,
        ignore_warning: bool,
    ) -> Self {
        Self {
            max_size,
            //torrent_client,
            upload_client,
            save_path,
            remote_path,
            ignore_warning,
        }
    }
}

#[derive(Serialize, Deserialize, Getters)]
struct RplQbitConfig {
    username: String,
    password: String,
    address: String,
    upload_limit: i64,
    download_limit: i64,
}

impl RplQbitConfig {
    fn new(
        username: String,
        password: String,
        address: String,
        upload_limit: i64,
        download_limit: i64,
    ) -> Self {
        Self {
            username,
            password,
            address,
            upload_limit,
            download_limit,
        }
    }
}

// TODO: more configs?
#[derive(Serialize, Deserialize, Getters)]
struct RplRcloneConfig {
    transfers: u16,
    drive_chunk_size: u16,
    extra_custom_flags: Vec<String>,
}

impl RplRcloneConfig {
    fn new(transfers: u16, drive_chunk_size: u16, extra_custom_flags: Vec<String>) -> Self {
        Self {
            transfers,
            drive_chunk_size,
            extra_custom_flags,
        }
    }
}

#[derive(Deserialize)]
struct MagnetTorrent {
    #[allow(dead_code)]
    result: i64,
    url: String,
}

// should always return error!
fn write_default_config(config_path: &Path) -> Result<(), error::Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(config_path)
        .unwrap();
    writeln!(file, "{}", STOCK_CONFIG)
        .expect("Could not write config to file, maybe there is a permission error?");
    warn!(
        "No config found, so I have created one at {}. Edit this file and run rpl again.",
        config_path.display()
    );
    Err(error::Error::SaveRemoteEmptyError)
}

impl Config {
    fn from_config(config_string: &str) -> Self {
        let config: Config = toml::from_str(config_string).expect("Could not parse config file");
        config
    }

    fn save_path_invalid(&self) -> bool {
        let save_path = &self.rpl.save_path;
        if save_path.is_empty() {
            true
        } else {
            let path = PathBuf::from(
                shellexpand::full(save_path)
                    .expect("Could not find the correct path to save data")
                    .into_owned(),
            );
            if !path.exists() {
                debug!("{} does not exist. I will create it now", path.display());
                fs::create_dir_all(path).unwrap();
            }
            false
        }
    }

    fn seed_path_invalid(&self) -> Result<bool, error::Error> {
        let seed_path = &self.seed_settings.seed_path();
        if seed_path.is_empty() {
            Ok(true)
        } else {
            let path = PathBuf::from(
                shellexpand::full(seed_path)
                    .expect("Could not find the correct path to save data")
                    .into_owned(),
            );
            if !path.exists() {
                error!(
                    "{} does not exist! rpl cannot seed after finishing leeching",
                    path.display()
                );
                Err(error::Error::MountPathNotExist)
            } else {
                Ok(false)
            }
        }
    }

    fn remote_path_invalid(&self) -> bool {
        self.rpl.remote_path.is_empty()
    }

    fn max_size_percentage_used(&self) -> Result<bool, error::Error> {
        // Can't use match here because https://github.com/rust-lang/rust/issues/37854
        let tmp = self.rpl.max_size_percentage;
        if tmp == 0 {
            Ok(false)
        } else if tmp > 0 && tmp <= 100 {
            Ok(true)
        } else {
            Err(error::Error::InvalidMaxSizePercentage)
        }
    }
}

fn get_rpl_config() -> Result<Config, error::Error> {
    let mut conf_file = util::get_conf_dir("", "", PROGRAM_NAME).unwrap();
    conf_file.push(PROGRAM_NAME);
    conf_file.set_file_name(PROGRAM_NAME);
    conf_file.set_extension("toml");

    let config: Config;
    if !conf_file.exists() {
        util::create_proj_conf("", "", PROGRAM_NAME).unwrap();
        write_default_config(&conf_file)?;
    }

    let s = fs::read_to_string(&conf_file).unwrap();
    config = Config::from_config(&s);

    Ok(config)
}

fn get_running_config(
    file_config: &Config,
    matches: &ArgMatches,
) -> Result<RplRunningConfig, error::Error> {
    let torrent_client = if let Some(client) = matches.value_of("torrent_client") {
        client
    } else {
        &file_config.rpl.torrent_client
    };

    if torrent_client != "qbittorrent" {
        return Err(error::Error::UnsupportedTorrentClient);
    }

    let upload_client = if let Some(client) = matches.value_of("upload_client") {
        client
    } else {
        &file_config.rpl.upload_client
    };

    match upload_client {
        "rclone" | "fclone" | "gclone" | "xclone" => (),
        _ => {
            return Err(error::Error::UnsupportedRcloneVariant);
        }
    }

    let save_path: String = if let Some(p) = matches.value_of("save_path") {
        let path = PathBuf::from(shellexpand::full(p).unwrap().into_owned());
        match !path.exists() {
            true => {
                debug!("{} does not exist. I will create it now", path.display());
                fs::create_dir_all(&path).expect("Could not create save_path from CLI arg");
            }
            false => (),
        }
        String::from(path.to_str().unwrap())
    } else {
        match &file_config.save_path_invalid() {
            true => {
                return Err(error::Error::SavePathEmptyError);
            }
            false => String::from(&file_config.rpl.save_path),
        }
    };

    let max_size_possible: u64 = match fs2::available_space(PathBuf::from(
        shellexpand::full(&file_config.rpl.save_path)
            .unwrap()
            .into_owned(),
    )) {
        Ok(size) => size,
        Err(_e) => return Err(error::Error::DiskSpaceReadError),
    };

    let max_size_allow: u64 = if let Some(percentage) = matches.value_of("max_size_percentage") {
        let p: u64 = percentage.parse::<u64>().unwrap();
        if p > 0 && p <= 100 {
            max_size_possible * p / 100
        } else {
            return Err(error::Error::InvalidMaxSizePercentage);
        }
    } else if let Some(size) = matches.value_of("max_size") {
        parse_size(size).expect("Could not parse max_size from input")
    } else if file_config.max_size_percentage_used().unwrap() {
        max_size_possible * (file_config.rpl.max_size_percentage as u64) / 100
    } else {
        parse_size(&file_config.rpl.max_size).expect("Could not parse max_size in file config")
    };

    let remote_path = if let Some(path) = matches.value_of("remote_path") {
        path
    } else {
        match &file_config.remote_path_invalid() {
            true => {
                return Err(error::Error::RemotePathEmptyError);
            }
            false => &file_config.rpl.remote_path,
        }
    };

    let ignore_warning: bool = if matches.is_present("ignore_warning") {
        true
    } else {
        file_config.rpl.ignore_warning
    };

    let running_config = RplRunningConfig::new(
        max_size_allow,
        //String::from(torrent_client),
        String::from(upload_client),
        save_path,
        String::from(remote_path),
        ignore_warning,
    );

    Ok(running_config)
}

fn get_qb_config(
    file_config: &Config,
    matches: &ArgMatches,
) -> Result<RplQbitConfig, error::Error> {
    let username = if let Some(usr) = matches.value_of("qbittorrent_username") {
        usr
    } else {
        &file_config.qbittorrent.username
    };

    let password = if let Some(pwd) = matches.value_of("qbittorrent_password") {
        pwd
    } else {
        &file_config.qbittorrent.password
    };

    let address = if let Some(addr) = matches.value_of("qbittorrent_address") {
        addr
    } else {
        &file_config.qbittorrent.address
    };

    let upload_limit: i64 = if let Some(val) = matches.value_of("qbittorrent_upload_limit") {
        val.parse().expect("Invalid upload limit")
    } else {
        file_config.qbittorrent.upload_limit
    };

    let download_limit: i64 = if let Some(val) = matches.value_of("qbittorrent_download_limit") {
        val.parse().expect("Invalid download limit")
    } else {
        file_config.qbittorrent.download_limit
    };

    let config = RplQbitConfig::new(
        String::from(username),
        String::from(password),
        String::from(address),
        upload_limit,
        download_limit,
    );

    Ok(config)
}

fn get_rclone_config(
    file_config: &Config,
    matches: &ArgMatches,
) -> Result<RplRcloneConfig, error::Error> {
    let transfers: u16 = if let Some(val) = matches.value_of("rclone_transfers") {
        val.parse().unwrap()
    } else {
        file_config.rclone.transfers
    };

    let drive_chunk_size: u16 = if let Some(val) = matches.value_of("rclone_drive_chunk_size") {
        val.parse().unwrap()
    } else {
        file_config.rclone.drive_chunk_size
    };

    // TODO: find a way to not clone
    let extra_custom_flags = file_config.rclone.extra_custom_flags.clone();

    let config = RplRcloneConfig::new(transfers, drive_chunk_size, extra_custom_flags);
    Ok(config)
}

struct TorrentInput {
    raw_data: Vec<u8>,
    input_type: RplInputType,
}

enum RplInputType {
    NormalPath,
    UrlLink,
    MagnetString,
}

impl TorrentInput {
    fn new(raw_data: Vec<u8>, input_type: RplInputType) -> Self {
        Self {
            raw_data,
            input_type,
        }
    }
}

async fn parse_input(matches: &ArgMatches<'_>) -> Result<TorrentInput, error::Error> {
    let input = matches.value_of("input").unwrap();

    let try_path = shellexpand::full(input)
        .expect("Could not look up a variable in input")
        .into_owned();
    if Path::new(&try_path).exists() {
        debug!("User inputted a normal path. Will now try to parse it.");
        let mut torrent_file = File::open(&try_path).unwrap();
        let mut raw_torrent = Vec::new();
        torrent_file.read_to_end(&mut raw_torrent)?;
        return Ok(TorrentInput::new(raw_torrent, RplInputType::NormalPath));
    } else if url::Url::parse(input).is_ok() {
        debug!("User inputted a url link. Will now download its content and try to parse it.");
        let response = reqwest::get(input).await?.bytes().await?;
        return Ok(TorrentInput::new(response.to_vec(), RplInputType::UrlLink));
    } else if input.contains("magnet") {
        // TODO: This is not ideal, maybe use a dedicated crate
        debug!("User inputted a magnet link, will now download the torrent file first");
        let client = reqwest::Client::new();
        let torrent_resp = client
            .get(format!(
                "https://anonymiz.com/magnet2torrent/magnet2torrent.php?magnet={}",
                input
            ))
            .send()
            .await?
            .json::<MagnetTorrent>()
            .await?;

        let re = regex::Regex::new("^(https://itorrents.org/torrent/.*.torrent)<").unwrap();
        let caps = re
            .captures(&torrent_resp.url)
            .expect("Could not capture torrent file link from MagnetTorrent response");
        let torrent_location = caps
            .get(1)
            .expect("Could not get capture group 1 from captures")
            .as_str();

        debug!("The torrent file location is {}", torrent_location);

        let response = reqwest::get(torrent_location).await?.bytes().await?;
        return Ok(TorrentInput::new(
            response.to_vec(),
            RplInputType::MagnetString,
        ));
    } else {
        return Err(error::Error::RplInvalidInput);
    }
}

fn get_seed_config(
    file_config: &Config,
    matches: &ArgMatches,
) -> Result<SeedSettings, error::Error> {
    let seed: bool = if matches.is_present("seed_enable") {
        true
    } else {
        *file_config.seed_settings.seed_enable()
    };

    if !seed {
        return Ok(SeedSettings::default());
    }

    let mut seed_path = String::new();
    if seed {
        seed_path = if let Some(p) = matches.value_of("seed_path") {
            let path = PathBuf::from(shellexpand::full(p).unwrap().into_owned());
            match path.exists() {
                true => {
                    return Err(error::Error::MountPathNotExist);
                }
                false => String::from(path.to_str().unwrap()),
            }
        } else {
            match &file_config.seed_path_invalid()? {
                true => {
                    return Err(error::Error::MountPathNotExist);
                }
                false => String::from(file_config.seed_settings.seed_path()),
            }
        };
    }

    let seed_wait: u32 = if let Some(val) = matches.value_of("seed_wait") {
        val.parse().unwrap()
    } else {
        *file_config.seed_settings.seed_wait()
    };

    let config = SeedSettings::new(seed, seed_path, seed_wait);

    Ok(config)
}

fn check_max_size_requirements(
    config: &RplRunningConfig,
    raw_data: &[u8],
) -> Result<(), error::Error> {
    let largest_file = get_largest_filesize(Torrent::read_from_bytes(raw_data).unwrap());

    match config.ignore_warning {
        false => {
            error!(
                "User specified max_size = {}, but the largest file in the pack is {}!",
                config.max_size.file_size(file_size_opts::BINARY).unwrap(),
                largest_file.file_size(file_size_opts::BINARY).unwrap(),
            );
            Err(error::Error::MaxSizeAllowedTooSmall)
        }
        true => {
            warn!(
                "User specified max_size = {}, but the largest file in the pack is {}! Some files will be skipped!",
                config.max_size.file_size(file_size_opts::BINARY).unwrap(),
                largest_file.file_size(file_size_opts::BINARY).unwrap(),
            );
            Ok(())
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let matches = App::new(PROGRAM_NAME)
        .setting(AppSettings::DisableHelpSubcommand)
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("input")
                .help("Input torrent file or url link or magnet string")
                .index(1)
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("log")
                .long("log")
                .takes_value(true)
                .help("Also log output to file (for debugging)"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("Sets the level of debug information verbosity"),
        )
        .arg(
            Arg::with_name("max_size_percentage")
                .long("percentage")
                .value_name("VALUE")
                .takes_value(true)
                .conflicts_with("max_size")
                .help("Set percentage of free available disk space allowed for rpl (1-100)"),
        )
        .arg(
            Arg::with_name("max_size")
                .short("s")
                .long("size")
                .value_name("VALUE")
                .takes_value(true)
                .help("Set disk space allowed for rpl"),
        )
        .arg(
            Arg::with_name("torrent_client")
                .long("torrent-client")
                .value_name("CLIENT")
                .takes_value(true)
                .help("Set the torrent client"),
        )
        .arg(
            Arg::with_name("upload_client")
                .long("upload-client")
                .value_name("CLIENT")
                .takes_value(true)
                .help("Set the upload client"),
        )
        .arg(
            Arg::with_name("save_path")
                .short("p")
                .long("save-path")
                .value_name("PATH")
                .takes_value(true)
                .help("Set the save path"),
        )
        .arg(
            Arg::with_name("remote_path")
                .short("r")
                .long("remote-path")
                .value_name("PATH")
                .takes_value(true)
                .help("Set the remote path"),
        )
        .arg(
            Arg::with_name("ignore_warning")
                .short("f")
                .long("force")
                .help("Force rpl to ignore warning about max_size being too small"),
        )
        .arg(
            Arg::with_name("seed_enable")
                .long("seed")
                .help("Seed the torrent after leeching"),
        )
        .arg(
            Arg::with_name("seed_path")
                .long("seed-path")
                .value_name("PATH")
                .help("Set the rclone's mount path used for seeding"),
        )
        .arg(
            Arg::with_name("seed_wait")
                .long("seed-wait")
                .value_name("VALUE")
                .help("Set the wait time for rclone to refresh mount path (in seconds)"),
        )
        .arg(
            Arg::with_name("skip")
                .long("skip")
                .value_name("VALUE")
                .takes_value(true)
                .help("Skip number of chunks (in case of unexpected errors)"),
        )
        .arg(
            Arg::with_name("qbittorrent_username")
                .long("qbu")
                .value_name("USERNAME")
                .takes_value(true)
                .help("Set the username of qBittorrent Web UI"),
        )
        .arg(
            Arg::with_name("qbittorrent_password")
                .long("qbp")
                .value_name("PASSWORD")
                .takes_value(true)
                .help("Set the password of qBittorrent Web UI"),
        )
        .arg(
            Arg::with_name("qbittorrent_address")
                .long("qba")
                .value_name("ADDRESS")
                .takes_value(true)
                .help("Set the address of qBittorrent Web UI"),
        )
        .arg(
            Arg::with_name("qbittorrent_upload_limit")
                .long("qbul")
                .value_name("VALUE")
                .takes_value(true)
                .help("Set the upload limit for torrents in qBittorrent (bytes/second)"),
        )
        .arg(
            Arg::with_name("qbittorrent_download_limit")
                .long("qbdl")
                .value_name("VALUE")
                .takes_value(true)
                .help("Set the download limit for torrents in qBittorrent (bytes/second)"),
        )
        .arg(
            Arg::with_name("rclone_transfers")
                .short("t")
                .long("transfers")
                .value_name("TRANSFERS")
                .takes_value(true)
                .help("Configure the number of transfers"),
        )
        .arg(
            Arg::with_name("rclone_drive_chunk_size")
                .long("drive-chunk-size")
                .value_name("SIZE")
                .takes_value(true)
                .help("Configure the drive chunk size value (in MiB)"),
        )
        .get_matches();

    let verbosity: u64 = matches.occurrences_of("verbose");
    let skip = if let Some(_val) = matches.value_of("skip") {
        value_t!(matches, "skip", u32).expect("Could not parse the value of skip")
    } else {
        0
    };

    let lock = matches.is_present("log");
    let log_path = if let Some(log) = matches.value_of("log") {
        setup_logging(verbosity, true, Some(log))?
    } else {
        setup_logging(verbosity, false, None)?
    };

    if lock {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(log_path.unwrap())
            .unwrap();
        file.lock_exclusive()?;
    }

    debug!("-----Logger is initialized. Starting main program!-----");
    let file_config = get_rpl_config()?;

    let config = get_running_config(&file_config, &matches)?;
    let qbconfig = get_qb_config(&file_config, &matches)?;
    let rclone_config = get_rclone_config(&file_config, &matches)?;
    let seed_config = get_seed_config(&file_config, &matches)?;
    let parsed_input = parse_input(&matches).await?;

    check_max_size_requirements(&config, &parsed_input.raw_data)?;

    let mut pack_config = TorrentPack::new(
        Torrent::read_from_bytes(&parsed_input.raw_data).unwrap(),
        config.ignore_warning,
    )
    .max_size(config.max_size as i64);

    let qbit = QbitConfig::new(&qbconfig.username, &qbconfig.password, &qbconfig.address).await?;

    let torrent_config = QbitTorrent::default()
        .torrents(Torrent::read_from_bytes(&parsed_input.raw_data).unwrap())
        .paused(true)
        .save_path(PathBuf::from(
            shellexpand::full(&config.save_path)
                .expect("Could not find the correct path to save data")
                .into_owned(),
        ))
        .upload_limit(qbconfig.upload_limit)
        .download_limit(qbconfig.download_limit);

    let upload_client = RcloneClient::new(
        config.upload_client,
        PathBuf::from(shellexpand::full(&config.save_path).unwrap().into_owned()),
        config.remote_path,
        rclone_config.transfers,
        rclone_config.drive_chunk_size,
        rclone_config.extra_custom_flags,
    );

    pack_config
        .leech_torrent(
            Torrent::read_from_bytes(&parsed_input.raw_data).unwrap(),
            torrent_config,
            qbit,
            upload_client,
            seed_config,
            skip,
        )
        .await?;

    match parsed_input.input_type {
        RplInputType::NormalPath => info!(
            "File `{}` finished leeching!",
            matches.value_of("input").unwrap()
        ),
        RplInputType::UrlLink => info!(
            "Url link `{}` finished leeching!",
            matches.value_of("input").unwrap()
        ),
        RplInputType::MagnetString => info!(
            "Magnet `{}` finished leeching!",
            matches.value_of("input").unwrap()
        ),
    }

    debug!("-----Everything is finished!-----");
    if lock {
        let file = OpenOptions::new()
            .write(true)
            .open(log_path.unwrap())
            .unwrap();
        file.unlock()?;
    }
    Ok(())
}

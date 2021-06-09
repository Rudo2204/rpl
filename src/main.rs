use anyhow::Result;
use chrono::{Local, Utc};
use clap::{crate_authors, crate_description, crate_version, App, AppSettings, Arg, ArgMatches};
use derive_getters::Getters;
use fern::colors::{Color, ColoredLevelConfig};
use fs2::FileExt;
use log::{debug, error, LevelFilter};
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
    upload_client: String,
    save_path: String,
    remote_path: String,
    ignore_warning: bool,
    seed: bool,
    seed_path: String,
}

struct RplRunningConfig {
    max_size: u64,
    //torrent_client: String,
    upload_client: String,
    save_path: String,
    remote_path: String,
    ignore_warning: bool,
    seed: bool,
    seed_path: String,
}

impl RplRunningConfig {
    fn new(
        max_size: u64,
        //torrent_client: String,
        upload_client: String,
        save_path: String,
        remote_path: String,
        ignore_warning: bool,
        seed: bool,
        seed_path: String,
    ) -> Self {
        Self {
            max_size,
            //torrent_client,
            upload_client,
            save_path,
            remote_path,
            ignore_warning,
            seed,
            seed_path,
        }
    }
}

#[derive(Serialize, Deserialize, Getters)]
struct RplQbitConfig {
    username: String,
    password: String,
    address: String,
}

impl RplQbitConfig {
    fn new(username: String, password: String, address: String) -> Self {
        Self {
            username,
            password,
            address,
        }
    }
}

// TODO: more configs?
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
uclone_client = "rclone"
# temporary data from pack will be saved to here
# this directory should be dedicated for rpl
save_path = ""
# rclone remote path for uploading. Example: "nugu:/rpl"
remote_path = ""
# Skip files that have size larger than max_size
ignore_warning = false
# set to true to seed the torrent through rclone's mount after rpl finishes
seed = false
# set the rclone's mount path
seed_path = ""

[qbittorrent]
# default username of qbittorrent Web UI
username = "admin"
# default password of qbittorrent Web UI
password = "adminadmin"
# default address of qbittorrent Web UI
address = "http://localhost:8080"

[rclone]
# default transfers of rclone
transfers = 4"#;

        Config::from_config(stock_config)
    }
}

impl Config {
    fn from_config(config_string: &str) -> Self {
        let config: Config = toml::from_str(config_string).expect("Could not parse config file");
        config
    }

    fn write_config(&self) {
        let mut config_file_path = util::get_conf_dir("", "", PROGRAM_NAME).unwrap();
        config_file_path.push(PROGRAM_NAME);
        config_file_path.set_extension("toml");
        let mut file = OpenOptions::new()
            .write(true)
            .open(config_file_path)
            .unwrap();
        writeln!(file, "{}", toml::to_string(&self).unwrap())
            .expect("Could not write config to file, maybe there is a permission error?");
    }

    fn save_path_invalid(&self) -> bool {
        let save_path = &self.rpl.save_path;
        if save_path.is_empty() {
            true
        } else {
            let path = Path::new(save_path);
            if !path.exists() {
                debug!("{} does not exist. I will create it now", path.display());
                fs::create_dir_all(path).unwrap();
            }
            false
        }
    }

    fn seed_path_invalid(&self) -> Result<bool, error::Error> {
        let seed_path = &self.rpl.seed_path;
        if seed_path.is_empty() {
            Ok(true)
        } else {
            let path = Path::new(seed_path);
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

    let max_size_possible: u64 = fs2::available_space(&file_config.rpl.save_path).unwrap();
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

    let upload_client = if let Some(client) = matches.value_of("upload_client") {
        client
    } else {
        &file_config.rpl.upload_client
    };

    match upload_client {
        "rclone" | "fclone" | "gclone" => (),
        _ => {
            return Err(error::Error::UnsupportedRcloneVariant);
        }
    }

    let save_path = if let Some(path) = matches.value_of("save_path") {
        match !Path::new(path).exists() {
            true => {
                return Err(error::Error::InvalidRplConfig);
            }
            false => path,
        }
    } else {
        match &file_config.save_path_invalid() {
            true => {
                return Err(error::Error::InvalidRplConfig);
            }
            false => &file_config.rpl.save_path,
        }
    };

    let remote_path = if let Some(path) = matches.value_of("remote_path") {
        match !Path::new(path).exists() {
            true => {
                return Err(error::Error::InvalidRplConfig);
            }
            false => path,
        }
    } else {
        match &file_config.remote_path_invalid() {
            true => {
                return Err(error::Error::InvalidRplConfig);
            }
            false => &file_config.rpl.remote_path,
        }
    };

    let ignore_warning: bool = if matches.is_present("ignore_warning") {
        true
    } else {
        file_config.rpl.ignore_warning
    };

    let seed: bool = if matches.is_present("seed") {
        true
    } else {
        file_config.rpl.seed
    };

    let seed_path = if let Some(path) = matches.value_of("seed_path") {
        match !Path::new(path).exists() {
            true => {
                return Err(error::Error::MountPathNotExist);
            }
            false => path,
        }
    } else {
        match &file_config.seed_path_invalid()? {
            true => {
                return Err(error::Error::MountPathNotExist);
            }
            false => &file_config.rpl.seed_path,
        }
    };

    let running_config = RplRunningConfig::new(
        max_size_allow,
        //String::from(torrent_client),
        String::from(upload_client),
        String::from(save_path),
        String::from(remote_path),
        ignore_warning,
        seed,
        String::from(seed_path),
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

    let config = RplQbitConfig::new(
        String::from(username),
        String::from(password),
        String::from(address),
    );

    Ok(config)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let matches = App::new(PROGRAM_NAME)
        .setting(AppSettings::DisableHelpSubcommand)
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("file")
                .help("The input torrent file")
                .index(1)
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("enable_logging")
                .short("l")
                .long("log")
                .help("Log output to logging file (for debugging)"),
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
                .takes_value(true)
                .conflicts_with("max_size")
                .help("Set percentage of free available disk space allowed for rpl"),
        )
        .arg(
            Arg::with_name("max_size")
                .short("s")
                .long("size")
                .takes_value(true)
                .help("Set disk space allowed for rpl"),
        )
        .arg(
            Arg::with_name("torrent_client")
                .long("tclient")
                .takes_value(true)
                .help("Set the torrent client"),
        )
        .arg(
            Arg::with_name("upload_client")
                .long("uclient")
                .takes_value(true)
                .help("Set the upload client"),
        )
        .arg(
            Arg::with_name("save_path")
                .short("p")
                .long("spath")
                .takes_value(true)
                .help("Set the save path"),
        )
        .arg(
            Arg::with_name("remote_path")
                .short("r")
                .long("rpath")
                .takes_value(true)
                .help("Set the remote path"),
        )
        .arg(
            Arg::with_name("ignore_warning")
                .short("f")
                .long("force")
                .help("Force rpl to ignore warning"),
        )
        .arg(
            Arg::with_name("seed")
                .long("seed")
                .help("Seed the torrent after rpl finishes leeching"),
        )
        .arg(
            Arg::with_name("seed_path")
                .long("spath")
                .help("Set the rclone's mount path used for seeding"),
        )
        .arg(
            Arg::with_name("qbittorrent_username")
                .long("qbu")
                .takes_value(true)
                .help("Set the username of qBittorrent Web UI"),
        )
        .arg(
            Arg::with_name("qbittorrent_password")
                .long("qbp")
                .takes_value(true)
                .help("Set the password of qBittorrent Web UI"),
        )
        .arg(
            Arg::with_name("qbittorrent_address")
                .long("qba")
                .takes_value(true)
                .help("Set the address of qBittorrent Web UI"),
        )
        .arg(
            Arg::with_name("rclone_transfers")
                .short("t")
                .long("transfers")
                .takes_value(true)
                .help("Configure the number of transfers"),
        )
        .get_matches();

    let chain = matches.is_present("enable_logging");
    let verbosity: u64 = matches.occurrences_of("verbose");

    let data_dir = util::get_data_dir("", "", PROGRAM_NAME)?;
    util::create_data_dir(&data_dir)?;

    setup_logging(verbosity, chain)?;
    let mut log_file_path = util::get_data_dir("", "", PROGRAM_NAME)?;
    log_file_path.push(PROGRAM_NAME);
    log_file_path.set_extension("log");
    let log_file = File::open(log_file_path)?;
    log_file.lock_exclusive()?;
    debug!("-----Logger is initialized. Starting main program!-----");
    let file_config = get_rpl_config()?;

    let config = get_running_config(&file_config, &matches)?;
    let qbconfig = get_qb_config(&file_config, &matches)?;

    let transfers: u16 = if let Some(trans) = matches.value_of("rclone_transfers") {
        trans.parse().unwrap()
    } else {
        file_config.rclone.transfers
    };

    let mut torrent_file = File::open(&matches.value_of("file").unwrap())?;
    let mut raw_torrent = Vec::new();
    torrent_file.read_to_end(&mut raw_torrent)?;

    let mut pack_config = TorrentPack::new(
        Torrent::read_from_bytes(&raw_torrent).unwrap(),
        config.ignore_warning,
    )
    .max_size(config.max_size as i64);

    let qbit = QbitConfig::new(&qbconfig.username, &qbconfig.password, &qbconfig.address).await?;

    let torrent_config = QbitTorrent::default()
        .torrents(Torrent::read_from_bytes(&raw_torrent).unwrap())
        .paused(true)
        .save_path(PathBuf::from(
            shellexpand::full(&config.save_path)
                .expect("Could not find the correct path to save data")
                .into_owned(),
        ));

    let upload_client = RcloneClient::new(
        config.upload_client,
        PathBuf::from(shellexpand::full(&config.save_path).unwrap().into_owned()),
        config.remote_path,
        transfers,
    );

    pack_config
        .leech_torrent(
            Torrent::read_from_bytes(&raw_torrent).unwrap(),
            torrent_config,
            qbit,
            upload_client,
            config.seed,
            &config.seed_path,
        )
        .await?;

    debug!("-----Everything is finished!-----");
    log_file.unlock()?;
    Ok(())
}

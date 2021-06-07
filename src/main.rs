use anyhow::Result;
use chrono::{Local, Utc};
//use clap::{crate_authors, crate_description, crate_version, App, AppSettings, Arg};
use fern::colors::{Color, ColoredLevelConfig};
use fs2::FileExt;
use log::{debug, LevelFilter};
use std::io::{stdout, Read};
use std::{fs::File, path::PathBuf};

mod librpl;
use librpl::qbittorrent::QbitConfig;
use librpl::util;

use librpl::qbittorrent::QbitTorrent;
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let chain = true;
    let verbosity: u64 = 1; //matches.occurrences_of("verbose");
    let max_size_allow: i64 = (5_f32 * (u32::pow(1024, 3) as f32)) as i64;
    let data_dir = util::get_data_dir("", "", PROGRAM_NAME)?;
    util::create_data_dir(&data_dir)?;

    setup_logging(verbosity, chain)?;
    let log_file_path =
        util::get_data_dir("", "", PROGRAM_NAME)?.join(format!("{}.log", PROGRAM_NAME));
    let log_file = File::open(log_file_path)?;
    log_file.lock_exclusive()?;
    debug!("-----Logger is initialized. Starting main program!-----");

    let mut torrent_file =
        File::open("[ReinForce] Maoujou de Oyasumi (BDRip 1920x1080 x264 FLAC).torrent")?;
    let mut raw_torrent = Vec::new();
    torrent_file.read_to_end(&mut raw_torrent)?;

    let mut pack_config =
        TorrentPack::new(Torrent::read_from_bytes(&raw_torrent).unwrap()).max_size(max_size_allow);

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
        PathBuf::from(
            shellexpand::full("~/rust_learnning/rust_product/rpl")
                .expect("Could not find the correct path to saved data")
                .into_owned(),
        ),
        String::from("rudovultr:/rpl"),
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

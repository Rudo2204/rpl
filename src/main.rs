use anyhow::Result;
use chrono::{Local, Utc};
//use clap::{crate_authors, crate_description, crate_version, App, AppSettings, Arg};
use fern::colors::{Color, ColoredLevelConfig};
use fs2::FileExt;
use log::{debug, info, LevelFilter};
use shellexpand::full_with_context;
use std::{fs::File, io, path::PathBuf};
use tokio::time::{sleep, Duration};

mod librpl;
use librpl::qbittorrent::QbitConfig;
use librpl::util;

use librpl::qbittorrent::TorrentDownload;
use librpl::torrent_parser::PackConfig;

pub const PROGRAM_NAME: &str = "mendo";

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
                "{date} {colored_level} {colored_target} > {colored_message}",
                date = Local::now().format("%H:%M:%S"),
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
        .chain(io::stdout());

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
    let verbosity: u64 = 2; //matches.occurrences_of("verbose");
    let max_size_allow: i64 = 2 * i64::pow(1024, 3);
    let data_dir = util::get_data_dir("", "", PROGRAM_NAME)?;
    util::create_data_dir(&data_dir)?;

    setup_logging(verbosity, chain)?;
    let log_file_path =
        util::get_data_dir("", "", PROGRAM_NAME)?.join(format!("{}.log", PROGRAM_NAME));
    let log_file = File::open(log_file_path)?;
    log_file.lock_exclusive()?;
    debug!("-----Logger is initialized. Starting main program!-----");

    let torrent = Torrent::read_from_file(
        "[ReinForce] Maoujou de Oyasumi (BDRip 1920x1080 x264 FLAC).torrent",
    )
    .unwrap();

    let mut pack_config = PackConfig::new(torrent.clone()).max_size(max_size_allow);
    info!("{}", pack_config.get_pack_size_human());

    let hash = pack_config.info_hash();
    let disable_all = pack_config.disable_all_string();
    info!("is_private: {}", &pack_config.is_private());
    debug!("{:#?}", pack_config.chunks()?);
    let addr = "http://localhost:7070";
    let qbit = QbitConfig::new("", "", addr).await?;
    info!(
        "Qbittorrent App Version: {}",
        qbit.application_version().await?
    );

    let t = TorrentDownload::default()
        .torrents(torrent)
        .paused(true)
        .save_path(PathBuf::from(
            full_with_context("~/Videos/", util::home_dir, util::get_env)
                .expect("Could not find the correct path to save data")
                .into_owned(),
        ));

    qbit.add_new_torrent(t).await?;

    info!("Sleeping 500ms for qbittorrent to add the torrent...");
    sleep(Duration::from_millis(500)).await;

    qbit.set_priority(hash, disable_all, 0).await?;

    debug!("-----Everything is finished!-----");
    log_file.unlock()?;
    Ok(())
}

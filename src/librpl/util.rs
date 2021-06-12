use directories::ProjectDirs;
use indicatif::{ProgressBar, ProgressStyle};
use log::debug;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

pub fn get_conf_dir(
    qualifier: &str,
    organization: &str,
    application: &str,
) -> anyhow::Result<PathBuf> {
    let proj_dirs = ProjectDirs::from(&qualifier, &organization, &application)
        .expect("Could not retrieve ProjectDirs, maybe you are using an unsupported OS");
    Ok(proj_dirs.config_dir().to_path_buf())
}

pub fn create_proj_conf(
    qualifier: &str,
    organization: &str,
    application: &str,
) -> anyhow::Result<()> {
    let proj_dirs = ProjectDirs::from(&qualifier, &organization, &application)
        .expect("Could not retrieve ProjectDirs, maybe you are using an unsupported OS");
    let conf_dir = proj_dirs.config_dir();

    debug!(
        "{} configuration file does not exist. I will now create a configuration file at {}",
        &application,
        conf_dir.display()
    );

    std::fs::create_dir_all(conf_dir).expect("Could not create config dir");
    Ok(())
}

pub async fn wait_with_progress(wait_time: u32) {
    let pb = ProgressBar::new(wait_time as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} {msg} [{bar:30.cyan/blue}] ({eta_precise})")
            .progress_chars("#>-"),
    );

    pb.set_message(format!("Waiting {} seconds", wait_time));
    for _i in 0..wait_time {
        sleep(Duration::from_millis(1000)).await;
        pb.tick();
    }
}

use directories::ProjectDirs;
use log::debug;
use std::path::{Path, PathBuf};

pub fn get_data_dir(
    qualifier: &str,
    organization: &str,
    application: &str,
) -> anyhow::Result<PathBuf> {
    let proj_dirs = ProjectDirs::from(&qualifier, &organization, &application)
        .expect("Could not retrieve ProjectDirs, maybe you are using an unsupported OS");
    Ok(proj_dirs.data_dir().to_path_buf())
}

pub fn create_data_dir(data_dir: &Path) -> anyhow::Result<()> {
    if !data_dir.exists() {
        debug!("Project data dir does not exist, creating them...");
        std::fs::create_dir_all(data_dir)?;
        debug!("Successfully created data dirs");
    }
    Ok(())
}

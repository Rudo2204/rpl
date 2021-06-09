use directories::ProjectDirs;
use log::debug;
use std::path::PathBuf;

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

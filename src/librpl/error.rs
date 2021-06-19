#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Request Error when talking to qbittorrent: {0}")]
    ReqErr(#[from] reqwest::Error),
    #[error("Could not convert reqwest header to string: {0}")]
    ToStringError(#[from] reqwest::header::ToStrError),
    #[error("Serde json could not correctly deserialize: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Serde_urlencoded could not serialize the url: {0}")]
    SerdeUrl(#[from] serde_urlencoded::ser::Error),
    #[error("Header value was malformed: {0}")]
    HeaderError(#[from] reqwest::header::InvalidHeaderValue),
    #[error("Header value was not correctly set")]
    MissingHeaders,
    #[error("Cookie value was not correctly set")]
    MissingCookie,
    #[error("Could not slice off SID from returned cookie")]
    SliceError,
    #[error("There is nothing in the pack to leech")]
    NothingToLeech,
    #[error("qBittorrent returned nothing for the hash provided")]
    QbitEmptyTorrentInfo,
    #[error("qBittorrent client: The torrent has encountered an unexpected error")]
    QbitTorrentErrored,
    #[error("Unexpected rclone stderr capture error encountered")]
    RcloneStderrCaptureError,
    #[error("Command io spawning error: {0}")]
    CommandSpawningError(#[from] std::io::Error),
    #[error("File size in pack is larger than maximum allowed size")]
    MaxSizeAllowedTooSmall,
    #[error("Invalid max_size_percentage, allowed value are 0-100")]
    InvalidMaxSizePercentage,
    #[error("qBittorrent client: The torrent has entered unimplemented state!")]
    QbitTorrentUnimplementedState,
    #[error("Config error: Unsupported torrent client")]
    UnsupportedTorrentClient,
    #[error("Config error: mount path does not exist")]
    MountPathNotExist,
    #[error(
        "Config error: Unsupported rclone variant (only rclone/fclone/gclone/xclone is supported)"
    )]
    UnsupportedRcloneVariant,
    #[error("Input error: rpl could not parse the input (only torrent file, url link and magnet link are supported)")]
    RplInvalidInput,
    #[error("Config error: save_path cannot be empty")]
    SavePathEmptyError,
    #[error("Config error: save_path and remote_path in config file cannot be empty")]
    SaveRemoteEmptyError,
    #[error("Config error: remote_path cannot be empty")]
    RemotePathEmptyError,
    #[error("Config error: could not read available disk space from save_path")]
    DiskSpaceReadError,
}

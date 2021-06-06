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
    #[error("The torrent added does not have any files")]
    EmptyTorrent,
    #[error("qBittorrent returned nothing for the hash provided")]
    QbitEmptyTorrentInfo,
    #[error("qBittorrent client: The torrent has encountered an unexpected error")]
    QbitTorrentErrored,
    #[error("Unexpected rclone stderr capture error encountered")]
    RcloneStderrCaptureError,
    #[error("Command io spawning error: {0}")]
    CommandSpawningError(#[from] std::io::Error),
}

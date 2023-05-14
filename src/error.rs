#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP Digest generation error")]
    Digest,
    #[error("JSON encoding error")]
    Json(#[from] serde_json::Error),
    #[error("Signature error")]
    Signature(#[from] sigh::Error),
    #[error("HTTP request error")]
    HttpReq(#[from] http::Error),
    #[error("HTTP client error")]
    Http(#[from] reqwest::Error),
    #[error("Invalid URI")]
    InvalidUri,
    #[error("Error response from remote")]
    Response(String),
}

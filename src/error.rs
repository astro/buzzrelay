use std::sync::Arc;

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP Digest generation error")]
    Digest,
    #[error("JSON encoding error")]
    Json(#[from] Arc<serde_json::Error>),
    #[error("Signature error")]
    Signature(#[from] Arc<sigh::Error>),
    #[error("Signature verification failure")]
    SignatureFail(String),
    #[error("HTTP request error")]
    HttpReq(#[from] Arc<http::Error>),
    #[error("HTTP client error")]
    Http(#[from] Arc<reqwest::Error>),
    #[error("Invalid URI")]
    InvalidUri,
    #[error("Error response from remote")]
    Response(String),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(Arc::new(e))
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(Arc::new(e))
    }
}

impl From<sigh::Error> for Error {
    fn from(e: sigh::Error) -> Self {
        Error::Signature(Arc::new(e))
    }
}

impl From<http::Error> for Error {
    fn from(e: http::Error) -> Self {
        Error::HttpReq(Arc::new(e))
    }
}

use std::{sync::Arc};

use http::StatusCode;
use http_digest_headers::{DigestHeader, DigestMethod};

use serde::Serialize;
use sigh::{PrivateKey, SigningConfig, alg::RsaSha256};

#[derive(Debug, thiserror::Error)]
pub enum SendError {
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

pub async fn send<T: Serialize>(
    client: &reqwest::Client,
    uri: &str,
    key_id: &str,
    private_key: &PrivateKey,
    body: &T,
) -> Result<(), SendError> {
    let body = Arc::new(
        serde_json::to_vec(body)
            .map_err(SendError::Json)?
    );
    send_raw(client, uri, key_id, private_key, body).await
}

pub async fn send_raw(
    client: &reqwest::Client,
    uri: &str,
    key_id: &str,
    private_key: &PrivateKey,
    body: Arc<Vec<u8>>,
) -> Result<(), SendError> {
    let mut digest_header = DigestHeader::new()
        .with_method(DigestMethod::SHA256, &body)
        .map(|h| format!("{}", h))
        .map_err(|_| SendError::Digest)?;
    if digest_header.starts_with("sha-") {
        digest_header.replace_range(..4, "SHA-");
    }
    // mastodon uses base64::alphabet::STANDARD, not base64::alphabet::URL_SAFE
    digest_header.replace_range(
        7..,
        &digest_header[7..].replace('-', "+").replace('_', "/")
    );

    let url = reqwest::Url::parse(uri)
        .map_err(|_| SendError::InvalidUri)?;
    let mut req = http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", format!("{}", url.host().ok_or(SendError::InvalidUri)?))
        .header("content-type", "application/activity+json")
        .header("date", chrono::Utc::now().to_rfc2822()
            .replace("+0000", "GMT"))
        .header("digest", digest_header)
        .body(body.as_ref().clone())
        .map_err(SendError::HttpReq)?;
    SigningConfig::new(RsaSha256, private_key, key_id)
        .sign(&mut req)?;
    let req: reqwest::Request = req.try_into()?;
    let res = client.execute(req)
        .await?;
    if res.status() >= StatusCode::OK && res.status() < StatusCode::MULTIPLE_CHOICES {
        Ok(())
    } else {
        let response = res.text().await?;
        Err(SendError::Response(response))
    }
}

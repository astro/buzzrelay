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
}

pub async fn send<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    key_id: &str,
    private_key: &PrivateKey,
    body: T,
) -> Result<(), SendError> {
    let body = serde_json::to_vec(&body)
        .map_err(SendError::Json)?;
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
        &digest_header[7..].replace("-", "+").replace("_", "/")
    );

    let mut req = http::Request::builder()
        .method("POST")
        .uri(url)
        .header("content-type", "application/activity+json")
        .header("date", chrono::Utc::now().to_rfc2822()
            .replace("+0000", "GMT"))
        .header("digest", digest_header)
        .body(body)
        .map_err(SendError::HttpReq)?;
    SigningConfig::new(RsaSha256, private_key, key_id)
        .sign(&mut req)?;
    dbg!(&req);
    let res = client.execute(req.try_into()?)
        .await?;
    dbg!(&res);
    dbg!(res.text().await);

    Ok(())
}

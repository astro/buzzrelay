use http::StatusCode;
use serde::de::DeserializeOwned;
use sigh::{PrivateKey, SigningConfig, alg::RsaSha256};
use crate::digest;
use crate::send::SendError;

pub async fn fetch<T>(client: &reqwest::Client, url: &str) -> Result<T, reqwest::Error>
where
    T: DeserializeOwned,
{
    client.get(url)
        .header("accept", "application/activity+json")
        .send()
        .await?
        .json()
        .await
}

pub async fn authorized_fetch<T>(
    client: &reqwest::Client,
    uri: &str,
    key_id: &str,
    private_key: &PrivateKey,
) -> Result<T, SendError>
where
    T: DeserializeOwned,
{
    let url = reqwest::Url::parse(uri)
        .map_err(|_| SendError::InvalidUri)?;
    let host = format!("{}", url.host().ok_or(SendError::InvalidUri)?);
    let digest_header = digest::generate_header(&[])
        .expect("digest::generate_header");
    let mut req = http::Request::builder()
        .uri(uri)
        .header("host", &host)
        .header("content-type", "application/activity+json")
        .header("date", chrono::Utc::now().to_rfc2822()
            .replace("+0000", "GMT"))
        .header("accept", "application/activity+json")
        .header("digest", digest_header)
        .body(vec![])?;
    SigningConfig::new(RsaSha256, private_key, key_id)
        .sign(&mut req)?;
    let req: reqwest::Request = req.try_into()?;
    let res = client.execute(req)
        .await?;
    if res.status() >= StatusCode::OK && res.status() < StatusCode::MULTIPLE_CHOICES {
        Ok(res.json().await?)
    } else {
        Err(SendError::Response(format!("{}", res.text().await?)))
    }
}

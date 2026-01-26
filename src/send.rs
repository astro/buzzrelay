use std::{
    sync::Arc,
    time::{Instant, SystemTime},
};
use http::StatusCode;
use metrics::histogram;
use serde::Serialize;
use sigh::{PrivateKey, SigningConfig, alg::RsaSha256};
use tokio::task::spawn_blocking;
use crate::{digest, error::Error};

pub async fn send<T: Serialize>(
    client: &reqwest::Client,
    uri: &str,
    key_id: &str,
    private_key: &PrivateKey,
    body: &T,
) -> Result<(), Error> {
    let body = Arc::new(
        serde_json::to_vec(body)?
    );
    send_raw(client, uri, key_id, private_key, body).await
}

pub async fn send_raw(
    client: &reqwest::Client,
    uri: &str,
    key_id: &str,
    private_key: &PrivateKey,
    body: Arc<Vec<u8>>,
) -> Result<(), Error> {
    let digest_header = digest::generate_header(&body)
        .map_err(|()| Error::Digest)?;
    let mut req = http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/activity+json")
        .header("date", httpdate::fmt_http_date(SystemTime::now()))
        .header("digest", digest_header)
        .body(body.as_ref().clone())?;
    let t1 = Instant::now();
    let private_key = private_key.clone();
    let key_id = key_id.to_string();
    let req = spawn_blocking(move || {
        SigningConfig::new(RsaSha256, &private_key, &key_id).sign(&mut req)?;
        Ok(req)
    })
    .await
    .map_err(|e| Error::Response(format!("{e}")))?
    .map_err(|e: sigh::Error| Error::Response(format!("{e}")))?;
    let t2 = Instant::now();
    let req: reqwest::Request = req.try_into()?;
    let res = client.execute(req).await?;
    let t3 = Instant::now();
    histogram!("relay_http_request_duration")
        .record(t2 - t1);
    if res.status() >= StatusCode::OK && res.status() < StatusCode::MULTIPLE_CHOICES {
        histogram!("relay_http_response_duration", "res" => "ok")
            .record(t3 - t2);
        Ok(())
    } else {
        histogram!("relay_http_response_duration", "res" => "err")
            .record(t3 - t2);
        let response = res.text().await?;
        Err(Error::Response(response))
    }
}

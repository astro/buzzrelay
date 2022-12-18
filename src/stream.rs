use std::time::Duration;
use futures::{Stream, StreamExt};
use eventsource_stream::Eventsource;
use tokio::{
    sync::mpsc::{channel, Receiver},
    time::sleep,
};

#[derive(Debug)]
pub enum StreamError {
    Http(reqwest::Error),
    HttpStatus(reqwest::StatusCode),
    InvalidContentType,
}

async fn run(host: &str) -> Result<impl Stream<Item = serde_json::Value>, StreamError> {
    let url = format!("https://{}/api/v1/streaming/public", host);
    let client = reqwest::Client::new();
    let res = client.get(url)
        .timeout(Duration::MAX)
        .send()
        .await
        .map_err(StreamError::Http)?;
    if res.status() != 200 {
        return Err(StreamError::HttpStatus(res.status()));
    }
    let ct = res.headers().get("content-type")
        .and_then(|c| c.to_str().ok());
    if ct.map_or(true, |ct| ct != "text/event-stream") {
        return Err(StreamError::InvalidContentType);
    }

    let src = res.bytes_stream()
        .eventsource()
        .filter_map(|result| async {
            let result = result.ok()?;
            if result.event == "update" {
                Some(result)
            } else {
                None
            }
        })
        .filter_map(|event| async move {
            match serde_json::from_str(&event.data) {
                Ok(post) => Some(post),
                Err(e) => {
                    tracing::error!("Decode stream: {}", e);
                    None
                }
            }
        });
    Ok(src)
}

pub fn spawn<H: Into<String>>(host: H) -> Receiver<serde_json::Value> {
    let host = host.into();
    let (tx, rx) = channel(1024);
    tokio::spawn(async move {
        loop {
            match run(&host).await {
                Ok(stream) =>
                    stream.for_each(|post| async {
                        tx.send(post).await.unwrap();
                    }).await,
                Err(e) =>
                    tracing::error!("stream: {:?}", e),
            }

            sleep(Duration::from_secs(1)).await;
        }
    });
    rx
}

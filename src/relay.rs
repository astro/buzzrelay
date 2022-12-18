use std::sync::Arc;

use serde_json::json;
use sigh::PrivateKey;
use tokio::{
    sync::mpsc::Receiver,
};
use crate::send::send;

pub fn spawn(
    client: Arc<reqwest::Client>,
    key_id: String,
    private_key: PrivateKey,
    mut stream_rx: Receiver<serde_json::Value>
) {
    tokio::spawn(async move {
        while let Some(post) = stream_rx.recv().await {
            dbg!(&post);
            let url = if let Some(serde_json::Value::String(url)) = post.get("url") {
                url
            } else {
                continue;
            };
            let uri = if let Some(serde_json::Value::String(uri)) = post.get("uri") {
                uri
            } else {
                continue;
            };
            let account = if let Some(serde_json::Value::String(account)) = post.get("account").and_then(|a| a.get("url")) {
                account
            } else {
                continue;
            };
            // {"@context": "https://www.w3.org/ns/activitystreams", "type": "Announce", "to": ["https://relay.dresden.network/followers"], "actor": "https://relay.dresden.network/actor", "object": "https://mastodon.online/users/evangreer/statuses/109521063161210607", "id": "https://relay.dresden.network/activities/5e41fd9c-bc51-408c-94ca-96a7bf9ce412"}
            let body = json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "type": "Announce",
                "actor": "https://relay.fedi.buzz/actor",
                "to": ["https://www.w3.org/ns/activitystreams#Public"],
                "object": &uri,
                "id": &url,
            });
            dbg!(&body);
            send(&client, "https://c3d2.social/inbox",
                 &key_id, &private_key, body).await
                .map_err(|e| tracing::error!("relay::send {:?}", e));
        }
    });
}

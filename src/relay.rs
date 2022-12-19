use std::{sync::Arc, collections::HashSet};

use serde::Deserialize;
use serde_json::json;
use sigh::PrivateKey;
use tokio::{
    sync::mpsc::Receiver,
};
use crate::{db::Database, send, actor};

#[derive(Deserialize)]
struct Post<'a> {
    // pub url: &'a str,
    pub uri: &'a str,
    pub tags: Option<Vec<Tag<'a>>>,
}

impl Post<'_> {
    pub fn host(&self) -> Option<String> {
        reqwest::Url::parse(&self.uri)
            .ok()
            .and_then(|url| url.domain()
                      .map(|s| s.to_lowercase())
            )
    }

    pub fn tags(&self) -> Vec<String> {
        match &self.tags {
            None =>
                vec![],
            Some(tags) =>
                tags.iter()
                .map(|tag| tag.name.to_lowercase())
                .collect()
        }
    }

    fn relay_target_kinds(&self) -> impl Iterator<Item = actor::ActorKind> {
        self.host()
            .into_iter()
            .map(|host| actor::ActorKind::InstanceRelay(host.clone()))
            .chain(
                self.tags()
                    .into_iter()
                    .map(|tag| actor::ActorKind::TagRelay(tag))
            )
    }

    pub fn relay_targets(&self, hostname: Arc<String>) -> impl Iterator<Item = actor::Actor> {
        self.relay_target_kinds()
            .map(move |kind| actor::Actor {
                host: hostname.clone(),
                kind,
            })
    }
}

#[derive(Deserialize)]
struct Tag<'a> {
    pub name: &'a str,
}

pub fn spawn(
    client: Arc<reqwest::Client>,
    hostname: Arc<String>,
    database: Database,
    private_key: PrivateKey,
    mut stream_rx: Receiver<String>
) {
    tokio::spawn(async move {
        while let Some(data) = stream_rx.recv().await {
            // dbg!(&data);
            let post: Post = match serde_json::from_str(&data) {
                Ok(post) => post,
                Err(e) => {
                    tracing::error!("parse error: {}", e);
                    tracing::trace!("data: {}", data);
                    continue;
                }
            };
            // TODO: queue by target?
            let mut seen = HashSet::new();
            for actor in post.relay_targets(hostname.clone()) {
                if seen.contains(&actor) {
                    continue;
                }

                let actor_id = actor.uri();
                let body = json!({
                    "@context": "https://www.w3.org/ns/activitystreams",
                    "type": "Announce",
                    "actor": &actor_id,
                    "to": ["https://www.w3.org/ns/activitystreams#Public"],
                    "object": &post.uri,
                    "id": &post.uri,
                });
                let body = Arc::new(
                    serde_json::to_vec(&body)
                        .unwrap()
                );
                for inbox in database.get_following_inboxes(&actor_id).await.unwrap() {
                    if let Err(e) = send::send_raw(
                        &client, &inbox,
                        &actor.key_id(), &private_key, body.clone()
                    ).await {
                        tracing::error!("relay::send {:?}", e);
                    }
                }

                seen.insert(actor);
            }
        }
    });
}

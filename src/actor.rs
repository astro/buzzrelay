use std::sync::Arc;
use deunicode::deunicode;
use sigh::{PublicKey, Key};

use crate::activitypub;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActorKind {
    Tag(String),
    Instance(String),
    Ingest,
}

impl ActorKind {
    pub fn from_tag(tag: &str) -> Self {
        let tag = deunicode(tag)
            .to_lowercase()
            .replace(char::is_whitespace, "");
        ActorKind::Tag(tag)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Actor {
    pub host: Arc<String>,
    pub kind: ActorKind,
}

impl Actor {
    pub fn uri(&self) -> String {
        match &self.kind {
            ActorKind::Tag(tag) =>
                format!("https://{}/tag/{}", self.host, tag),
            ActorKind::Instance(instance) =>
                format!("https://{}/instance/{}", self.host, instance),
            ActorKind::Ingest => format!("https://{}/ingest", self.host),
        }
    }

    pub fn key_id(&self) -> String {
        format!("{}#key", self.uri())
    }

    pub fn as_activitypub(&self, pub_key: &PublicKey) -> activitypub::Actor {
        activitypub::Actor {
            jsonld_context: serde_json::Value::String("https://www.w3.org/ns/activitystreams".to_string()),
            actor_type: "Service".to_string(),
            id: self.uri(),
            name: Some(match &self.kind {
                ActorKind::Tag(tag) =>
                    format!("#{}", tag),
                ActorKind::Instance(instance) =>
                    instance.to_string(),
                ActorKind::Ingest =>
                    self.host.to_string()
            }),
            icon: Some(activitypub::Media {
                media_type: "Image".to_string(),
                content_type: "image/jpeg".to_string(),
                url: "https://fedi.buzz/assets/favicon48.png".to_string(),
            }),
            inbox: self.uri(),
            outbox: format!("{}/outbox", self.uri()),
            public_key: activitypub::ActorPublicKey {
                id: self.key_id(),
                owner: Some(self.uri()),
                pem: pub_key.to_pem().unwrap(),
            },
            preferred_username: Some(match &self.kind {
                ActorKind::Tag(tag) =>
                    format!("tag-{}", tag),
                ActorKind::Instance(instance) =>
                    format!("instance-{}", instance),
                ActorKind::Ingest =>
                    String::from(env!("CARGO_PKG_NAME")),
            }),
        }
    }
}

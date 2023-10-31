use std::sync::Arc;
use deunicode::deunicode;
use sigh::{PublicKey, Key};

use crate::activitypub;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActorKind {
    TagRelay(String),
    InstanceRelay(String),
}

impl ActorKind {
    pub fn from_tag(tag: &str) -> Self {
        let tag = deunicode(tag)
            .to_lowercase()
            .replace(char::is_whitespace, "");
        ActorKind::TagRelay(tag)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Actor {
    pub host: Arc<String>,
    pub kind: ActorKind,
}

impl Actor {
    pub fn from_uri(mut uri: &str) -> Option<Self> {
        let kind;
        let host;
        if uri.starts_with("acct:tag-") {
            let off = "acct:tag-".len();
            let Some(at) = uri.find('@') else { return None; };
            kind = ActorKind::TagRelay(uri[off..at].to_string());
            host = Arc::new(uri[at + 1..].to_string());
        } else if uri.starts_with("acct:instance-") {
            let off = "acct:instance-".len();
            let Some(at) = uri.find('@') else { return None; };
            kind = ActorKind::InstanceRelay(uri[off..at].to_string());
            host = Arc::new(uri[at + 1..].to_string());
        } else if uri.starts_with("https://") {
            uri = &uri[8..];

            let parts = uri.split('/').collect::<Vec<_>>();
            if parts.len() != 3 {
                return None;
            }

            let Ok(topic) = urlencoding::decode(parts[2]) else { return None; };
            kind = match parts[1] {
                "tag" =>
                    ActorKind::TagRelay(topic.to_string()),
                "instance" =>
                    ActorKind::InstanceRelay(topic.to_string()),
                _ =>
                    return None,
            };
            host = Arc::new(parts[0].to_string());
        } else {
            return None;
        }
        Some(Actor { host, kind })
    }

    pub fn uri(&self) -> String {
        match &self.kind {
            ActorKind::TagRelay(tag) =>
                format!("https://{}/tag/{}", self.host, tag),
            ActorKind::InstanceRelay(instance) =>
                format!("https://{}/instance/{}", self.host, instance),
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
                ActorKind::TagRelay(tag) =>
                    format!("#{}", tag),
                ActorKind::InstanceRelay(instance) =>
                    instance.to_string(),
            }),
            icon: Some(activitypub::Media {
                media_type: Some("Image".to_string()),
                content_type: Some("image/jpeg".to_string()),
                url: "https://fedi.buzz/assets/favicon48.png".to_string(),
            }),
            inbox: self.uri(),
            endpoints: Some(activitypub::ActorEndpoints {
                shared_inbox: format!("https://{}/instance/{}", self.host, self.host),
            }),
            outbox: Some(format!("{}/outbox", self.uri())),
            public_key: activitypub::ActorPublicKey {
                id: self.key_id(),
                owner: Some(self.uri()),
                pem: pub_key.to_pem().unwrap(),
            },
            preferred_username: Some(match &self.kind {
                ActorKind::TagRelay(tag) =>
                    format!("tag-{}", tag),
                ActorKind::InstanceRelay(instance) =>
                    format!("instance-{}", instance),
            }),
        }
    }
}

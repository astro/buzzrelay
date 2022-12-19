use std::sync::Arc;
use sigh::{PublicKey, Key};

use crate::activitypub;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActorKind {
    TagRelay(String),
    InstanceRelay(String),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Actor {
    pub host: Arc<String>,
    pub kind: ActorKind,
}

impl Actor {
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
            inbox: self.uri(),
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

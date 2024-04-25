use std::sync::Arc;
use deunicode::deunicode;
use serde_json::json;
use sigh::{PublicKey, Key};

use crate::activitypub;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)]
pub enum ActorKind {
    TagRelay(String),
    InstanceRelay(String),
    LanguageRelay(String),
}

impl ActorKind {
    pub fn from_tag(tag: &str) -> Self {
        let tag = deunicode(tag)
            .to_lowercase()
            .replace(char::is_whitespace, "");
        ActorKind::TagRelay(tag)
    }

    pub fn from_language(language: &str) -> Option<Self> {
        let language = language.to_lowercase()
            .chars()
            .take_while(|c| c.is_alphabetic())
            .collect::<String>();
        if language.is_empty() {
            None
        } else {
            Some(ActorKind::LanguageRelay(language))
        }
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
            kind = ActorKind::from_tag(&uri[off..at]);
            host = Arc::new(uri[at + 1..].to_string());
        } else if uri.starts_with("acct:instance-") {
            let off = "acct:instance-".len();
            let Some(at) = uri.find('@') else { return None; };
            kind = ActorKind::InstanceRelay(uri[off..at].to_lowercase());
            host = Arc::new(uri[at + 1..].to_string());
        } else if uri.starts_with("acct:language-") {
            let off = "acct:language-".len();
            let Some(at) = uri.find('@') else { return None; };
            kind = ActorKind::from_language(&uri[off..at])?;
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
                "language" =>
                    ActorKind::LanguageRelay(topic.to_string()),
                _ =>
                    return None,
            };
            host = Arc::new(parts[0].to_string());
        } else {
            return None;
        }
        Some(Actor { host, kind })
    }

    pub fn from_object(object: &serde_json::Value) -> Option<Self> {
        let mut target: Option<String> = None;
        if let Some(object) = object.as_str() {
            target = Some(object.to_string());
        }
        if let Some(object_0) = object.as_array()
            .and_then(|object| {
                if object.len() == 1 {
                    object.first()
                } else {
                    None
                }
            }).and_then(|object_0| object_0.as_str())
        {
            target = Some(object_0.to_string());
        }

        target.and_then(|target| Self::from_uri(&target))
    }

    pub fn uri(&self) -> String {
        match &self.kind {
            ActorKind::TagRelay(tag) =>
                format!("https://{}/tag/{}", self.host, tag),
            ActorKind::InstanceRelay(instance) =>
                format!("https://{}/instance/{}", self.host, instance),
            ActorKind::LanguageRelay(language) =>
                format!("https://{}/language/{}", self.host, language),
        }
    }

    pub fn key_id(&self) -> String {
        format!("{}#key", self.uri())
    }

    pub fn as_activitypub(&self, pub_key: &PublicKey) -> activitypub::Actor {
        activitypub::Actor {
            jsonld_context: json!([
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1"
            ]),
            actor_type: "Service".to_string(),
            id: self.uri(),
            name: Some(match &self.kind {
                ActorKind::TagRelay(tag) =>
                    format!("#{tag}"),
                ActorKind::InstanceRelay(instance) =>
                    instance.to_string(),
                ActorKind::LanguageRelay(language) =>
                    format!("in {language}"),
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
                    format!("tag-{tag}"),
                ActorKind::InstanceRelay(instance) =>
                    format!("instance-{instance}"),
                ActorKind::LanguageRelay(language) =>
                    format!("language-{language}"),
            }),
        }
    }
}

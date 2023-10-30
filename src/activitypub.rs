use axum::{response::IntoResponse, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    #[serde(rename = "@context")]
    pub jsonld_context: serde_json::Value,
    #[serde(rename = "type")]
    pub actor_type: String,
    pub id: String,
    pub name: Option<String>,
    pub icon: Option<Media>,
    pub inbox: String,
    pub outbox: Option<String>,
    pub endpoints: Option<ActorEndpoints>,
    #[serde(rename = "publicKey")]
    pub public_key: ActorPublicKey,
    #[serde(rename = "preferredUsername")]
    pub preferred_username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorEndpoints {
    #[serde(rename = "sharedInbox")]
    pub shared_inbox: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorPublicKey {
    pub id: String,
    pub owner: Option<String>,
    #[serde(rename = "publicKeyPem")]
    pub pem: String,
}

/// `ActivityPub` "activity"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action<O> {
    #[serde(rename = "@context")]
    pub jsonld_context: serde_json::Value,
    #[serde(rename = "type")]
    pub action_type: String,
    pub id: String,
    pub actor: String,
    pub to: Option<serde_json::Value>,
    pub object: Option<O>,
}

impl IntoResponse for Actor {
    fn into_response(self) -> axum::response::Response {
        ([("content-type", "application/activity+json")],
         Json(self)).into_response()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    #[serde(rename = "mediaType")]
    pub content_type: Option<String>,
    pub url: String,
}

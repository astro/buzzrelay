use axum::{response::IntoResponse, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    #[serde(rename = "@context")]
    pub jsonld_context: serde_json::Value,
    #[serde(rename = "type")]
    pub actor_type: String,
    pub id: String,
    pub inbox: String,
    #[serde(rename = "publicKey")]
    pub public_key: ActorPublicKey,
    #[serde(rename = "preferredUsername")]
    pub preferred_username: Option<String>,
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
    pub to: Option<String>,
    pub object: Option<O>,
}

impl IntoResponse for Actor {
    fn into_response(self) -> axum::response::Response {
        ([("content-type", "application/activity+json")],
         Json(self)).into_response()
    }
}

use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    #[serde(rename = "@context")]
    pub jsonld_context: serde_json::Value,
    #[serde(rename = "type")]
    pub actor_type: String,
    pub id: String,
    pub inbox: String,
    // pub outbox: String,
    #[serde(rename = "publicKey")]
    pub public_key: ActorPublicKey,
    #[serde(rename = "preferredUsername")]
    pub preferredUsername: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorPublicKey {
    pub id: String,
    pub owner: Option<String>,
    #[serde(rename = "publicKeyPem")]
    pub pem: String,
}

/// ActivityPub "activity"
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

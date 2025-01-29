use std::sync::Arc;

use axum::{
    body::{Bytes, Body},
    extract::{FromRef, FromRequest},
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use http_digest_headers::DigestHeader;
use sigh::{Signature, PublicKey, Key, PrivateKey};

use crate::fetch::authorized_fetch;
use crate::activitypub::Actor;
use crate::error::Error;
use crate::actor_cache::ActorCache;


const SIGNATURE_HEADERS_REQUIRED: &[&str] = &[
    "(request-target)",
    "host", "date",
    "digest",
];

pub struct Endpoint<'a> {
    pub payload: serde_json::Value,
    signature: Signature<'a>,
    pub remote_actor_uri: String,
}

impl<'a, S> FromRequest<S> for Endpoint<'a>
where
    S: Send + Sync,
    Arc<reqwest::Client>: FromRef<S>,
{
    type Rejection = (StatusCode, String);

    async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
        // validate content-type
        let Some(content_type) = req.headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next()) else {
                return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "No content-type".to_string()));
            };
        if ! (content_type.starts_with("application/json") ||
            (content_type.starts_with("application/") && content_type.ends_with("+json")))
        {
            return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "Invalid content-type".to_string()));
        }
        // get signature before consuming req
        let signature = Signature::from(&req);
        // check signature fields
        let signature_headers = signature.headers()
            .ok_or((StatusCode::BAD_REQUEST, "No signed headers".to_string()))?;
        for header in SIGNATURE_HEADERS_REQUIRED {
            if !signature_headers.iter().any(|h| h == header) {
                return Err((StatusCode::BAD_REQUEST, format!("Header {header:?} not signed")));
            }
        }

        // parse digest
        let mut digest_header: String = req.headers().get("digest")
            .ok_or((StatusCode::BAD_REQUEST, "Missing Digest: header".to_string()))?
            .to_str()
            .map_err(|_| (StatusCode::BAD_REQUEST, "Digest: header contained invalid characters".to_string()))?
            .to_string();
        // fixup digest header
        if digest_header.starts_with("SHA-") {
            digest_header.replace_range(..4, "sha-");
        }
        // mastodon uses base64::alphabet::STANDARD, not base64::alphabet::URL_SAFE
        digest_header = digest_header.replace('+', "-")
            .replace('/', "_");
        let digest: DigestHeader = digest_header.parse()
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Cannot parse Digest: header: {e}")))?;
        // read body
        let bytes = Bytes::from_request(req, state).await
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Body: {e}")))?;
        // validate digest
        if ! digest.verify(&bytes).unwrap_or(false) {
            return Err((StatusCode::BAD_REQUEST, "Digest didn't match".to_string()));
        }
        // parse body
        let payload: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| (StatusCode::BAD_REQUEST, "Error parsing JSON".to_string()))?;
        let remote_actor_uri = if let Some(serde_json::Value::String(actor_uri)) = payload.get("actor") {
            actor_uri.to_string()
        } else {
            return Err((StatusCode::BAD_REQUEST, "Actor missing".to_string()));
        };

        Ok(Endpoint { payload, signature, remote_actor_uri })
    }
}

impl<'a> Endpoint<'a> {
    /// Validates the requesting actor
    pub async fn remote_actor(
        &self,
        client: &reqwest::Client,
        cache: &ActorCache,
        key_id: String,
        private_key: Arc<PrivateKey>,
    ) -> Result<Arc<Actor>, Error> {
        let client = client.clone();
        let url = self.remote_actor_uri.clone();
        let remote_actor = cache.get(&self.remote_actor_uri, || async move {
            tracing::info!("GET actor {}", url);
            let actor: Actor = serde_json::from_value(
                authorized_fetch(&client, &url, &key_id, &private_key).await?
            )?;
            Ok(actor)
        }).await?;

        let public_key = PublicKey::from_pem(remote_actor.public_key.pem.as_bytes())?;
        if ! (self.signature.verify(&public_key)?) {
            tracing::error!("Cannot verify signature for {}: {:?}", self.remote_actor_uri, self.payload);
            return Err(Error::SignatureFail(self.remote_actor_uri.clone()));
        }

        Ok(remote_actor)
    }
}

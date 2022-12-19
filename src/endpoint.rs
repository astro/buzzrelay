use std::sync::Arc;

use axum::{
    async_trait,
    body::{Bytes, HttpBody},
    extract::{FromRef, FromRequest},
    http::{header::CONTENT_TYPE, Request, StatusCode}, BoxError,
};

use http_digest_headers::{DigestHeader};

use sigh::{Signature, PublicKey, Key};


use crate::fetch::fetch;
use crate::activitypub::Actor;

const SIGNATURE_HEADERS_REQUIRED: &[&str] = &[
    "(request-target)",
    "host", "date",
    "digest", "content-type",
];

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub payload: serde_json::Value,
    pub actor: Actor,
}

// impl Endpoint {
//     pub fn parse<T: DeserializeOwned>(self) -> Result<T, serde_json::Error> {
//         serde_json::from_value(self.payload)
//     }
// }

#[async_trait]
impl<S, B> FromRequest<S, B> for Endpoint
where
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
    S: Send + Sync,
    Arc<reqwest::Client>: FromRef<S>,
{
    type Rejection = (StatusCode, String);

    async fn from_request(req: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
        // validate content-type
        let content_type = if let Some(content_type) = req.headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()) {
                content_type
            } else {
                return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "No content-type".to_string()));
            };
        if ! content_type.starts_with("application/json") &&
            ! (content_type.starts_with("application/") && content_type.ends_with("+json"))
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
                return Err((StatusCode::BAD_REQUEST, format!("Header {:?} not signed", header)));
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
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Cannot parse Digest: header: {}", e)))?;
        // read body
        let bytes = Bytes::from_request(req, state).await
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Body: {}", e)))?;
        // validate digest
        if ! digest.verify(&bytes).unwrap_or(false) {
            return Err((StatusCode::BAD_REQUEST, "Digest didn't match".to_string()));
        }
        // parse body
        let payload: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| (StatusCode::BAD_REQUEST, "Error parsing JSON".to_string()))?;
        let actor_uri = if let Some(serde_json::Value::String(actor_uri)) = payload.get("actor") {
            actor_uri
        } else {
            return Err((StatusCode::BAD_REQUEST, "Actor missing".to_string()));
        };

        // validate actor
        let client = Arc::from_ref(state);
        let actor: Actor =
            serde_json::from_value(
                fetch(&client, actor_uri).await
                    .map_err(|e| (StatusCode::BAD_GATEWAY, format!("{}", e)))?
            ).map_err(|e| (StatusCode::BAD_GATEWAY, format!("Invalid actor: {}", e)))?;
        let public_key = PublicKey::from_pem(actor.public_key.pem.as_bytes())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e)))?;
        if !(signature.verify(&public_key)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e)))?)
        {
            return Err((StatusCode::BAD_REQUEST, "Signature verification failed".to_string()));
        }
        
        return Ok(Endpoint { payload, actor });
    }
}

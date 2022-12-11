use axum::{
    async_trait,
    extract::{FromRequest, FromRef},
    http::{header::CONTENT_TYPE, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Form, Json, RequestExt, Router,
};
use serde::{Deserialize, Serialize};
use sigh::{PrivateKey, PublicKey, alg::{RsaSha256, Algorithm}, Key};
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod fetch;
pub use fetch::fetch;
mod send;
pub use send::send;
mod activitypub;
mod endpoint;

#[derive(Debug, Clone)]
struct State {
    client: Arc<reqwest::Client>,
    private_key: PrivateKey,
    public_key: PublicKey,
}


impl FromRef<State> for Arc<reqwest::Client> {
    fn from_ref(state: &State) -> Arc<reqwest::Client> {
        state.client.clone()
    }
}

async fn actor(axum::extract::State(state): axum::extract::State<State>) -> impl IntoResponse {
    let id = "https://relay.fedi.buzz/".to_string();
    Json(activitypub::Actor {
        jsonld_context: serde_json::Value::String(
            "https://www.w3.org/ns/activitystreams".to_string()
        ),
        actor_type: "Application".to_string(),
        id: id.clone(),
        inbox: id.clone(),
        outbox: id.clone(),
        public_key: activitypub::ActorPublicKey {
            id: id.clone(),
            owner: Some(id),
            pem: state.public_key.to_pem().unwrap(),
        },
    })
}

async fn handler(
    axum::extract::State(state): axum::extract::State<State>,
    endpoint: endpoint::Endpoint,
) -> Response {
    let action = match serde_json::from_value::<activitypub::Action<serde_json::Value>>(endpoint.payload.clone()) {
        Ok(action) => action,
        Err(e) => return (
            StatusCode::BAD_REQUEST,
            format!("Bad action: {:?}", e)
        ).into_response(),
    };
    dbg!(&action);
    
    if action.action_type == "Follow" {
        let private_key = state.private_key.clone();
        let client = state.client.clone();
        tokio::spawn(async move {
            let accept = activitypub::Action {
                action_type: "Accept".to_string(),
                actor: "https://relay.fedi.buzz/".to_string(),
                to: Some(endpoint.actor.id),
                object: Some(endpoint.payload),
            };
            dbg!(serde_json::to_string_pretty(&accept));
            send::send(
                client.as_ref(), &endpoint.actor.inbox,
                "https://relay.fedi.buzz/",
                &private_key,
                accept,
            ).await
                .map_err(|e| tracing::error!("post: {}", e));
        });
        
        StatusCode::OK.into_response()
    } else {
        (StatusCode::BAD_REQUEST, "Not a recognized request").into_response()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "buzzrelay=trace,tower_http=trace,axum=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (private_key, public_key) = RsaSha256.generate_keys().unwrap();

    let app = Router::new()
        .route("/", get(actor).post(handler))
        .with_state(State {
            client: Arc::new(reqwest::Client::new()),
            private_key, public_key,
        });

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

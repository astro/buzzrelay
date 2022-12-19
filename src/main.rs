use axum::{
    async_trait,
    extract::{FromRequest, FromRef, Path, Query},
    http::{header::CONTENT_TYPE, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Form, Json, RequestExt, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sigh::{PrivateKey, PublicKey, alg::{RsaSha256, Algorithm}, Key};
use std::{net::SocketAddr, sync::Arc, time::Duration, collections::HashMap};
use std::{panic, process};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod actor;
mod db;
mod fetch;
pub use fetch::fetch;
mod send;
mod stream;
mod relay;
mod activitypub;
mod endpoint;


#[derive(Clone)]
struct State {
    database: db::Database,
    client: Arc<reqwest::Client>,
    hostname: Arc<String>,
    priv_key: PrivateKey,
    pub_key: PublicKey,
}


impl FromRef<State> for Arc<reqwest::Client> {
    fn from_ref(state: &State) -> Arc<reqwest::Client> {
        state.client.clone()
    }
}

async fn webfinger(
    axum::extract::State(state): axum::extract::State<State>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let resource = match params.get("resource") {
        Some(resource) => resource,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let (target_kind, target_host) =
        if resource.starts_with("acct:tag-") {
            let off = "acct:tag-".len();
            let at = resource.find('@');
            (actor::ActorKind::TagRelay(resource[off..at.unwrap_or(resource.len())].to_string()),
             at.map_or_else(|| state.hostname.clone(), |at| Arc::new(resource[at + 1..].to_string())))
        } else if resource.starts_with("acct:instance-") {
            let off = "acct:instance-".len();
            let at = resource.find('@');
            (actor::ActorKind::InstanceRelay(resource[off..at.unwrap_or(resource.len())].to_string()),
             at.map_or_else(|| state.hostname.clone(), |at| Arc::new(resource[at + 1..].to_string())))
        } else {
            return StatusCode::NOT_FOUND.into_response();
        };
    let target = actor::Actor {
        host: target_host,
        kind: target_kind,
    };
    Json(json!({
        "subject": &resource,
        "aliases": &[
            target.uri(),
        ],
        "links": &[json!({
            "rel": "self",
            "type": "application/activity+json",
            "href": target.uri(),
        })],
    })).into_response()
}

async fn get_tag_actor(
    axum::extract::State(state): axum::extract::State<State>,
    Path(tag): Path<String>
) -> Response {
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind: actor::ActorKind::TagRelay(tag.to_lowercase()),
    };
    target.as_activitypub(&state.pub_key)
        .into_response()
}

async fn get_instance_actor(
    axum::extract::State(state): axum::extract::State<State>,
    Path(instance): Path<String>
) -> Response {
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind: actor::ActorKind::InstanceRelay(instance.to_lowercase()),
    };
    target.as_activitypub(&state.pub_key)
        .into_response()
}

async fn post_tag_relay(
    axum::extract::State(state): axum::extract::State<State>,
    Path(tag): Path<String>,
    endpoint: endpoint::Endpoint
) -> Response {
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind: actor::ActorKind::TagRelay(tag.to_lowercase()),
    };
    post_relay(state, endpoint, target).await
}

async fn post_instance_relay(
    axum::extract::State(state): axum::extract::State<State>,
    Path(instance): Path<String>,
    endpoint: endpoint::Endpoint
) -> Response {
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind: actor::ActorKind::InstanceRelay(instance.to_lowercase()),
    };
    post_relay(state, endpoint, target).await
}

async fn post_relay(
    state: State,
    endpoint: endpoint::Endpoint,
    target: actor::Actor
) -> Response {
    dbg!(&endpoint);
    let action = match serde_json::from_value::<activitypub::Action<serde_json::Value>>(endpoint.payload.clone()) {
        Ok(action) => action,
        Err(e) => return (
            StatusCode::BAD_REQUEST,
            format!("Bad action: {:?}", e)
        ).into_response(),
    };

    if action.action_type == "Follow" {
        let priv_key = state.priv_key.clone();
        let client = state.client.clone();
        tokio::spawn(async move {
            let accept = activitypub::Action {
                jsonld_context: serde_json::Value::String("https://www.w3.org/ns/activitystreams".to_string()),
                action_type: "Accept".to_string(),
                actor: target.uri(),
                to: Some(endpoint.actor.id.clone()),
                id: action.id,
                object: Some(endpoint.payload),
            };
            let result = send::send(
                client.as_ref(), &endpoint.actor.inbox,
                &target.key_id(),
                &priv_key,
                &accept,
            ).await;
            match result {
                Ok(()) => {
                    state.database.add_follow(
                        &endpoint.actor.id,
                        &endpoint.actor.inbox,
                        &target.uri(),
                    ).await.unwrap();
                }
                Err(e) => {
                    tracing::error!("post accept: {}", e);
                }
            }
        });

        (StatusCode::ACCEPTED,
         [("content-type", "application/activity+json")],
         "{}"
        ).into_response()
    } else {
        // TODO: Undo Follow
        (StatusCode::BAD_REQUEST, "Not a recognized request").into_response()
    }
}

#[tokio::main]
async fn main() {
    exit_on_panic();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "buzzrelay=trace,tower_http=trace,axum=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::load(
        &std::env::args()
            .skip(1)
            .next()
            .expect("Call with config.yaml")
    );
    let database = db::Database::connect(&config.db).await;

    let stream_rx = stream::spawn(config.upstream.clone());
    let client = Arc::new(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION"),
            ))
            .pool_max_idle_per_host(1)
            .pool_idle_timeout(Some(Duration::from_secs(5)))
            .build()
            .unwrap()
    );
    let hostname = Arc::new(config.hostname.clone());
    relay::spawn(client.clone(), hostname.clone(), database.clone(), config.priv_key(), stream_rx);

    let app = Router::new()
        .route("/tag/:tag", get(get_tag_actor).post(post_tag_relay))
        .route("/instance/:instance", get(get_instance_actor).post(post_instance_relay))
        .route("/.well-known/webfinger", get(webfinger))
        .with_state(State {
            database,
            client,
            hostname,
            priv_key: config.priv_key(),
            pub_key: config.pub_key(),
        });

    let addr = SocketAddr::from(([127, 0, 0, 1], config.listen_port));
    tracing::info!("serving on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn exit_on_panic() {
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));
}

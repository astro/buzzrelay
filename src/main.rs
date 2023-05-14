use axum::{
    extract::{FromRef, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get, Json, Router,
};
use axum_extra::routing::SpaRouter;
use metrics::increment_counter;
use metrics_util::MetricKindMask;
use metrics_exporter_prometheus::PrometheusBuilder;
use serde_json::json;
use sigh::{PrivateKey, PublicKey};
use std::{net::SocketAddr, sync::Arc, time::Duration, collections::HashMap};
use std::{panic, process};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod error;
mod config;
mod actor;
mod db;
mod digest;
mod fetch;
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

fn track_request(method: &'static str, controller: &'static str, result: &'static str) {
    increment_counter!("api_http_requests_total", "controller" => controller, "method" => method, "result" => result);
}

async fn webfinger(
    axum::extract::State(state): axum::extract::State<State>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let resource = match params.get("resource") {
        Some(resource) => resource,
        None => {
            track_request("GET", "webfinger", "invalid");
            return StatusCode::NOT_FOUND.into_response();
        },
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
            track_request("GET", "webfinger", "not_found");
            return StatusCode::NOT_FOUND.into_response();
        };
    track_request("GET", "webfinger", "found");
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
    track_request("GET", "actor", "tag");
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind: actor::ActorKind::from_tag(&tag),
    };
    target.as_activitypub(&state.pub_key)
        .into_response()
}

async fn get_instance_actor(
    axum::extract::State(state): axum::extract::State<State>,
    Path(instance): Path<String>
) -> Response {
    track_request("GET", "actor", "instance");
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
    endpoint: endpoint::Endpoint<'_>
) -> Response {
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind: actor::ActorKind::from_tag(&tag),
    };
    post_relay(state, endpoint, target).await
}

async fn post_instance_relay(
    axum::extract::State(state): axum::extract::State<State>,
    Path(instance): Path<String>,
    endpoint: endpoint::Endpoint<'_>
) -> Response {
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind: actor::ActorKind::InstanceRelay(instance.to_lowercase()),
    };
    post_relay(state, endpoint, target).await
}

async fn post_relay(
    state: State,
    endpoint: endpoint::Endpoint<'_>,
    target: actor::Actor
) -> Response {
    let remote_actor = match endpoint.remote_actor(&state.client, &target.key_id(), &state.priv_key).await {
        Ok(remote_actor) => remote_actor,
        Err(e) => {
            track_request("POST", "relay", "bad_actor");
            return (
                StatusCode::BAD_REQUEST,
                format!("Bad actor: {:?}", e)
            ).into_response();
        }
    };
    let action = match serde_json::from_value::<activitypub::Action<serde_json::Value>>(endpoint.payload.clone()) {
        Ok(action) => action,
        Err(e) => {
            track_request("POST", "relay", "bad_action");
            return (
                StatusCode::BAD_REQUEST,
                format!("Bad action: {:?}", e)
            ).into_response();
        }
    };
    let object_type = action.object
        .and_then(|object| object.get("type").cloned())
        .and_then(|object_type| object_type.as_str().map(std::string::ToString::to_string));

    if action.action_type == "Follow" {
        let priv_key = state.priv_key.clone();
        let client = state.client.clone();
        tokio::spawn(async move {
            let accept = activitypub::Action {
                jsonld_context: serde_json::Value::String("https://www.w3.org/ns/activitystreams".to_string()),
                action_type: "Accept".to_string(),
                actor: target.uri(),
                to: Some(json!(remote_actor.id.clone())),
                id: action.id,
                object: Some(endpoint.payload),
            };
            let result = send::send(
                client.as_ref(), &remote_actor.inbox,
                &target.key_id(),
                &priv_key,
                &accept,
            ).await;
            match result {
                Ok(()) => {
                    match state.database.add_follow(
                        &remote_actor.id,
                        &remote_actor.inbox,
                        &target.uri(),
                    ).await {
                        Ok(()) => {
                            track_request("POST", "relay", "follow");
                        }
                        Err(e) => {
                            // duplicate key constraint
                            tracing::error!("add_follow: {}", e);
                            track_request("POST", "relay", "follow_error");
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("post accept: {}", e);
                    track_request("POST", "relay", "follow_accept_error");
                }
            }
        });

        (StatusCode::ACCEPTED,
         [("content-type", "application/activity+json")],
         "{}"
        ).into_response()
    } else if action.action_type == "Undo" && object_type == Some("Follow".to_string()) {
        match state.database.del_follow(
            &remote_actor.id,
            &target.uri(),
        ).await {
            Ok(()) => {
                track_request("POST", "relay", "unfollow");
                (StatusCode::ACCEPTED,
                 [("content-type", "application/activity+json")],
                 "{}"
                ).into_response()
            }
            Err(e) => {
                tracing::error!("del_follow: {}", e);
                track_request("POST", "relay", "unfollow_error");
                (StatusCode::INTERNAL_SERVER_ERROR,
                 format!("{}", e)
                 ).into_response()
            }
        }
    } else {
        track_request("POST", "relay", "unrecognized");
        (StatusCode::BAD_REQUEST, "Not a recognized request").into_response()
    }
}

async fn nodeinfo(axum::extract::State(state): axum::extract::State<State>) -> Response {
    let follows_count = state.database.get_follows_count()
        .await
        .unwrap_or(0);
    let followers_count = state.database.get_followers_count()
        .await
        .unwrap_or(0);

    Json(json!({
        "version": "2.1",
        "software": {
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
            "repository": env!("CARGO_PKG_REPOSITORY"),
            "homepage": env!("CARGO_PKG_HOMEPAGE"),
        },
        "protocols": ["activitypub"],
        "services": {
            "inbound": [],
            "outbound": []
        },
        "openRegistrations": false,
        "usage": {
            "users": {
                "total": followers_count,
                "activeHalfyear": followers_count,
                "activeMonth": followers_count,
            },
            "localPosts": follows_count,
            "localComments": 0
        },
        "metadata": {
            "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        }
    })).into_response()
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
        &std::env::args().nth(1)
            .expect("Call with config.yaml")
    );
    let priv_key = config.priv_key();
    let pub_key = config.pub_key();

    let recorder = PrometheusBuilder::new()
        .add_global_label("application", env!("CARGO_PKG_NAME"))
        .idle_timeout(MetricKindMask::ALL, Some(Duration::from_secs(600)))
        .install_recorder()
        .unwrap();

    let database = db::Database::connect(&config.db).await;

    let stream_rx = stream::spawn(config.streams.into_iter());
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
    relay::spawn(client.clone(), hostname.clone(), database.clone(), priv_key.clone(), stream_rx);

    let app = Router::new()
        .route("/tag/:tag", get(get_tag_actor).post(post_tag_relay))
        .route("/instance/:instance", get(get_instance_actor).post(post_instance_relay))
        .route("/.well-known/webfinger", get(webfinger))
        .route("/.well-known/nodeinfo", get(nodeinfo))
        .route("/metrics", get(|| async move {
            recorder.render().into_response()
        }))
        .with_state(State {
            database,
            client,
            hostname,
            priv_key,
            pub_key,
        })
        .merge(SpaRouter::new("/", "static"));

    let addr = SocketAddr::from(([127, 0, 0, 1], config.listen_port));
    let server = axum::Server::bind(&addr)
        .serve(app.into_make_service());

    tracing::info!("serving on {}", addr);
    systemd::daemon::notify(false, [(systemd::daemon::STATE_READY, "1")].iter())
        .unwrap();
    server.await
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

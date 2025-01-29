use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get, Json, Router,
};
use tower_http::services::ServeDir;
use metrics::counter;
use metrics_util::MetricKindMask;
use metrics_exporter_prometheus::PrometheusBuilder;
use serde_json::json;
use std::{net::SocketAddr, time::Duration, collections::HashMap};
use std::{panic, process};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use reqwest::Url;

mod error;
mod config;
mod state;
mod actor;
mod db;
mod digest;
mod fetch;
mod send;
mod stream;
mod relay;
mod activitypub;
mod actor_cache;
mod endpoint;

use actor::Actor;
use state::State;


fn track_request(method: &'static str, controller: &'static str, result: &'static str) {
    counter!("api_http_requests_total", "controller" => controller, "method" => method, "result" => result)
        .increment(1);
}

async fn webfinger(
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(resource) = params.get("resource") else {
        track_request("GET", "webfinger", "invalid");
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(target) = Actor::from_uri(resource) else {
        track_request("GET", "webfinger", "not_found");
        return StatusCode::NOT_FOUND.into_response();
    };
    track_request("GET", "webfinger", "found");
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

async fn get_language_actor(
    axum::extract::State(state): axum::extract::State<State>,
    Path(language): Path<String>
) -> Response {
    track_request("GET", "actor", "language");
    let Some(kind) = actor::ActorKind::from_language(&language) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind,
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

async fn post_language_relay(
    axum::extract::State(state): axum::extract::State<State>,
    Path(language): Path<String>,
    endpoint: endpoint::Endpoint<'_>
) -> Response {
    let Some(kind) = actor::ActorKind::from_language(&language) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let target = actor::Actor {
        host: state.hostname.clone(),
        kind,
    };
    post_relay(state, endpoint, target).await
}

async fn post_relay(
    state: State,
    endpoint: endpoint::Endpoint<'_>,
    mut target: actor::Actor
) -> Response {
    if let Some((redis, in_topic)) = &state.redis {
        if let Ok(data) = serde_json::to_vec(&endpoint.payload) {
            if let Err(e) = redis::Cmd::publish(in_topic.as_ref(), data)
                .query_async::<redis::Value>(&mut redis.clone())
                .await
            {
                tracing::error!("redis publish: {}", e);
            }
        }
    }

    let remote_actor = endpoint.remote_actor(&state.client, &state.actor_cache, target.key_id(), state.priv_key.clone())
        .await
        .map_err(|e| {
            track_request("POST", "relay", "bad_actor");
            tracing::error!("post_relay bad actor: {e:?}");
            e
        });

    let action = match serde_json::from_value::<activitypub::Action<serde_json::Value>>(endpoint.payload.clone()) {
        Ok(action) => action,
        Err(e) => {
            track_request("POST", "relay", "bad_action");
            tracing::error!("post_relay bad action: {e:?}");
            return (
                StatusCode::BAD_REQUEST,
                format!("Bad action: {e:?}")
            ).into_response();
        }
    };
    let object_type = action.object.as_ref()
        .and_then(|object| object.get("type").cloned())
        .and_then(|object_type| object_type.as_str().map(std::string::ToString::to_string));

    if action.action_type == "Follow" {
        let Ok(remote_actor) = remote_actor else {
            return (StatusCode::BAD_REQUEST, "Invalid actor").into_response();
        };
        if let Some(action_target) = action.object.and_then(|object| Actor::from_object(&object)) {
            if action_target.host == state.hostname {
                // A sharedInbox receives the actual follow target in the
                // `object` field.
                target = action_target;
            }
        }
        let priv_key = state.priv_key.clone();
        let client = state.client.clone();
        tokio::spawn(async move {
            let accept_id = format!(
                "https://{}/activity/accept/{}/{}",
                state.hostname,
                urlencoding::encode(&target.uri()),
                urlencoding::encode(&remote_actor.inbox),
            );
            let accept = activitypub::Action {
                jsonld_context: serde_json::Value::String("https://www.w3.org/ns/activitystreams".to_string()),
                action_type: "Accept".to_string(),
                actor: target.uri(),
                to: Some(json!(remote_actor.id.clone())),
                id: accept_id,
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
        let Ok(remote_actor) = remote_actor else {
            return (StatusCode::BAD_REQUEST, "Invalid actor").into_response();
        };
        if let Some(action_target) = action.object
            .and_then(|object| object.get("object")
                      .and_then(Actor::from_object))
        {
            if action_target.host == state.hostname {
                // A sharedInbox receives the actual follow target in the
                // `object` field.
                target = action_target;
            }
        }
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
                 format!("{e}")
                 ).into_response()
            }
        }
    } else {
        track_request("POST", "relay", "unrecognized");
        (StatusCode::ACCEPTED,
         [("content-type", "application/activity+json")],
         "{}"
        ).into_response()
    }
}

/// An empty `ActivityStreams` outbox just to satisfy the spec
async fn outbox() -> Response {
    Json(json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "summary": "buzzrelay stub outbox",
        "type": "OrderedCollection",
        "totalItems": 0,
        "orderedItems": []
    })).into_response()
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
                "total": 0,
                "activeHalfyear": followers_count,
                "activeMonth": 0,
            },
            "localPosts": follows_count,
            "localComments": 0
        },
        "metadata": {
            "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        },
        "links": vec![
            json!({
                "rel": "http://nodeinfo.diaspora.software/ns/schema/2.1",
                "href": format!("https://{}/.well-known/nodeinfo", state.hostname),
            }),
        ],
    })).into_response()
}

/// Expected by `AodeRelay`
async fn instanceinfo() -> Response {
    Json(json!({
        "title": env!("CARGO_PKG_NAME"),
        "description": "#FediBuzz Relay",
        "version": env!("CARGO_PKG_VERSION"),
        "registrations": false,
        "default_approval": false,
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

    let recorder = PrometheusBuilder::new()
        .add_global_label("application", env!("CARGO_PKG_NAME"))
        .idle_timeout(MetricKindMask::ALL, Some(Duration::from_secs(600)))
        .install_recorder()
        .unwrap();

    let config = config::Config::load(
        &std::env::args().nth(1)
            .expect("Call with config.yaml")
    );
    let database = db::Database::connect(&config.db).await;
    let mut redis = None;
    if let Some(redis_config) = config.redis.clone() {
        let mut redis_url = Url::parse(&redis_config.connection)
            .expect("redis.connection");
        let redis_password = std::fs::read_to_string(redis_config.password_file)
            .expect("redis.password_file");
        redis_url.set_password(Some(&redis_password)).unwrap();
        let client = redis::Client::open(redis_url)
            .expect("redis::Client");
        let manager = redis::aio::ConnectionManager::new(client)
            .await
            .expect("redis::Client");
        redis = Some((manager, redis_config.in_topic));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent(format!(
            "{}/{} (+https://{})",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            config.hostname,
        ))
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(Some(Duration::from_secs(5)))
        .build()
        .unwrap();
    let state = State::new(config.clone(), database, redis, client);

    let stream_rx = stream::spawn(config.streams.clone().into_iter());
    relay::spawn(state.clone(), stream_rx);

    let app = Router::new()
        .route("/tag/{tag}", get(get_tag_actor).post(post_tag_relay))
        .route("/instance/{instance}", get(get_instance_actor).post(post_instance_relay))
        .route("/language/{language}", get(get_language_actor).post(post_language_relay))
        .route("/tag/{tag}/outbox", get(outbox))
        .route("/instance/{instance}/outbox", get(outbox))
        .route("/language/{language}/outbox", get(outbox))
        .route("/.well-known/webfinger", get(webfinger))
        .route("/.well-known/nodeinfo", get(nodeinfo))
        .route("/api/v1/instance", get(instanceinfo))
        .route("/metrics", get(|| async move {
            recorder.render().into_response()
        }))
        .with_state(state)
        .fallback_service(ServeDir::new("static"));

    let addr = SocketAddr::from(([127, 0, 0, 1], config.listen_port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server = axum::serve(listener, app.into_make_service());

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

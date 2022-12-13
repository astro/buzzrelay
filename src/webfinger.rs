use std::collections::HashMap;

use axum::{
    async_trait,
    body::{Bytes, HttpBody},
    extract::{Query},
    http::{header::CONTENT_TYPE, Request, StatusCode},
    Json,
    response::{IntoResponse, Response},
    routing::post,
    Form, RequestExt, Router, BoxError,
};
use serde_json::json;

pub async fn webfinger(Query(params): Query<HashMap<String, String>>) -> Response {
    let resource = match params.get("resource") {
        Some(resource) => resource,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    Json(json!({
        "subject": &resource,
        "aliases": &[
            "https://relay.fedi.buzz/actor",
        ],
        "links": &[json!({
            "rel": "self",
            "type": "application/activity+json",
            "href": "https://relay.fedi.buzz/actor",
        })],
    })).into_response()
}

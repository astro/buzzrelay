use std::{sync::Arc, collections::{HashSet, HashMap}, time::{Duration, Instant}};
use futures::{channel::mpsc::{channel, Sender}, StreamExt};
use metrics::{counter, histogram};
use serde::Deserialize;
use serde_json::json;
use sigh::PrivateKey;
use tokio::sync::mpsc::Receiver;
use crate::{send, actor, state::State};

#[derive(Deserialize)]
struct Post<'a> {
    pub url: Option<&'a str>,
    pub uri: &'a str,
    pub tags: Option<Vec<Tag<'a>>>,
    pub language: Option<&'a str>,
}

impl Post<'_> {
    pub fn host(&self) -> Option<String> {
        reqwest::Url::parse(self.url?)
            .ok()
            .and_then(|url| url.domain()
                      .map(str::to_lowercase)
            )
    }

    pub fn tags(&self) -> Vec<String> {
        match &self.tags {
            None =>
                vec![],
            Some(tags) =>
                tags.iter()
                .map(|tag| tag.name.to_string())
                .collect()
        }
    }

    fn relay_target_kinds(&self) -> impl Iterator<Item = actor::ActorKind> {
        self.host()
            .into_iter()
            .map(actor::ActorKind::InstanceRelay)
            .chain(
                self.tags()
                    .into_iter()
                    .flat_map(|ref s| {
                        // Don't handle the empty hashtag `#`
                        if s.is_empty() {
                            return vec![];
                        }

                        let actor1 = actor::ActorKind::from_tag(s);

                        // Distribute hashtags that end in a date to
                        // followers of the hashtag with the date
                        // stripped. Example: #dd1302 -> #dd
                        let mut first_trailing_digit = 0;
                        let mut scanning_digits = false;
                        for (pos, c) in s.char_indices() {
                            if char::is_digit(c, 10) {
                                if ! scanning_digits {
                                    first_trailing_digit = pos;
                                    scanning_digits = true;
                                }
                            } else {
                                scanning_digits = false;
                            }
                        }
                        if scanning_digits && first_trailing_digit > 0 {
                            let tag = &s[..first_trailing_digit];
                            let actor2 = actor::ActorKind::from_tag(tag);
                            vec![actor1, actor2]
                        } else {
                            vec![actor1]
                        }
                    })
            )
            .chain(
                self.language
                    .and_then(actor::ActorKind::from_language)
            )
    }

    pub fn relay_targets(&self, hostname: Arc<String>) -> impl Iterator<Item = actor::Actor> {
        self.relay_target_kinds()
            .map(move |kind| actor::Actor {
                host: hostname.clone(),
                kind,
            })
    }
}

#[derive(Deserialize)]
struct Tag<'a> {
    pub name: &'a str,
}

struct Job {
    post_url: Arc<String>,
    actor_id: Arc<String>,
    body: Arc<Vec<u8>>,
    key_id: String,
    private_key: Arc<PrivateKey>,
    inbox_url: reqwest::Url,
}

fn spawn_worker(client: Arc<reqwest::Client>) -> Sender<Job> {
    let (tx, mut rx) = channel(512);

    tokio::spawn(async move {
        let mut errors = 0u32;
        let mut last_request = None;

        while let Some(Job { post_url, actor_id, key_id, private_key, body, inbox_url }) = rx.next().await {
            if errors > 0 && last_request.is_some_and(|last_request: Instant|
                last_request.elapsed() < Duration::from_secs(10) * errors
            ) {
                // there have been errors, skip for time proportional
                // to the number of subsequent errors
                tracing::trace!("skip {} from {} to {}", post_url, actor_id, inbox_url);
                continue;
            }

            tracing::debug!("relay {} from {} to {}", post_url, actor_id, inbox_url);
            last_request = Some(Instant::now());
            if let Err(e) = send::send_raw(
                &client, inbox_url.as_str(),
                &key_id, &private_key, body
            ).await {
                tracing::error!("relay::send {:?}", e);
                errors = errors.saturating_add(1);
            } else {
                // success
                errors = 0;
                systemd::daemon::notify(
                    false, [
                        (systemd::daemon::STATE_WATCHDOG, "1")
                    ].iter()
                ).unwrap();
            }
        }

        panic!("Worker dead");
    });

    tx
}

pub fn spawn(
    state: State,
    mut stream_rx: Receiver<String>
) {
    tokio::spawn(async move {
        let mut workers = HashMap::new();

        while let Some(data) = stream_rx.recv().await {
            let t1 = Instant::now();
            let post: Post = match serde_json::from_str(&data) {
                Ok(post) => post,
                Err(e) => {
                    tracing::error!("parse error: {}", e);
                    tracing::trace!("data: {}", data);
                    continue;
                }
            };
            let post_url = if let Some(url) = post.url {
                Arc::new(url.to_string())
            } else {
                // skip reposts
                counter!("relay_posts_total", "action" => "skip")
                    .increment(1);
                continue;
            };
            let mut seen_actors = HashSet::new();
            let mut seen_inboxes = HashSet::new();
            let published = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            for actor in post.relay_targets(state.hostname.clone()) {
                if seen_actors.contains(&actor) {
                    continue;
                }

                let actor_id = Arc::new(actor.uri());
                let announce_id = format!("https://{}/announce/{}", state.hostname, urlencoding::encode(&post_url));
                let body = json!({
                    "@context": "https://www.w3.org/ns/activitystreams",
                    "type": "Announce",
                    "actor": *actor_id,
                    "published": &published,
                    "to": ["https://www.w3.org/ns/activitystreams#Public"],
                    "object": &post.uri,
                    "id": announce_id,
                });
                let Ok(post_url_url) = reqwest::Url::parse(&post_url) else { continue; };
                let body = Arc::new(
                    serde_json::to_vec(&body)
                        .unwrap()
                );
                for inbox in state.database.get_following_inboxes(&actor_id).await.unwrap() {
                    let Ok(inbox_url) = reqwest::Url::parse(&inbox) else { continue; };

                    // Avoid duplicate processing.
                    if seen_inboxes.contains(&inbox) {
                        continue;
                    }
                    seen_inboxes.insert(inbox);

                    // Prevent relaying back to the originating instance.
                    if inbox_url.host_str() == post_url_url.host_str() {
                        continue;
                    }

                    // Lookup/create worker queue per inbox.
                    let tx = workers.entry(inbox_url.host_str().unwrap_or("").to_string())
                        .or_insert_with(|| spawn_worker(state.client.clone()));
                    // Create queue item.
                    let job = Job {
                        post_url: post_url.clone(),
                        actor_id: actor_id.clone(),
                        body: body.clone(),
                        key_id: actor.key_id(),
                        private_key: state.priv_key.clone(),
                        inbox_url,
                    };
                    // Enqueue job for worker.
                    let _ = tx.try_send(job);
                }

                seen_actors.insert(actor);
            }
            if seen_inboxes.is_empty() {
                counter!("relay_posts_total", "action" => "no_relay")
                    .increment(1);
            } else {
                counter!("relay_posts_total", "action" => "relay")
                    .increment(1);
            }
            let t2 = Instant::now();
            histogram!("relay_post_duration").record(t2 - t1);
        }
    });
}

#[cfg(test)]
mod test {
    use super::*;
    use actor::ActorKind;

    #[test]
    fn post_relay_kind() {
        let post = Post {
            url: Some("http://example.com/post/1"),
            uri: "http://example.com/post/1",
            tags: Some(vec![Tag {
                name: "foo",
            }]),
            language: Some("en"),
        };
        let mut kinds = post.relay_target_kinds();
        assert_eq!(kinds.next(), Some(ActorKind::InstanceRelay("example.com".to_string())));
        assert_eq!(kinds.next(), Some(ActorKind::TagRelay("foo".to_string())));
        assert_eq!(kinds.next(), Some(ActorKind::LanguageRelay("en".to_string())));
        assert_eq!(kinds.next(), None);
    }

    #[test]
    fn post_relay_kind_empty() {
        let post = Post {
            url: Some("http://example.com/post/1"),
            uri: "http://example.com/post/1",
            tags: Some(vec![Tag {
                name: "",
            }]),
            language: None,
        };
        let mut kinds = post.relay_target_kinds();
        assert_eq!(kinds.next(), Some(ActorKind::InstanceRelay("example.com".to_string())));
        assert_eq!(kinds.next(), None);
    }

    #[test]
    fn post_relay_kind_numeric() {
        let post = Post {
            url: Some("http://example.com/post/1"),
            uri: "http://example.com/post/1",
            tags: Some(vec![Tag {
                name: "23",
            }]),
            language: None,
        };
        let mut kinds = post.relay_target_kinds();
        assert_eq!(kinds.next(), Some(ActorKind::InstanceRelay("example.com".to_string())));
        assert_eq!(kinds.next(), Some(ActorKind::TagRelay("23".to_string())));
        assert_eq!(kinds.next(), None);
    }

    #[test]
    fn post_relay_kind_date() {
        let post = Post {
            url: Some("http://example.com/post/1"),
            uri: "http://example.com/post/1",
            tags: Some(vec![Tag {
                name: "dd1302",
            }]),
            language: None,
        };
        let mut kinds = post.relay_target_kinds();
        assert_eq!(kinds.next(), Some(ActorKind::InstanceRelay("example.com".to_string())));
        assert_eq!(kinds.next(), Some(ActorKind::TagRelay("dd1302".to_string())));
        assert_eq!(kinds.next(), Some(ActorKind::TagRelay("dd".to_string())));
        assert_eq!(kinds.next(), None);
    }

    #[test]
    fn post_relay_kind_ja() {
        let post = Post {
            url: Some("http://example.com/post/1"),
            uri: "http://example.com/post/1",
            tags: Some(vec![Tag {
                name: "スコティッシュ・フォールド・ロングヘアー",
            }]),
            language: Some("ja"),
        };
        let mut kinds = post.relay_target_kinds();
        assert_eq!(kinds.next(), Some(ActorKind::InstanceRelay("example.com".to_string())));
        assert_eq!(kinds.next(), Some(ActorKind::TagRelay("sukoteitusiyuhuorudoronguhea".to_string())));
        assert_eq!(kinds.next(), Some(ActorKind::LanguageRelay("ja".to_string())));
        assert_eq!(kinds.next(), None);
    }

    #[test]
    fn post_relay_language_long() {
        let post = Post {
            url: Some("http://example.com/post/1"),
            uri: "http://example.com/post/1",
            tags: None,
            language: Some("de_CH"),
        };
        let mut kinds = post.relay_target_kinds();
        assert_eq!(kinds.next(), Some(ActorKind::InstanceRelay("example.com".to_string())));
        assert_eq!(kinds.next(), Some(ActorKind::LanguageRelay("de".to_string())));
        assert_eq!(kinds.next(), None);
    }

    #[test]
    fn post_relay_language_invalid() {
        let post = Post {
            url: Some("http://example.com/post/1"),
            uri: "http://example.com/post/1",
            tags: None,
            language: Some("23q"),
        };
        let mut kinds = post.relay_target_kinds();
        assert_eq!(kinds.next(), Some(ActorKind::InstanceRelay("example.com".to_string())));
        assert_eq!(kinds.next(), None);
    }
}

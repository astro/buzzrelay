use axum::{
    extract::FromRef,
};
use sigh::{PrivateKey, PublicKey};
use std::sync::Arc;
use crate::{config::Config, db::Database, endpoint::ActorCache};

#[derive(Clone)]
pub struct State {
    pub database: Database,
    pub redis: Option<(redis::aio::ConnectionManager, Arc<String>)>,
    pub client: Arc<reqwest::Client>,
    pub actor_cache: ActorCache,
    pub hostname: Arc<String>,
    pub priv_key: Arc<PrivateKey>,
    pub pub_key: Arc<PublicKey>,
}


impl FromRef<State> for Arc<reqwest::Client> {
    fn from_ref(state: &State) -> Arc<reqwest::Client> {
        state.client.clone()
    }
}

impl State {
    pub fn new(config: Config, database: Database, redis: Option<(redis::aio::ConnectionManager, String)>, client: reqwest::Client) -> Self {
        let priv_key = Arc::new(config.priv_key());
        let pub_key = Arc::new(config.pub_key());
        State {
            database,
            redis: redis.map(|(connection, in_topic)| (connection, Arc::new(in_topic))),
            client: Arc::new(client),
            actor_cache: Default::default(),
            hostname: Arc::new(config.hostname),
            priv_key,
            pub_key,
        }
    }
}

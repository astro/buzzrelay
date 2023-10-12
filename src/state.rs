use axum::{
    extract::FromRef,
};
use sigh::{PrivateKey, PublicKey};
use std::sync::Arc;
use crate::{config::Config, db::Database};

#[derive(Clone)]
pub struct State {
    pub database: Database,
    pub client: Arc<reqwest::Client>,
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
    pub fn new(config: Config, database: Database, client: reqwest::Client) -> Self {
        let priv_key = Arc::new(config.priv_key());
        let pub_key = Arc::new(config.pub_key());
        State {
            database,
            client: Arc::new(client),
            hostname: Arc::new(config.hostname),
            priv_key,
            pub_key,
        }
    }
}

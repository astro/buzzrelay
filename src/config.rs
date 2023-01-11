use serde::Deserialize;
use sigh::{PrivateKey, PublicKey, Key};

#[derive(Deserialize)]
pub struct Config {
    pub streams: Vec<String>,
    pub db: String,
    pub hostname: String,
    pub listen_port: u16,
    priv_key_file: String,
    pub_key_file: String,
}

impl Config {
    pub fn load(config_file: &str) -> Config {
        let data = std::fs::read_to_string(config_file)
            .expect("read config");
        serde_yaml::from_str(&data)
            .expect("parse config")
    }

    pub fn priv_key(&self) -> PrivateKey {
        let data = std::fs::read_to_string(&self.priv_key_file)
            .expect("read priv_key_file");
        PrivateKey::from_pem(data.as_bytes())
            .expect("priv_key")
    }

    pub fn pub_key(&self) -> PublicKey {
        let data = std::fs::read_to_string(&self.pub_key_file)
            .expect("read pub_key_file");
        PublicKey::from_pem(data.as_bytes())
            .expect("pub_key")
    }
}

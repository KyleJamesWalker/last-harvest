//! `GET /ice`: the ICE servers a peer passes to `IceConfig`, with TURN credentials in coturn's `use-auth-secret`
//! scheme: the username is `<expiry>:<name>` and the password is base64(HMAC-SHA1(secret, username)).

use crate::config::Config;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

/// TURN credentials stay valid this long after they are minted.
pub const TURN_CREDENTIAL_SECONDS: u64 = 6 * 60 * 60;

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn turn_credentials(secret: &str, now_unix: u64, name: &str) -> (String, String) {
    let username = format!("{}:{}", now_unix + TURN_CREDENTIAL_SECONDS, name);
    let mut mac = Hmac::<sha1::Sha1>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key length");
    mac.update(username.as_bytes());
    let password = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    (username, password)
}

/// `{"iceServers": [...]}`, as in `RTCConfiguration`: empty without a TURN URL and secret.
pub fn ice_servers(config: &Config, now_unix: u64) -> String {
    let servers = match (&config.turn_url, &config.turn_secret) {
        (Some(url), Some(secret)) => {
            let mut name = [0u8; 4];
            getrandom::fill(&mut name).expect("the OS random source");
            let name: String = name.iter().map(|b| format!("{b:02x}")).collect();
            let (username, credential) = turn_credentials(secret, now_unix, &format!("lh-{name}"));
            vec![json!({ "urls": [url], "username": username, "credential": credential })]
        }
        _ => Vec::new(),
    };
    json!({ "iceServers": servers }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_credentials_match_coturn_rest_scheme() {
        let (username, password) = turn_credentials("0123abcd", 1_700_000_000 - TURN_CREDENTIAL_SECONDS, "smw");
        assert_eq!(username, "1700000000:smw");
        assert_eq!(password, "raLvRNXAbHqTzoE05oM0nB4fPbg=");
    }

    #[test]
    fn ice_servers_need_a_url_and_a_secret() {
        let mut config = Config::embedded(0);
        assert_eq!(ice_servers(&config, 0), r#"{"iceServers":[]}"#);
        config.turn_url = Some("turn:relay.example.com:3478".into());
        assert_eq!(ice_servers(&config, 0), r#"{"iceServers":[]}"#);
        config.turn_secret = Some("00ff".into());
        let servers: serde_json::Value = serde_json::from_str(&ice_servers(&config, 100)).unwrap();
        let server = &servers["iceServers"][0];
        assert_eq!(server["urls"][0], "turn:relay.example.com:3478");
        let username = server["username"].as_str().unwrap();
        assert!(username.starts_with(&format!("{}:lh-", 100 + TURN_CREDENTIAL_SECONDS)), "{username}");
        assert_eq!(server["credential"].as_str().unwrap(), turn_credentials("00ff", 100, &username[username.find(':').unwrap() + 1..]).1);
    }
}

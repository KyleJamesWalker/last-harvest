//! TURN credentials in coturn's `use-auth-secret` scheme: the username is `<expiry>:<name>` and the password is
//! base64(HMAC-SHA1(secret, username)). The relay hands them only to signaling peers, as an `IceServers` message.

use crate::config::Config;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn hmac(secret: &str, text: &str) -> Vec<u8> {
    let mut mac = Hmac::<sha1::Sha1>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key length");
    mac.update(text.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// A credential for `name` that expires `lifetime_seconds` after `now_unix`.
pub fn turn_credentials(secret: &str, now_unix: u64, lifetime_seconds: u64, name: &str) -> (String, String) {
    let username = format!("{}:{}", now_unix + lifetime_seconds, name);
    let password = base64::engine::general_purpose::STANDARD.encode(hmac(secret, &username));
    (username, password)
}

/// The TURN name of every peer from `client`'s address: coturn's `user-quota` counts allocations per name.
pub fn turn_name(secret: &str, client: IpAddr) -> String {
    let digest = hmac(secret, &format!("quota {}", crate::limits::key(client)));
    format!("lh-{}", digest[..8].iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// `{"iceServers": [...]}`, as in `RTCConfiguration`, for a peer from `client`; None without a TURN URL and secret.
pub fn ice_servers(config: &Config, now_unix: u64, client: IpAddr) -> Option<Value> {
    let (Some(url), Some(secret)) = (&config.turn_url, &config.turn_secret) else { return None };
    let (username, credential) = turn_credentials(secret, now_unix, config.turn_lifetime.as_secs(), &turn_name(secret, client));
    Some(json!({ "iceServers": [{ "urls": [url], "username": username, "credential": credential }] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_credentials_match_coturn_rest_scheme() {
        let (username, password) = turn_credentials("0123abcd", 1_700_000_000 - 600, 600, "smw");
        assert_eq!(username, "1700000000:smw");
        assert_eq!(password, "raLvRNXAbHqTzoE05oM0nB4fPbg=");
    }

    #[test]
    fn ice_servers_need_a_url_and_a_secret() {
        let client: IpAddr = "203.0.113.7".parse().unwrap();
        let mut config = Config::embedded(0);
        assert_eq!(ice_servers(&config, 0, client), None);
        config.turn_url = Some("turn:relay.example.com:3478".into());
        assert_eq!(ice_servers(&config, 0, client), None);
        config.turn_secret = Some("00ff".into());
        let servers = ice_servers(&config, 100, client).unwrap();
        let server = &servers["iceServers"][0];
        assert_eq!(server["urls"][0], "turn:relay.example.com:3478");
        let username = server["username"].as_str().unwrap();
        let lifetime = config.turn_lifetime.as_secs();
        assert_eq!(username, format!("{}:{}", 100 + lifetime, turn_name("00ff", client)));
        assert_eq!(server["credential"].as_str().unwrap(), turn_credentials("00ff", 100, lifetime, &turn_name("00ff", client)).1);
    }

    #[test]
    fn one_address_has_one_turn_name() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        let name = turn_name("00ff", ip("2001:db8:5:1::1"));
        assert!(name.starts_with("lh-") && name.len() == 19, "{name}");
        assert_eq!(name, turn_name("00ff", ip("2001:db8:5:2::2")), "the same /56");
        assert_ne!(name, turn_name("00ff", ip("2001:db8:5:100::1")));
        assert_ne!(name, turn_name("11ee", ip("2001:db8:5:1::1")), "the secret keys the name");
        assert_ne!(turn_name("00ff", ip("203.0.113.1")), turn_name("00ff", ip("203.0.113.2")));
    }
}

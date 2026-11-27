//! ICE servers as `{"iceServers": [...]}`: what an HTTP(S) endpoint such as the relay's `/ice` answers, and what the
//! relay sends a signaling peer, with TURN credentials, as an `IceServers` message.

use crate::{IceConfig, IceServer};

/// Reads the relay's answer; a server without URLs is left out.
pub fn parse_ice(json: &str) -> Result<IceConfig, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("bad ICE answer: {e}"))?;
    parse_ice_value(&v)
}

pub(crate) fn parse_ice_value(v: &serde_json::Value) -> Result<IceConfig, String> {
    let list = v
        .get("iceServers")
        .and_then(|s| s.as_array())
        .ok_or("no iceServers in the answer")?;
    let text = |s: &serde_json::Value, k: &str| {
        s.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let servers = list
        .iter()
        .map(|s| {
            let urls = match s.get("urls") {
                Some(serde_json::Value::String(u)) => vec![u.clone()],
                Some(serde_json::Value::Array(a)) => a
                    .iter()
                    .filter_map(|u| u.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            };
            IceServer {
                urls,
                username: text(s, "username"),
                credential: text(s, "credential"),
            }
        })
        .filter(|s| !s.urls.is_empty())
        .collect();
    Ok(IceConfig {
        servers,
        relay_only: false,
    })
}

/// `base` with the servers the relay sent over signaling, which replace any it sent before.
pub(crate) fn with_relay_servers(base: &IceConfig, relay: &[IceServer]) -> IceConfig {
    IceConfig {
        servers: base.servers.iter().chain(relay).cloned().collect(),
        relay_only: base.relay_only,
    }
}

/// A fetch of the relay's ICE servers in flight; poll it once a frame.
pub struct IceFetch {
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<Result<IceConfig, String>>,
    #[cfg(target_arch = "wasm32")]
    slot: std::rc::Rc<std::cell::RefCell<Option<Result<IceConfig, String>>>>,
}

impl IceFetch {
    /// Starts fetching `url`, an `http://` or `https://` address.
    pub fn start(url: &str) -> IceFetch {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let url = url.to_string();
            std::thread::spawn(move || {
                drop(tx.send(native::get(&url).and_then(|body| parse_ice(&body))))
            });
            IceFetch { rx }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
            web::get(url, slot.clone());
            IceFetch { slot }
        }
    }

    /// The servers once the answer is in, or why there are none.
    pub fn poll(&mut self) -> Option<Result<IceConfig, String>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self.rx.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("the fetch stopped".into()))
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.slot.borrow_mut().take()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Arc;
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(10);
    const MOST: u64 = 64 * 1024;

    /// The body of a plain GET, which must answer 200.
    pub fn get(url: &str) -> Result<String, String> {
        let (secure, rest) = match url.split_once("://") {
            Some(("http", rest)) => (false, rest),
            Some(("https", rest)) => (true, rest),
            _ => return Err(format!("{url} is not an http or https address")),
        };
        let (authority, path) = rest
            .split_once('/')
            .map_or((rest, "/".to_string()), |(a, p)| (a, format!("/{p}")));
        let host = authority
            .rsplit_once(':')
            .filter(|(_, p)| p.parse::<u16>().is_ok())
            .map_or(authority, |(h, _)| h);
        let addr = if authority.contains(':') {
            authority.to_string()
        } else {
            format!("{authority}:{}", if secure { 443 } else { 80 })
        };
        let tcp = TcpStream::connect(&addr).map_err(|e| format!("{addr}: {e}"))?;
        tcp.set_read_timeout(Some(TIMEOUT)).ok();
        tcp.set_write_timeout(Some(TIMEOUT)).ok();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {authority}\r\nUser-Agent: last-harvest\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        );
        let mut raw = Vec::new();
        if secure {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let name = rustls::pki_types::ServerName::try_from(host.to_string())
                .map_err(|e| format!("{host}: {e}"))?;
            let conn =
                rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
            let mut tls = rustls::StreamOwned::new(conn, tcp);
            tls.write_all(request.as_bytes())
                .map_err(|e| e.to_string())?;
            read_all(&mut tls, &mut raw)?;
        } else {
            let mut tcp = tcp;
            tcp.write_all(request.as_bytes())
                .map_err(|e| e.to_string())?;
            read_all(&mut tcp, &mut raw)?;
        }
        body(&raw)
    }

    fn read_all(r: &mut impl Read, out: &mut Vec<u8>) -> Result<(), String> {
        match r.take(MOST).read_to_end(out) {
            Ok(_) => Ok(()),
            Err(_) if !out.is_empty() => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn split_crlf(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
        let at = bytes.windows(2).position(|w| w == b"\r\n")?;
        Some((&bytes[..at], &bytes[at + 2..]))
    }

    fn body(raw: &[u8]) -> Result<String, String> {
        let end = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("no HTTP answer")?;
        let (head, body) = (String::from_utf8_lossy(&raw[..end]), &raw[end + 4..]);
        let status = head.lines().next().unwrap_or("");
        if status.split_whitespace().nth(1) != Some("200") {
            return Err(format!("the relay answered {status}"));
        }
        if head
            .lines()
            .any(|l| l.to_ascii_lowercase().replace(' ', "") == "transfer-encoding:chunked")
        {
            let mut out = Vec::new();
            let mut rest = body;
            while let Some((size, after)) = split_crlf(rest) {
                let n = std::str::from_utf8(size)
                    .ok()
                    .and_then(|s| usize::from_str_radix(s.trim(), 16).ok())
                    .ok_or("bad chunk")?;
                if n == 0 || after.len() < n {
                    break;
                }
                out.extend_from_slice(&after[..n]);
                rest = after[n..].strip_prefix(b"\r\n").unwrap_or(&after[n..]);
            }
            return Ok(String::from_utf8_lossy(&out).into_owned());
        }
        Ok(String::from_utf8_lossy(body).into_owned())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn a_plain_and_a_chunked_answer_read_the_same() {
            assert_eq!(
                super::body(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").unwrap(),
                "{}"
            );
            assert_eq!(
                super::body(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n"
                )
                .unwrap(),
                "{}"
            );
            assert!(super::body(b"HTTP/1.1 403 Forbidden\r\n\r\n").is_err());
        }

        #[test]
        fn a_chunk_that_splits_a_character_reads_whole() {
            let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n\"\xc3\r\n2\r\n\xa9\"\r\n0\r\n\r\n";
            assert_eq!(super::body(raw).unwrap(), "\"\u{e9}\"");
            assert_eq!(
                super::body(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n9\r\n\xc3")
                    .unwrap(),
                ""
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::rc::Rc;

    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    use crate::IceConfig;

    pub fn get(url: &str, slot: Rc<RefCell<Option<Result<IceConfig, String>>>>) {
        let Some(window) = web_sys::window() else {
            *slot.borrow_mut() = Some(Err("no window".into()));
            return;
        };
        let promise = window.fetch_with_str(url);
        wasm_bindgen_futures::spawn_local(async move {
            let got = async {
                let resp: web_sys::Response = JsFuture::from(promise)
                    .await
                    .map_err(|e| format!("{e:?}"))?
                    .dyn_into()
                    .map_err(|_| "not a response".to_string())?;
                if !resp.ok() {
                    return Err(format!("the relay answered {}", resp.status()));
                }
                let text = JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                super::parse_ice(&text.as_string().unwrap_or_default())
            };
            *slot.borrow_mut() = Some(got.await);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relays_answer_reads_as_ice_servers() {
        let ice = parse_ice(r#"{"iceServers":[{"urls":["turn:relay.example.com:3478"],"username":"1:lh","credential":"x"},{"urls":"stun:s.example.com"},{"urls":[]}]}"#).unwrap();
        assert_eq!(ice.servers.len(), 2);
        assert_eq!(ice.servers[0].urls, ["turn:relay.example.com:3478"]);
        assert_eq!(ice.servers[1].username, "");
        assert_eq!(
            parse_ice(r#"{"iceServers":[]}"#).unwrap(),
            IceConfig::default()
        );
        assert!(parse_ice("nope").is_err());
    }

    #[test]
    fn relay_servers_join_the_games_own() {
        let stun = IceServer {
            urls: vec!["stun:s.example.com".into()],
            ..Default::default()
        };
        let turn = IceServer {
            urls: vec!["turn:t.example.com".into()],
            username: "1:lh-ab".into(),
            credential: "x".into(),
        };
        let base = IceConfig {
            servers: vec![stun.clone()],
            relay_only: true,
        };
        assert_eq!(
            with_relay_servers(&base, std::slice::from_ref(&turn)),
            IceConfig {
                servers: vec![stun, turn],
                relay_only: true
            }
        );
    }
}

//! The door a local agent reads and redraws the open sprite through, on 127.0.0.1 at `SPRITE_EDITOR_REMOTE`'s
//! port. Requests wait here for the window's next frame, which answers them on the main thread.

use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use tiny_http::{Header, Response, Server};

pub type Reply = (u16, &'static str, Vec<u8>);

const MAX_BODY: u64 = 4 << 20;

pub struct Call {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
    pub reply: Sender<Reply>,
}

fn refuse(why: &str) -> Reply {
    (403, "text/plain; charset=utf-8", why.as_bytes().to_vec())
}

/// Whether a request came from a local program rather than a web page: browsers send Origin on writes, and a
/// rebound DNS name shows in Host.
fn local(req: &tiny_http::Request, port: u16) -> bool {
    let header = |name: &'static str| req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str().to_string());
    let host_ok = header("Host").is_none_or(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    host_ok && header("Origin").is_none()
}

pub fn listen(port: u16) -> Result<Receiver<Call>, String> {
    let server = Server::http(("127.0.0.1", port)).map_err(|e| format!("remote port {port}: {e}"))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let (status, kind, bytes) = if !local(&req, port) {
                refuse("only local programs may drive the editor")
            } else {
                let mut body = Vec::new();
                let _ = req.as_reader().take(MAX_BODY).read_to_end(&mut body);
                let (reply, wait) = mpsc::channel();
                let call = Call { method: req.method().as_str().to_string(), path: req.url().to_string(), body, reply };
                if tx.send(call).is_err() {
                    break;
                }
                wait.recv_timeout(Duration::from_secs(10)).unwrap_or((503, "text/plain; charset=utf-8", b"the editor didn't answer".to_vec()))
            };
            let kind = Header::from_bytes("Content-Type", kind).expect("a static content type");
            let _ = req.respond(Response::from_data(bytes).with_status_code(status).with_header(kind));
        }
    });
    Ok(rx)
}

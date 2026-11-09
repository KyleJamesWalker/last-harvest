//! The page's end of the remote door. Opened with `?remote`, the page asks the server that served it
//! (`tools/editor_relay.py`) for an agent's next request, answers it as the native window does, and asks again.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Blob, HtmlCanvasElement, XmlHttpRequest, XmlHttpRequestResponseType};

use super::Web;

type Reply = (u16, &'static str, Vec<u8>);

fn text(status: u16, body: impl Into<String>) -> Reply {
    (status, "text/plain; charset=utf-8", body.into().into_bytes())
}

pub fn wanted(search: &str) -> bool {
    search.trim_start_matches('?').split('&').any(|kv| kv == "remote")
}

fn later(ms: i32, f: impl FnOnce() + 'static) {
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(Closure::once_into_js(f).unchecked_ref(), ms);
    }
}

/// Asks for the next request, and keeps asking while the relay answers.
pub fn poll(web: Rc<RefCell<Web>>) {
    let Ok(x) = XmlHttpRequest::new() else { return };
    if x.open("GET", "remote/next").is_err() {
        return;
    }
    x.set_response_type(XmlHttpRequestResponseType::Arraybuffer);
    let xhr = x.clone();
    let done = Closure::once_into_js(move || match xhr.status().unwrap_or(0) {
        200 => {
            answer(&web, &xhr);
            poll(web);
        }
        204 => poll(web),
        0 | 500.. => later(2000, move || poll(web)),
        status => web_sys::console::warn_1(&format!("remote: the server that served this page has no remote door ({status})").into()),
    });
    let _ = x.add_event_listener_with_callback("loadend", done.unchecked_ref());
    let _ = x.send();
}

fn answer(web: &Rc<RefCell<Web>>, x: &XmlHttpRequest) {
    let header = |name: &str| x.get_response_header(name).ok().flatten().unwrap_or_default();
    let (id, method, path) = (header("X-Call"), header("X-Method"), header("X-Path"));
    let body = x.response().map(|b| js_sys::Uint8Array::new(&b).to_vec()).unwrap_or_default();
    let Ok(mut w) = web.try_borrow_mut() else { return reply(&id, text(503, "the editor is busy; try again")) };
    if method == "GET" && path == "/shot.png" {
        w.shots.push(id);
    } else {
        let r = w.remote(&method, &path, &body);
        reply(&id, r);
    }
}

fn send(id: &str, status: u16, kind: &str, body: impl FnOnce(&XmlHttpRequest) -> Result<(), JsValue>) {
    let Ok(x) = XmlHttpRequest::new() else { return };
    let sent = x
        .open("POST", &format!("remote/reply/{id}"))
        .and_then(|()| x.set_request_header("X-Status", &status.to_string()))
        .and_then(|()| x.set_request_header("Content-Type", kind))
        .and_then(|()| body(&x));
    if let Err(e) = sent {
        web_sys::console::warn_1(&e);
    }
}

fn reply(id: &str, (status, kind, bytes): Reply) {
    send(id, status, kind, |x| x.send_with_opt_u8_array(Some(&bytes)));
}

/// Answers a request for a picture of the panes; called just after they're drawn, while the canvas still holds them.
pub fn shoot(canvas: &HtmlCanvasElement, id: String) {
    let done = Closure::once_into_js(move |blob: JsValue| match blob.dyn_into::<Blob>() {
        Ok(blob) => send(&id, 200, "image/png", |x| x.send_with_opt_blob(Some(&blob))),
        Err(_) => reply(&id, text(500, "the canvas gave no picture")),
    });
    let _ = canvas.to_blob(done.unchecked_ref());
}

impl Web {
    fn remote(&mut self, method: &str, path: &str, body: &[u8]) -> Reply {
        let bare = path.split(['?', '#']).next().unwrap_or("");
        match (method, bare.strip_prefix("/act/")) {
            ("POST", Some(act)) => {
                let on_show = self.document.query_selector(&format!("[data-act=\"{act}\"]")).ok().flatten().is_some_and(|b| b.closest("[hidden]").ok().flatten().is_none());
                if !act.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') || !on_show {
                    return text(404, "no such toolbar button on show; GET /acts lists them");
                }
                self.act(act);
                self.studio.editor.remote("GET", "/state", &[])
            }
            ("GET", _) if bare == "/acts" => {
                let buttons = self.document.query_selector_all("[data-act]").ok();
                let acts = (0..buttons.as_ref().map_or(0, |b| b.length())).filter_map(|i| buttons.as_ref()?.item(i)?.dyn_into::<web_sys::Element>().ok());
                let lines: String = acts.filter(|b| b.closest("[hidden]").ok().flatten().is_none()).filter_map(|b| Some(format!("{}\t{}\n", b.get_attribute("data-act")?, b.text_content().unwrap_or_default()))).collect();
                text(200, lines)
            }
            ("POST", _) if bare == "/view" => {
                let arg = |k: &str| path.split_once('?').into_iter().flat_map(|(_, q)| q.split('&')).find_map(|p| p.strip_prefix(k)?.strip_prefix('=')?.parse::<f32>().ok()).unwrap_or(0.0);
                self.studio.turn_view(arg("yaw"), arg("closer"));
                text(200, "turned")
            }
            _ => self.studio.editor.remote(method, path, body),
        }
    }
}

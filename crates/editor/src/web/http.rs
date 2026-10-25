//! The sprite store of the `sprite-server` that served the page.

use sprite_editor::history::Commit;
use sprite_editor::sprite::{Sprite, bundle};
use sprite_editor::store::Store;
use web_sys::XmlHttpRequest;

pub struct HttpStore;

/// Sends a request and waits for the answer: the editor reads and saves in step with its edits, and on a local
/// network that takes milliseconds.
fn request(method: &str, url: &str, body: Option<&[u8]>) -> Result<(u16, Vec<u8>), String> {
    let x = XmlHttpRequest::new().map_err(|e| format!("{e:?}"))?;
    x.open_with_async(method, url, false).map_err(|e| format!("{e:?}"))?;
    // A synchronous request can't ask for bytes, so it takes each byte as one character.
    x.override_mime_type("text/plain; charset=x-user-defined").map_err(|e| format!("{e:?}"))?;
    x.send_with_opt_u8_array(body).map_err(|_| "the sprite server isn't answering".to_string())?;
    let text = x.response_text().map_err(|e| format!("{e:?}"))?.unwrap_or_default();
    Ok((x.status().unwrap_or(0), text.chars().map(|c| c as u32 as u8).collect()))
}

fn get(url: &str) -> Option<Vec<u8>> {
    request("GET", url, None).ok().filter(|(status, _)| *status == 200).map(|(_, b)| b)
}

fn put(url: &str, body: &[u8]) -> Result<(), String> {
    match request("PUT", url, Some(body))? {
        (200, _) => Ok(()),
        (status, b) => Err(format!("{status} {}", String::from_utf8_lossy(&b))),
    }
}

fn utf8(b: Vec<u8>) -> String {
    String::from_utf8_lossy(&b).into_owned()
}

impl Store for HttpStore {
    fn names(&self) -> Vec<(String, bool)> {
        let text = get("api/names").map(utf8).unwrap_or_default();
        text.lines().filter_map(|l| l.split_once('\t')).map(|(n, saved)| (n.to_string(), saved == "1")).collect()
    }

    fn read(&self, name: &str, file: &str) -> Option<Vec<u8>> {
        get(&format!("api/file/{name}/{file}"))
    }

    fn save(&self, sprite: &Sprite) -> Result<(), String> {
        put(&format!("api/sprite/{}", sprite.name), &bundle(&sprite.files()))
    }

    fn stand(&self, name: &str) -> Option<String> {
        get(&format!("api/stand/{name}")).map(utf8)
    }

    fn save_stand(&self, name: &str, how: &str) -> Result<(), String> {
        put(&format!("api/stand/{name}"), how.as_bytes())
    }

    fn history(&self, name: &str) -> Option<(Vec<Commit>, bool)> {
        let text = utf8(get(&format!("api/history/{name}"))?);
        let (changed, log) = text.split_once('\n').unwrap_or((&text, ""));
        Some((Commit::parse(log), changed == "1"))
    }

    fn version(&self, id: &str, name: &str, file: &str) -> Option<Vec<u8>> {
        get(&format!("api/version/{id}/{name}/{file}"))
    }
}

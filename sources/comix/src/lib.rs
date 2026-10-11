// Ported from keiyoushi src/en/comix (Apache-2.0). Modified for the Kurobayomi plug-in contract; see README.md.
//! Comix (comix.to) as a manga source plug-in, ABI 1.4 (`wit/kuro-source.wit`). The site is reached only through the
//! host's `http` import, so the manifest's `hosts` list is the whole network reach of this plug-in.
//!
//! Ops: `info`, `create-sources`, `popular`, `latest`, `search`, `details`, `chapters`, `pages`, and `unscramble`, which
//! is an addition the app must call for page images (see `docs/comix-image-hook.md`). Every other op is "unknown op".

mod api;
mod descramble;
mod keys;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::Params;
use crate::keys::Keys;

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use kuro::source::host;

const NAME: &str = "Comix";
const LANG: &str = "en";
const BASE_URL: &str = "https://comix.to";
/// Kotlin `MAX_CHAPTER_PAGES`: the chapter list stops after this many pages of 100.
const MAX_CHAPTER_PAGES: u32 = 200;

/// The id the Rust source had (`kuro_source::generate_id`): the first 8 bytes of MD5 of `name/lang/version`, lower-cased
/// name, masked to a positive i64. Library entries refer to it, so it must not change.
fn generate_id(name: &str, lang: &str, version: i32) -> i64 {
    let digest = md5::compute(format!("{}/{}/{}", name.to_lowercase(), lang, version).as_bytes());
    digest.0.iter().take(8).fold(0_i64, |acc, &b| (acc << 8) | i64::from(b)) & i64::MAX
}

fn descriptor() -> serde_json::Value {
    json!({ "id": generate_id(NAME, LANG, 1), "name": NAME, "lang": LANG, "supportsLatest": true })
}

#[derive(Deserialize)]
struct Envelope<A> {
    args: A,
}

#[derive(Deserialize)]
struct PageArgs {
    page: i32,
}

#[derive(Deserialize)]
struct SearchArgs {
    page: i32,
    query: String,
}

#[derive(Deserialize)]
struct Named {
    url: String,
}

#[derive(Deserialize)]
struct MangaArgs {
    manga: Named,
}

#[derive(Deserialize)]
struct ChapterArgs {
    chapter: Named,
}

#[derive(Deserialize)]
struct UnscrambleArgs {
    /// The image bytes, base64.
    body: String,
    /// The response headers of the image request, as `[name, value]` pairs.
    headers: Vec<(String, String)>,
}

/// The `args` of the `{"sourceId", "args"}` envelope.
fn args<A: for<'de> Deserialize<'de>>(input: &str) -> Result<A, String> {
    serde_json::from_str::<Envelope<A>>(input)
        .map(|e| e.args)
        .map_err(|e| format!("bad input: {e}"))
}

fn out<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

/// The host's `http` answer: `{"status", "headers", "body", "bodyEncoding"?}`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn get(url: &str) -> Result<Response, String> {
    let request = json!({ "method": "GET", "url": url, "headers": [["Accept", "*/*"]] }).to_string();
    let raw: HostResponse = serde_json::from_str(&host::http(&request)?).map_err(|e| format!("host response: {e}"))?;
    let body = match raw.body_encoding.as_deref() {
        Some("base64") => STANDARD.decode(&raw.body).map_err(|e| e.to_string())?,
        _ => raw.body.into_bytes(),
    };
    Ok(Response { status: raw.status, headers: raw.headers, body })
}

fn check_status(response: &Response, url: &str) -> Result<(), String> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else {
        Err(format!("HTTP {} from {url}", response.status))
    }
}

/// One signed API call. The answer is the decoded body and the keys that signed the call.
fn api_get(path: &str, params: &Params) -> Result<(Vec<u8>, Keys), String> {
    let keys = Keys::current()?;
    let url = api::signed_url(BASE_URL, path, params, &keys);
    let response = get(&url)?;
    if response.status == 403 {
        let body = String::from_utf8_lossy(&response.body);
        return Err(format!(
            "Comix refused the request signature (keys of site build {}): {}. The site keys have probably changed; recapture them with capture.js.",
            keys.build,
            body.trim()
        ));
    }
    check_status(&response, &url)?;
    let build = response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("x-build-id"))
        .map(|(_, value)| value.as_str());
    if let Some(build) = build.filter(|b| *b != keys.build) {
        host::log(2, &format!("comix: the site build {build} differs from the build of the request keys {}", keys.build));
    }
    Ok((response.body, keys))
}

fn browse(mut params: Params, page: i32) -> Result<String, String> {
    params.insert("page".into(), vec![page.to_string()]);
    params.entry("limit".into()).or_insert_with(|| vec!["28".into()]);
    let (body, keys) = api_get("/manga", &params)?;
    out(&api::manga_page(&body, &keys)?)
}

fn listing(order: &str, page: i32) -> Result<String, String> {
    let key = format!("order[{order}]");
    let mut params = api::params(&[(key.as_str(), "desc")]);
    api::preference_params(&mut params);
    browse(params, page)
}

fn details(manga_url: &str) -> Result<String, String> {
    let url = format!("{BASE_URL}/title{manga_url}");
    let response = get(&url)?;
    check_status(&response, &url)?;
    let html = String::from_utf8_lossy(&response.body);
    out(&api::details(&html, manga_url)?)
}

fn chapters(manga_url: &str) -> Result<String, String> {
    let id = api::manga_id(manga_url).ok_or_else(|| format!("no manga id in {manga_url}"))?;
    let slug = api::manga_slug(manga_url);
    let path = format!("/manga/{id}/chapters");
    let mut all = Vec::new();
    for page in 1..=MAX_CHAPTER_PAGES {
        let page_text = page.to_string();
        let params = api::params(&[("limit", "100"), ("order[number]", "desc"), ("page", page_text.as_str())]);
        let (body, keys) = api_get(&path, &params)?;
        let (found, has_next) = api::chapter_page(&body, &keys, slug)?;
        let empty = found.is_empty();
        all.extend(found);
        if !has_next || empty {
            break;
        }
    }
    out(&api::select_chapters(all))
}

fn pages(chapter_url: &str) -> Result<String, String> {
    let id = api::chapter_id(chapter_url).ok_or_else(|| format!("no chapter id in {chapter_url}"))?;
    let (body, keys) = api_get(&format!("/chapters/{id}"), &Params::new())?;
    let pages = api::pages(&body, &keys)?;
    if pages.is_empty() {
        return Err("Comix gave no pages for this chapter".into());
    }
    out(&pages)
}

/// The image hook: the bytes to show for a page image, or `null` when the bytes need no change.
fn unscramble(input: &str) -> Result<String, String> {
    let a: UnscrambleArgs = args(input)?;
    let body = STANDARD.decode(&a.body).map_err(|e| format!("body is not base64: {e}"))?;
    let fixed = descramble::unscramble(&body, &a.headers)?;
    out(&json!({ "body": fixed.map(|bytes| STANDARD.encode(bytes)) }))
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        match op.as_str() {
            "info" => Ok(descriptor().to_string()),
            "create-sources" => Ok(json!([descriptor()]).to_string()),
            "popular" => {
                let a: PageArgs = args(&input_json)?;
                listing("score", a.page)
            }
            "latest" => {
                let a: PageArgs = args(&input_json)?;
                listing("chapter_updated_at", a.page)
            }
            "search" => {
                let a: SearchArgs = args(&input_json)?;
                browse(api::search_params(&a.query), a.page)
            }
            "details" => {
                let a: MangaArgs = args(&input_json)?;
                details(&a.manga.url)
            }
            "chapters" => {
                let a: MangaArgs = args(&input_json)?;
                chapters(&a.manga.url)
            }
            "pages" => {
                let a: ChapterArgs = args(&input_json)?;
                pages(&a.chapter.url)
            }
            "unscramble" => unscramble(&input_json),
            _ => Err(format!("unknown op: {op}")),
        }
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    /// The vectors of `kuro_source::generate_id` in the app (`crates/kuro-source/src/id.rs`).
    #[test]
    fn the_id_matches_the_app_function() {
        assert_eq!(generate_id("Example", "en", 1), 4_851_548_778_462_321_757);
        assert_eq!(generate_id("Ünïcode Src", "ja", 2), 8_842_957_433_569_127_029);
    }

    /// The wire form of the image hook: an image without scrambling headers comes back as `null`, and a grid image that
    /// is not an image is an error.
    #[test]
    fn the_unscramble_op_answers_null_for_a_plain_image() {
        let input = json!({"sourceId": 1, "args": {"body": "AAEC", "headers": []}}).to_string();
        assert_eq!(unscramble(&input), Ok(r#"{"body":null}"#.to_owned()));
        let grid = json!({"sourceId": 1, "args": {"body": "AAEC", "headers": [["x-scramble-grid", "5x5"], ["x-scramble-seed", "9"]]}}).to_string();
        assert_eq!(unscramble(&grid), Err("Failed to decode image".to_owned()));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(
            <Plugin as Guest>::invoke("hosters".into(), "{}".into()),
            Err("unknown op: hosters".to_owned())
        );
    }
}

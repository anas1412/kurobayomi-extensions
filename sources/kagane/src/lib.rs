//! Kagane (<https://kagane.si>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`). Ported from the
//! keiyoushi `all/kagane` extension (Apache-2.0), English only. This file is the ABI glue: the ops, the one network path
//! (the `http` import) and the integrity token. The URLs and parsers are in `api.rs`.

mod api;

use serde::Deserialize;
use serde_json::{Value, json};

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use api::Res;
use kuro::source::host;

/// The source id: `generate_id("Kagane", "en", 1)`, the MD5 of `kagane/en/1`, as the Kotlin extension has it. Keep it:
/// backups and the library name it.
const ID: i64 = 4024736764982024684;
const NAME: &str = "Kagane";
const LANG: &str = "en";
/// The Kotlin keeps to 3 requests a second (`rateLimit(3)`).
const REQUEST_GAP_MS: u64 = 350;
/// Where the integrity token is kept between calls, with the time it runs out.
const INTEGRITY_PREF: &str = "integrity";
/// A token this close to its end is fetched again, so a page list never starts with one about to expire.
const INTEGRITY_MARGIN_S: u64 = 60;

/// The `args` of an op. Each op reads only the fields it needs.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Args {
    page: i32,
    query: String,
    manga: Option<Ref>,
    chapter: Option<Ref>,
}

#[derive(Deserialize)]
struct Ref {
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    #[serde(default)]
    args: Args,
    /// The app's "Show adult (18+) titles" switch. An app that does not send it hides them.
    #[serde(default)]
    show_adult: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse {
    status: u16,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

fn descriptor() -> Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": true, "requestGapMs": REQUEST_GAP_MS })
}

/// Sends a request to the site through the host, which refuses any host outside `hosts` in `manifest.toml`. A POST
/// carries a JSON body. A non-2xx answer is an error: the API's own message when it gives one.
fn send(method: &str, url: &str, body: Option<&str>, extra: &[(&str, &str)]) -> Res<String> {
    let mut headers = vec![
        ("Accept".to_owned(), "application/json".to_owned()),
        ("Referer".to_owned(), format!("{}/", api::BASE)),
        ("Origin".to_owned(), api::BASE.to_owned()),
    ];
    if body.is_some() {
        headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
    }
    headers.extend(extra.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())));
    let request = json!({ "method": method, "url": url, "headers": headers, "body": body });
    let response: HttpResponse = serde_json::from_str(&host::http(&request.to_string())?).map_err(|e| format!("invalid http reply: {e}"))?;
    if response.body_encoding.is_some() {
        return Err(format!("{url} sent a binary body"));
    }
    match response.status {
        200..=299 => Ok(response.body),
        429 => Err("Kagane is limiting requests; wait a few seconds and try again".into()),
        status => Err(api::error_message(&response.body).unwrap_or_else(|| format!("HTTP {status} from Kagane"))),
    }
}

fn to_value<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// The list sorted by `sort` (empty: relevance), filtered by `query`; adult titles only when `show_adult`.
fn list(page: i32, query: &str, sort: &str, show_adult: bool) -> Res<Value> {
    to_value(api::parse_search(&send("POST", &api::search_url(page, sort), Some(&api::search_body(query, show_adult)), &[])?)?)
}

/// The integrity token the page challenge needs: the kept one while it has time left, else a new one, kept for the next
/// chapter.
fn integrity() -> Res<String> {
    let now = host::unix_seconds();
    if let Ok(kept) = serde_json::from_str::<api::Integrity>(&host::pref_get(INTEGRITY_PREF))
        && kept.exp > now + INTEGRITY_MARGIN_S
    {
        return Ok(kept.token);
    }
    let fresh = api::parse_integrity(&send("POST", api::INTEGRITY_URL, Some(""), &[])?)?;
    if let Err(error) = host::pref_set(INTEGRITY_PREF, &serde_json::to_string(&fresh).map_err(|e| e.to_string())?) {
        host::log(2, &format!("the Kagane integrity token could not be kept: {error}"));
    }
    Ok(fresh.token)
}

/// The page images of a chapter: the challenge with the integrity token names the cache host, the access token and the
/// pages.
fn pages(chapter_url: &str) -> Res<Value> {
    let book = api::book_of(chapter_url)?;
    let token = integrity()?;
    let images = api::parse_pages(book, &send("POST", &api::challenge_url(book), Some("{}"), &[("x-integrity-token", &token)])?)?;
    to_value(images.into_iter().map(|image| json!({ "url": "", "imageUrl": image })).collect::<Vec<_>>())
}

fn run(op: &str, args: Args, show_adult: bool) -> Res<Value> {
    match op {
        // the Kotlin's Popular is "Popular (Total Views)", its Latest "Latest" (`updated_at`)
        "popular" => list(args.page, "", "total_views", show_adult),
        "latest" => list(args.page, "", "updated_at", show_adult),
        "search" => list(args.page, &args.query, "", show_adult),
        "details" => {
            let manga = args.manga.ok_or("details needs args.manga")?;
            to_value(api::parse_details(&manga.url, &send("GET", &api::details_url(&manga.url), None, &[])?)?)
        }
        "chapters" => {
            let manga = args.manga.ok_or("chapters needs args.manga")?;
            to_value(api::parse_books(&manga.url, &send("GET", &api::books_url(&manga.url), None, &[])?)?)
        }
        "pages" => pages(&args.chapter.ok_or("pages needs args.chapter")?.url),
        _ => Err(format!("unknown op: {op}")),
    }
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        let out = match op.as_str() {
            "info" => descriptor(),
            "create-sources" => Value::Array(vec![descriptor()]),
            // ABI 1.5: the page-image headers. The cache host sends JPEG or JPEG XL; the app reads both.
            "request-headers" => json!([["Accept", "image/jxl,image/avif,image/webp,image/*,*/*"], ["Referer", format!("{}/", api::BASE)]]),
            "popular" | "latest" | "search" | "details" | "chapters" | "pages" => {
                let envelope: Envelope = serde_json::from_str(&input_json).map_err(|e| format!("invalid input: {e}"))?;
                run(&op, envelope.args, envelope.show_adult)?
            }
            _ => return Err(format!("unknown op: {op}")),
        };
        Ok(out.to_string())
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: Value = serde_json::from_str(&<Plugin as Guest>::invoke("info".into(), "{}".into()).unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(350)));
        assert_eq!(info["supportsLatest"], true);
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(<Plugin as Guest>::invoke("hosters".into(), "{}".into()), Err("unknown op: hosters".to_owned()));
    }
}

//! Weeb Central (<https://weebcentral.com>), a manga source plug-in for Kurobayomi, ABI 1.4 (`wit/kuro-source.wit`).
//! Port of `rust-sources/weebcentral` (keiyoushi `en/weebcentral`, Apache-2.0). This file is the ABI glue: the ops, the
//! wire shapes and the one network path, the `http` import. The URLs and parsers are in `site.rs`.

mod site;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use kuro::source::host;

/// The source id the app has always derived for this source: `generate_id("Weeb Central", "en", 1)`.
const ID: i64 = 2131019126180322627;
const NAME: &str = "Weeb Central";
const LANG: &str = "en";

// The wire shapes of the ABI 1.4 manga ops (`crates/kuro-source-wasm/src/wire.rs` in the app), camelCase on the wire.

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireManga {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub status: i32,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    /// The stored ordinal of the app's `UpdateStrategy`: 0 always updates, 1 fetches once.
    #[serde(default)]
    pub update_strategy: i64,
    #[serde(default)]
    pub initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireChapter {
    pub url: String,
    pub name: String,
    #[serde(default)]
    pub date_upload: i64,
    #[serde(default = "unknown_chapter_number")]
    pub chapter_number: f32,
    #[serde(default)]
    pub scanlator: Option<String>,
}

fn unknown_chapter_number() -> f32 {
    -1.0
}

/// A page as the guest lists it. The app numbers pages by their position, so no index is sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePage {
    pub url: String,
    #[serde(default)]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMangasPage {
    #[serde(default)]
    pub mangas: Vec<WireManga>,
    #[serde(default)]
    pub has_next_page: bool,
}

/// The `args` of an op. Each op reads only the fields it needs.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Args {
    page: i32,
    query: String,
    manga: Option<WireManga>,
    chapter: Option<WireChapter>,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    args: Args,
}

/// The reply of the `http` import (`bridge.rs` in the app).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse {
    status: u16,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

fn descriptor() -> Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": true })
}

/// Fetches a page of the site through the host. The host refuses any host outside `hosts` in `manifest.toml`, and
/// it follows redirects itself. The site's rate limit is the app's network layer; this guest cannot sleep.
fn get(url: &Url) -> Result<String, String> {
    let request = json!({
        "method": "GET",
        "url": url.as_str(),
        "headers": [["Referer", "https://weebcentral.com/"]],
    });
    let reply = host::http(&request.to_string())?;
    let response: HttpResponse = serde_json::from_str(&reply).map_err(|e| format!("invalid http reply: {e}"))?;
    if !(200..300).contains(&response.status) {
        return Err(format!("HTTP {} from {url}", response.status));
    }
    if response.body_encoding.is_some() {
        return Err(format!("{url} is not UTF-8 text"));
    }
    Ok(response.body)
}

fn to_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn listing(page: i32, query: &str, sort: &str) -> Result<Value, String> {
    let url = site::listing_url(page, query, sort)?;
    to_value(site::parse_listing(&get(&url)?, &url)?)
}

fn run(op: &str, args: Args) -> Result<Value, String> {
    match op {
        "popular" => listing(args.page, "", "Popularity"),
        "latest" => listing(args.page, "", "Latest Updates"),
        "search" => listing(args.page, &args.query, "Best Match"),
        "details" => {
            let manga = args.manga.ok_or("details needs args.manga")?;
            let url = site::page(&manga.url)?;
            to_value(site::parse_details(&get(&url)?, &url)?)
        }
        "chapters" => {
            let manga = args.manga.ok_or("chapters needs args.manga")?;
            let url = site::chapters_url(&manga.url)?;
            to_value(site::parse_chapters(&get(&url)?, &url)?)
        }
        "pages" => {
            let chapter = args.chapter.ok_or("pages needs args.chapter")?;
            let url = site::pages_url(&chapter.url)?;
            let pages: Vec<WirePage> = site::parse_pages(&get(&url)?, &url)
                .into_iter()
                .map(|image| WirePage { url: String::new(), image_url: Some(image) })
                .collect();
            to_value(pages)
        }
        _ => Err(format!("unknown op: {op}")),
    }
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        let out = match op.as_str() {
            "info" => descriptor(),
            "create-sources" => Value::Array(vec![descriptor()]),
            "popular" | "latest" | "search" | "details" | "chapters" | "pages" => {
                let envelope: Envelope =
                    serde_json::from_str(&input_json).map_err(|e| format!("invalid input: {e}"))?;
                run(&op, envelope.args)?
            }
            _ => return Err(format!("unknown op: {op}")),
        };
        Ok(out.to_string())
    }
}

export!(Plugin);

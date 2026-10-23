//! Usagi (<https://web.usagi.one>), a Russian manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! Ported from the keiyoushi `ru/usagi` extension and its GroupLe multisrc (Apache-2.0). This file is the ABI glue: the
//! ops and the one network path, the `http` import. The URLs and parsers are in `site.rs`.

mod site;

use serde::Deserialize;
use serde_json::{Value, json};

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use kuro::source::host;
use site::Res;

/// The source id the app derives for it: `generate_id("Usagi", "ru", 1)`, the MD5 of `usagi/ru/1`, as the Kotlin
/// extension has it. Keep it: backups and the library name it.
const ID: i64 = 5705163226489175947;
const NAME: &str = "Usagi";
const LANG: &str = "ru";
/// The Kotlin's `rateLimit(2)`: two requests a second.
const REQUEST_GAP_MS: u64 = 500;
/// The Kotlin's default User-Agent. The site answers 404 to title and chapter pages for a browser User-Agent in some
/// countries, which is why GroupLe sends this one.
const USER_AGENT: &str = "arora";

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
struct Envelope {
    #[serde(default)]
    args: Args,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse {
    status: u16,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

fn descriptor() -> Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": true, "requestGapMs": REQUEST_GAP_MS })
}

/// The headers of a page image and of every request: the site as Referer and the Kotlin's User-Agent.
fn headers() -> Value {
    json!([["Referer", "https://web.usagi.one/"], ["User-Agent", USER_AGENT]])
}

/// Fetches a page of the site through the host, which refuses any host outside `hosts` in `manifest.toml`.
fn get(url: &str) -> Res<String> {
    let request = json!({ "method": "GET", "url": url, "headers": headers() });
    let response: HttpResponse = serde_json::from_str(&host::http(&request.to_string())?).map_err(|e| format!("invalid http reply: {e}"))?;
    if response.status == 404 && url.contains("mtr=") {
        return Err("Для просмотра главы необходима авторизация на сайте (ошибка 404).".into());
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("HTTP {} from {url}", response.status));
    }
    // the Kotlin refuses a page that redirected to another site (a moved title)
    if response.url.as_deref().is_some_and(|u| !site::on_site(u)) {
        return Err(format!("{url} moved to another site: {}", response.url.unwrap_or_default()));
    }
    if response.body_encoding.is_some() {
        return Err(format!("{url} is not UTF-8 text"));
    }
    Ok(response.body)
}

fn to_value<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn run(op: &str, args: Args) -> Res<Value> {
    match op {
        "popular" => to_value(site::parse_catalog(&get(&site::catalog_url("RATING", args.page, ""))?)?),
        "latest" => to_value(site::parse_catalog(&get(&site::catalog_url("DATE_UPDATE", args.page, ""))?)?),
        "search" => to_value(site::parse_catalog(&get(&site::catalog_url("RATING", args.page, &args.query))?)?),
        "details" => {
            let manga = args.manga.ok_or("details needs args.manga")?;
            let mut found = site::parse_details(&get(&site::page(&manga.url)?)?)?;
            found.url = manga.url;
            to_value(found)
        }
        "chapters" => {
            let manga = args.manga.ok_or("chapters needs args.manga")?;
            to_value(site::parse_chapters(&get(&site::page(&manga.url)?)?)?)
        }
        "pages" => {
            let chapter = args.chapter.ok_or("pages needs args.chapter")?;
            let pages = site::parse_pages(&get(&site::page(&chapter.url)?)?)?;
            to_value(pages.into_iter().map(|image| json!({ "url": "", "imageUrl": image })).collect::<Vec<_>>())
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
            // ABI 1.5: the page-image headers. They replace the app's default set for this source.
            "request-headers" => headers(),
            "popular" | "latest" | "search" | "details" | "chapters" | "pages" => {
                let envelope: Envelope = serde_json::from_str(&input_json).map_err(|e| format!("invalid input: {e}"))?;
                run(&op, envelope.args)?
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
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(500)));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(<Plugin as Guest>::invoke("hosters".into(), "{}".into()), Err("unknown op: hosters".to_owned()));
    }
}

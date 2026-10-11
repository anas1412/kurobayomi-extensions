//! Request building and response parsing as pure functions, so tests run them over saved responses.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use kuro_source::SourceError;
use kuro_source::model::{MangasPage, Page, SChapter, SManga};
use serde::de::DeserializeOwned;
use url::Url;

use crate::dto::{ChapterResult, Envelope, Items, Manga};
use crate::keys::Keys;

/// Query parameters by raw name (`content_rating[]` for a list), as the Kotlin `nativeMangaParams` map.
pub type Params = BTreeMap<String, Vec<String>>;

/// Kotlin `canonicalEntries`: names in order, `[]` lists numbered `name[0]`, `name[1]`, a single plain value as is.
pub fn canonical(params: &Params) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (raw, values) in params {
        let name = raw.strip_suffix("[]").unwrap_or(raw);
        if values.len() == 1 && !raw.ends_with("[]") {
            out.push((name.to_owned(), values[0].clone()));
        } else {
            out.extend(
                values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (format!("{name}[{i}]"), v.clone())),
            );
        }
    }
    out
}

/// Percent-encoding for a query part; a space is `%20`, as OkHttp writes it.
fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// The signed address of an API call: `{base}/api/v1{path}?{canonical query}&_={token}`. Values are trimmed first, so
/// the address and the signed text agree.
pub fn signed_url(
    base: &str,
    path: &str,
    params: &Params,
    keys: &Keys,
) -> Result<Url, SourceError> {
    let entries: Vec<(String, String)> = canonical(params)
        .into_iter()
        .map(|(k, v)| (k, v.trim().to_owned()))
        .collect();
    let text = entries
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let mut query: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect();
    query.push(format!("_={}", keys.sign(path, &text)));
    Url::parse(&format!("{base}/api/v1{path}?{}", query.join("&")))
        .map_err(|e| SourceError::Url(e.to_string()))
}

/// An API body as `T`: the `result` of `{"status":"ok","result":...}`, decrypted first when it is `{"e": ...}`.
pub fn decode<T: DeserializeOwned>(body: &[u8], keys: &Keys) -> Result<T, SourceError> {
    let parse = |e: serde_json::Error| SourceError::Parse(format!("comix json: {e}"));
    let value: serde_json::Value = serde_json::from_slice(body).map_err(parse)?;
    let value = match value.get("e").and_then(serde_json::Value::as_str) {
        Some(e) => serde_json::from_slice(&keys.decrypt(e)?).map_err(parse)?,
        None => value,
    };
    serde_json::from_value::<Envelope<T>>(value)
        .map(|e| e.result)
        .map_err(parse)
}

pub fn manga_page(body: &[u8], keys: &Keys) -> Result<MangasPage, SourceError> {
    let items: Items<Manga> = decode(body, keys)?;
    Ok(MangasPage {
        mangas: items.items.iter().map(Manga::to_basic).collect(),
        has_next_page: items.has_next_page(),
    })
}

/// One page of chapters and whether another follows.
pub fn chapter_page(
    body: &[u8],
    keys: &Keys,
    manga_slug: &str,
    now: DateTime<Utc>,
) -> Result<(Vec<SChapter>, bool), SourceError> {
    let items: Items<crate::dto::Chapter> = decode(body, keys)?;
    Ok((
        items
            .items
            .iter()
            .map(|c| c.to_chapter(manga_slug, now))
            .collect(),
        items.has_next_page(),
    ))
}

/// Kotlin `selectChapters` with deduplication and the scanlator blacklist off (their defaults): one per URL, highest
/// number first.
pub fn select_chapters(mut chapters: Vec<SChapter>) -> Vec<SChapter> {
    let mut seen = std::collections::HashSet::new();
    chapters.retain(|c| seen.insert(c.url.clone()));
    chapters.sort_by(|a, b| b.chapter_number.total_cmp(&a.chapter_number));
    chapters
}

/// The manga detail inside a title page: the `initial-data` query whose key names `"detail"`.
pub fn details(html: &str) -> Result<SManga, SourceError> {
    let doc = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("script#initial-data").expect("static selector");
    let data = doc
        .select(&selector)
        .next()
        .map(|s| s.text().collect::<String>())
        .ok_or_else(|| SourceError::Parse("Could not find initial data in page".into()))?;
    let value: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| SourceError::Parse(e.to_string()))?;
    let detail = value
        .get("queries")
        .and_then(serde_json::Value::as_object)
        .and_then(|q| q.iter().find(|(k, _)| k.contains("\"detail\"")))
        .map(|(_, v)| v.clone())
        .ok_or_else(|| SourceError::Parse("Could not find manga detail in queries".into()))?;
    let manga: Manga =
        serde_json::from_value(detail).map_err(|e| SourceError::Parse(e.to_string()))?;
    Ok(manga.to_details())
}

/// Kotlin `buildPages`: absolute image URLs, the `v3` flag for grid-scrambled pages, and the retry hosts in the
/// fragment `#alt=host1,host2,...` (the fragment never reaches the server).
pub fn pages(body: &[u8], keys: &Keys) -> Result<Vec<Page>, SourceError> {
    let result: ChapterResult = decode(body, keys)?;
    let pages = result.pages;
    let base = pages.base_url.trim_end_matches('/');
    let mut out = Vec::new();
    for (index, item) in pages.items.iter().enumerate() {
        let full = if item.url.starts_with("http") {
            item.url.clone()
        } else {
            format!("{base}/{}", item.url.trim_start_matches('/'))
        };
        let mut url = Url::parse(&full).map_err(|e| SourceError::Url(format!("{full}: {e}")))?;
        let has_v3 = url.query_pairs().any(|(k, _)| k == "v3");
        if item.s == 1 && !has_v3 {
            url.query_pairs_mut().append_key_only("v3");
        }
        let alt = pages.alt_hosts(url.host_str().unwrap_or(""), index);
        if !alt.is_empty() {
            url.set_fragment(Some(&format!("alt={}", alt.join(","))));
        }
        out.push(Page {
            image_url: Some(url.into()),
            ..Page::new(i32::try_from(index).unwrap_or(i32::MAX), "")
        });
    }
    Ok(out)
}

/// The id in a stored manga URL: `/x0ynk-villains-...` gives `x0ynk`.
pub fn manga_id(manga_url: &str) -> Option<&str> {
    let slug = manga_slug(manga_url);
    slug.split('-').next().filter(|id| !id.is_empty())
}

/// `x0ynk-villains-...` from `/x0ynk-villains-...`.
pub fn manga_slug(manga_url: &str) -> &str {
    manga_url
        .trim_start_matches('/')
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
}

/// The id in a stored chapter URL: `title/x0ynk-.../11452041-chapter-214` gives `11452041`.
pub fn chapter_id(chapter_url: &str) -> Option<i64> {
    let last = chapter_url.trim_end_matches('/').rsplit('/').next()?;
    last.split("-chapter-").next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &[&str])]) -> Params {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.iter().map(|s| (*s).to_owned()).collect()))
            .collect()
    }

    #[test]
    fn the_canonical_query_numbers_lists_and_sorts_names() {
        let p = params(&[
            ("page", &["1"]),
            ("order[score]", &["desc"]),
            ("content_rating[]", &["safe", "suggestive"]),
            ("limit", &["28"]),
        ]);
        let text: Vec<String> = canonical(&p)
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        assert_eq!(
            text.join("&"),
            "content_rating[0]=safe&content_rating[1]=suggestive&limit=28&order[score]=desc&page=1"
        );
    }

    /// The canonical text for the browse call matches what the site's signer was given in a browser (its
    /// `TextEncoder` input began `/manga?content_rating[0]=safe&content_rating[1]=suggestive&l`).
    #[test]
    fn a_signed_url_carries_the_encoded_query_and_the_token() {
        let keys = Keys::current();
        let p = params(&[("limit", &["28"]), ("page", &["1"])]);
        assert_eq!(
            signed_url("https://comix.to", "/manga", &p, &keys)
                .unwrap()
                .as_str(),
            "https://comix.to/api/v1/manga?limit=28&page=1&_=17w.gZzX1AqVvudWzvBR"
        );
        let p = params(&[("keyword", &[" naruto & co/é+ "]), ("page", &["1"])]);
        let url = signed_url("https://comix.to", "/manga", &p, &keys).unwrap();
        assert_eq!(
            url.as_str(),
            "https://comix.to/api/v1/manga?keyword=naruto%20%26%20co%2F%C3%A9%2B&page=1&_=17w.9Cq8M827sGNHQIjG"
        );
        assert_eq!(url.query_pairs().next().unwrap().1, "naruto & co/é+");
    }

    #[test]
    fn ids_come_from_the_stored_urls() {
        assert_eq!(
            manga_id("/x0ynk-villains-are-destined-to-die"),
            Some("x0ynk")
        );
        assert_eq!(
            manga_slug("/x0ynk-villains-are-destined-to-die"),
            "x0ynk-villains-are-destined-to-die"
        );
        assert_eq!(manga_id("/"), None);
        assert_eq!(
            chapter_id("title/x0ynk-villains-are-destined-to-die/11452041-chapter-214"),
            Some(11452041)
        );
        assert_eq!(chapter_id("title/x/abc"), None);
    }

    #[test]
    fn chapters_are_unique_by_url_and_highest_first() {
        let c = |url: &str, n: f32| SChapter {
            chapter_number: n,
            ..SChapter::new(url, url)
        };
        let out = select_chapters(vec![c("a", 1.0), c("b", 3.0), c("a", 1.0), c("c", 2.5)]);
        assert_eq!(
            out.iter().map(|c| c.url.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
    }

    /// Image entries copied from the real decrypted page list of chapter 11452041 (browser, 2026-10-10); the
    /// surrounding object is cut down to the fields the parser reads.
    #[test]
    fn pages_are_absolute_with_retry_hosts() {
        let body = br#"{"status":"ok","result":{"id":11452041,"pages":{"alt":["trueglow85.site","deepforce74.site","wildlife97.site","brightlife09.site","boldstep66.site"],"node":"lov","items":[{"width":720,"height":1408,"url":"https://8pl.markscorner.site/hi/bEqPbYfoNT0Gm1XlBiKfoD5Y4rkdeu6i3R0VvpLI6y4AiVMhaGDNl"},{"width":720,"height":1408,"url":"https://8pl.realtalkjournal.site/hi/bEqPbYfoNT0Gm1XlBiKfoD5Y4rkdeu6i3R0VvpLI6y4AiV8haGDNl"}]}}}"#;
        let out = pages(body, &Keys::current()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].index, 1);
        assert_eq!(
            out[1].image_url.as_deref(),
            Some(
                "https://8pl.realtalkjournal.site/hi/bEqPbYfoNT0Gm1XlBiKfoD5Y4rkdeu6i3R0VvpLI6y4AiV8haGDNl#alt=8pl.deepforce74.site,8pl.wildlife97.site,8pl.brightlife09.site,8pl.boldstep66.site,8pl.trueglow85.site,lov.deepforce74.site"
            )
        );
        let v3 = br#"{"result":{"pages":{"baseUrl":"https://img.example.site/","items":[{"url":"/p/1.jpg","s":1}]}}}"#;
        assert_eq!(
            pages(v3, &Keys::current()).unwrap()[0].image_url.as_deref(),
            Some("https://img.example.site/p/1.jpg?v3")
        );
    }
}

//! The parsers over responses saved from the real site on 2026-10-10 (no network).
//!
//! - `popular.json`: `GET /api/v1/manga?limit=28&page=1&_=...` (plain JSON).
//! - `title.html`: `GET /title/x0ynk-villains-are-destined-to-die`.
//! - `chapters_encrypted.json`: `GET /api/v1/manga/px0j/chapters?limit=100&order[number]=desc&page=1&_=...`, an
//!   `{"e": ...}` body whose plain text the site's own decoder gave in a browser.

use kuro_source::model::manga;
use kuro_src_comix::{Keys, api};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .expect("fixture")
}

#[test]
fn a_listing_gives_mangas_with_kotlin_urls_and_large_posters() {
    let page = api::manga_page(&fixture("popular.json"), &Keys::current()).unwrap();
    assert_eq!(page.mangas.len(), 28);
    assert!(page.has_next_page, "page 1 of 3342");
    let first = &page.mangas[0];
    assert_eq!(
        (first.url.as_str(), first.title.as_str()),
        ("/px0j-androids-have-no-blood", "Androids Have No Blood")
    );
    assert_eq!(
        first.thumbnail_url.as_deref(),
        Some("https://static.comix.to/29c9/i/f/4b/68de559c9da52.jpg")
    );
    assert!(
        page.mangas
            .iter()
            .all(|m| m.url.starts_with('/') && !m.title.is_empty())
    );
}

#[test]
fn the_title_page_gives_the_details() {
    let details = api::details(&String::from_utf8(fixture("title.html")).unwrap()).unwrap();
    assert_eq!(details.title, "Villains Are Destined to Die");
    assert_eq!(details.url, "/x0ynk-villains-are-destined-to-die");
    assert!(details.initialized);
    assert_ne!(details.status, manga::UNKNOWN);
    let genre = details.genre.as_deref().unwrap();
    assert!(genre.starts_with("Manhwa"), "{genre}");
    let description = details.description.as_deref().unwrap();
    assert!(description.contains("**Content rating**"), "{description}");
    assert!(
        details
            .thumbnail_url
            .as_deref()
            .is_some_and(|u| u.starts_with("https://"))
    );
    println!("{details:#?}");
}

#[test]
fn an_encrypted_chapter_list_decrypts_to_the_plain_text_the_site_showed() {
    let body = fixture("chapters_encrypted.json");
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let plain = Keys::current()
        .decrypt(value["e"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        String::from_utf8(plain).unwrap(),
        r#"{"status":"ok","result":{"items":[],"meta":{"total":0,"perPage":100,"page":1,"lastPage":1,"from":0,"to":0,"hasNext":false,"hasPrev":false}}}"#
    );
    let (chapters, next) = api::chapter_page(
        &body,
        &Keys::current(),
        "px0j-androids-have-no-blood",
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(chapters.is_empty() && !next);
}

#[test]
fn the_registered_source_has_the_kotlin_identity_and_filters() {
    use kuro_source::{CatalogueSource, MangaSource};
    let dir = tempfile::tempdir().unwrap();
    let prefs = kuro_prefs::network::NetworkPrefs::new(kuro_prefs::store::PrefStore::in_memory());
    let (toaster, _feed) = kuro_platform::toast::toast_channel();
    let client = kuro_net::NetworkClient::new(
        &prefs,
        dir.path().join("cookies.json"),
        std::sync::Arc::new(kuro_platform::webview::NoWebView),
        toaster,
    )
    .unwrap();
    let source = kuro_src_comix::source(client);
    assert_eq!(
        (source.id(), source.name(), source.lang()),
        (kuro_source::generate_id("Comix", "en", 1), "Comix", "en")
    );
    assert!(source.supports_latest());
    assert!(source.filter_list().0.iter().any(|f| f.name() == "Genres"));
    assert!(!source.image_headers().contains_key("referer"));
}

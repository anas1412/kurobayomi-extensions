//! Comix (comix.to), a built-in manga source ported from the keiyoushi `en/comix` extension.
//!
//! - [`keys`]: the request signature and response cipher the site uses now (not the Kotlin substitution cipher).
//! - [`api`]: request building and parsing as pure functions; [`dto`]: the JSON; [`filters`]: the search filters.
//! - [`descramble`]: the XOR and 5x5 grid decoding of page images, for the image loader to call.
//! - [`Comix`]: the source traits over the app HTTP client, one request at a time, Comix hosts only.

pub mod api;
pub mod descramble;
pub mod dto;
pub mod filters;
pub mod keys;

use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use http::{HeaderMap, HeaderValue, header};
use kuro_net::NetworkClient;
use kuro_source::model::{MangasPage, Page, SChapter, SManga};
use kuro_source::{CatalogueSource, FilterList, MangaSource, SourceError};
use kuro_source_http::http_source::{HttpCore, default_headers};
use kuro_source_http::response::HttpResponse;
use kuro_source_http::{map_net_error, request};
use tokio::sync::Mutex;
use tokio::time::Instant;
use url::Url;

pub use keys::Keys;

pub const NAME: &str = "Comix";
pub const LANG: &str = "en";
pub const BASE_URL: &str = "https://comix.to";
/// The mirrors of the Kotlin source; requests go to these hosts (and their subdomains) only.
pub const HOSTS: [&str; 2] = ["comix.to", "comix.ws"];
/// Kotlin `rateLimit(5)`: five requests a second.
const INTERVAL: Duration = Duration::from_millis(200);
const MAX_CHAPTER_PAGES: u32 = 200;

/// The source the app registers.
pub fn source(client: NetworkClient) -> Comix {
    Comix::new(client, BASE_URL, INTERVAL)
}

pub struct Comix {
    core: HttpCore,
    base: String,
    headers: HeaderMap,
    keys: RwLock<Arc<Keys>>,
    interval: Duration,
    last_done: Mutex<Option<Instant>>,
}

impl Comix {
    /// `base` is one of the mirrors (no trailing slash); `interval` is the least time between two requests.
    pub fn new(client: NetworkClient, base: &str, interval: Duration) -> Self {
        let core = HttpCore::new(client);
        let mut headers = default_headers(&core);
        headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
        let base = base.trim_end_matches('/').to_owned();
        Self {
            core,
            base,
            headers,
            keys: RwLock::new(Arc::new(Keys::current())),
            interval,
            last_done: Mutex::new(None),
        }
    }

    /// Replaces the request keys, for when the site ships a new build (see `capture.js` and the README).
    pub fn set_keys(&self, keys: Keys) {
        *self.keys.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(keys);
    }

    pub fn keys(&self) -> Arc<Keys> {
        self.keys.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The headers for page images: the image hosts refuse any request with `Referer` or `Origin`, so none is sent.
    pub fn image_headers(&self) -> HeaderMap {
        let mut headers = self.headers.clone();
        headers.remove(header::REFERER);
        headers.remove(header::ORIGIN);
        headers
    }

    pub fn manga_web_url(&self, manga: &SManga) -> String {
        format!("{}/title{}", self.base, manga.url)
    }

    pub fn chapter_web_url(&self, chapter: &SChapter) -> String {
        format!("{}/{}", self.base, chapter.url.trim_start_matches('/'))
    }

    fn check_host(url: &Url) -> Result<(), SourceError> {
        let host = url.host_str().unwrap_or("");
        if url.scheme() == "https"
            && HOSTS
                .iter()
                .any(|h| host == *h || host.ends_with(&format!(".{h}")))
        {
            Ok(())
        } else {
            Err(SourceError::Url(format!("{url} is not a Comix address")))
        }
    }

    /// One GET, after the previous one by at least the interval. A 403 from the API says why (the site answers
    /// `{"code":"missing_token"}` or similar when a signature is refused).
    async fn get(&self, url: &Url) -> Result<HttpResponse, SourceError> {
        Self::check_host(url)?;
        let req = request::get(url.as_str(), &self.headers)?;
        let mut last = self.last_done.lock().await;
        if let Some(done) = *last {
            tokio::time::sleep_until(done + self.interval).await;
        }
        let outcome = self.core.client().execute(req).await.map_err(map_net_error);
        *last = Some(Instant::now());
        drop(last);
        let response = outcome?;
        if response.status.as_u16() == 403 && url.path().starts_with("/api/") {
            let body = response.text().await.unwrap_or_default();
            let keys = self.keys();
            return Err(SourceError::Parse(format!(
                "Comix refused the request signature (keys of site build {}): {}. The site keys have probably changed; recapture them with capture.js.",
                keys.build,
                body.trim()
            )));
        }
        let response = HttpResponse::read(response).await?;
        Self::check_host(&response.url)?;
        if let Some(build) = response
            .headers
            .get("x-build-id")
            .and_then(|v| v.to_str().ok())
            && build != self.keys().build
        {
            tracing::warn!(site = build, keys = %self.keys().build, "comix: the site build differs from the build of the request keys");
        }
        Ok(response)
    }

    async fn api(
        &self,
        path: &str,
        params: &api::Params,
    ) -> Result<(Vec<u8>, Arc<Keys>), SourceError> {
        let keys = self.keys();
        let url = api::signed_url(&self.base, path, params, &keys)?;
        Ok((self.get(&url).await?.body.to_vec(), keys))
    }

    async fn browse(&self, mut params: api::Params, page: i32) -> Result<MangasPage, SourceError> {
        params.insert("page".into(), vec![page.to_string()]);
        params
            .entry("limit".into())
            .or_insert_with(|| vec!["28".into()]);
        let (body, keys) = self.api("/manga", &params).await?;
        api::manga_page(&body, &keys)
    }

    async fn listing(&self, order: &str, page: i32) -> Result<MangasPage, SourceError> {
        let mut params = api::Params::new();
        params.insert(format!("order[{order}]"), vec!["desc".into()]);
        filters::preference_params(&mut params);
        self.browse(params, page).await
    }
}

#[async_trait]
impl MangaSource for Comix {
    fn id(&self) -> i64 {
        self.core.id(NAME, LANG, 1)
    }

    fn name(&self) -> &str {
        NAME
    }

    fn lang(&self) -> &str {
        LANG
    }

    async fn get_manga_details(&self, manga: &SManga) -> Result<SManga, SourceError> {
        let url =
            Url::parse(&self.manga_web_url(manga)).map_err(|e| SourceError::Url(e.to_string()))?;
        let mut out = api::details(&self.get(&url).await?.text())?;
        out.url = manga.url.clone();
        Ok(out)
    }

    async fn get_chapter_list(&self, manga: &SManga) -> Result<Vec<SChapter>, SourceError> {
        let id = api::manga_id(&manga.url)
            .ok_or_else(|| SourceError::Url(format!("no manga id in {}", manga.url)))?;
        let slug = api::manga_slug(&manga.url);
        let path = format!("/manga/{id}/chapters");
        let mut all = Vec::new();
        for page in 1..=MAX_CHAPTER_PAGES {
            let params: api::Params = [
                ("limit", "100"),
                ("order[number]", "desc"),
                ("page", &page.to_string()),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), vec![v.to_owned()]))
            .collect();
            let (body, keys) = self.api(&path, &params).await?;
            let (chapters, has_next) = api::chapter_page(&body, &keys, slug, chrono::Utc::now())?;
            let empty = chapters.is_empty();
            all.extend(chapters);
            if !has_next || empty {
                break;
            }
        }
        Ok(api::select_chapters(all))
    }

    async fn get_page_list(&self, chapter: &SChapter) -> Result<Vec<Page>, SourceError> {
        let id = api::chapter_id(&chapter.url)
            .ok_or_else(|| SourceError::Url(format!("no chapter id in {}", chapter.url)))?;
        let (body, keys) = self
            .api(&format!("/chapters/{id}"), &api::Params::new())
            .await?;
        let pages = api::pages(&body, &keys)?;
        if pages.is_empty() {
            Err(SourceError::EmptyPageList)
        } else {
            Ok(pages)
        }
    }
}

#[async_trait]
impl CatalogueSource for Comix {
    fn supports_latest(&self) -> bool {
        true
    }

    fn filter_list(&self) -> FilterList {
        filters::filter_list()
    }

    async fn popular(&self, page: i32) -> Result<MangasPage, SourceError> {
        self.listing("score", page).await
    }

    async fn search(
        &self,
        page: i32,
        query: &str,
        filters: &FilterList,
    ) -> Result<MangasPage, SourceError> {
        self.browse(filters::search_params(filters, query)?, page)
            .await
    }

    async fn latest(&self, page: i32) -> Result<MangasPage, SourceError> {
        self.listing("chapter_updated_at", page).await
    }
}

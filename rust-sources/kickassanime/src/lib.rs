//! KickAssAnime (<https://kaa.lt>), a built-in anime source ported from the yuzono `en/kickassanime` extension
//! (Apache-2.0). The site is a JSON API under `/api`; each episode lists servers whose player pages give an HLS or
//! DASH manifest and subtitles.
//!
//! The pure parts are in [`api`] (URLs, filters, one parser per response) and [`extract`] (player pages, the older
//! player's SHA-1 signature and AES decryption, HLS and DASH splitting), so tests run over saved responses.
//! [`KickAssAnime`] is the only network path: the API on `kaa.lt` only, the players over https only, one request at a
//! time with [`INTERVAL`] between them.

pub mod api;
pub mod extract;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use http::header::{ACCEPT, CONTENT_TYPE, ORIGIN, REFERER, USER_AGENT};
use http::{HeaderMap, HeaderName, HeaderValue};
use kuro_net::{Body, NetworkClient, Request};
use kuro_source::anime::{AnimeSource, AnimesPage, Hoster, SAnime, SEpisode, Video};
use kuro_source::{FilterList, SourceError};
use kuro_source_http::http_source::{HttpCore, default_headers};
use kuro_source_http::response::HttpResponse;
use kuro_source_http::{map_net_error, request};
use tokio::sync::Mutex;
use tokio::time::Instant;
use url::Url;

use crate::api::{BASE_URL, HOST, SearchRequest};
use crate::extract::SERVERS;

pub const NAME: &str = "KickAssAnime";
pub const LANG: &str = "en";
/// The gap between two requests of the source.
pub const INTERVAL: Duration = Duration::from_millis(300);

// The Kotlin settings, at their defaults. ponytail: fixed until the app gives built-in sources a settings store.
const USE_ENGLISH: bool = false;
const AUDIO_FIRST: &str = "ja-JP";
const AUDIO_SECOND: &str = "en-US";
const QUALITY: &str = "1080p";
const SERVER: &str = SERVERS[0];

/// The source. Build it with [`source`].
pub struct KickAssAnime {
    core: HttpCore,
    headers: HeaderMap,
    interval: Duration,
    last_done: Mutex<Option<Instant>>,
}

/// The source over the app's client.
pub fn source(client: NetworkClient) -> KickAssAnime {
    with_interval(client, INTERVAL)
}

/// The source with another gap between requests (the live check uses one second).
pub fn with_interval(client: NetworkClient, interval: Duration) -> KickAssAnime {
    let core = HttpCore::new(client);
    let headers = default_headers(&core);
    KickAssAnime { core, headers, interval, last_done: Mutex::new(None) }
}

fn header(map: &mut HeaderMap, name: HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        map.insert(name, value);
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl KickAssAnime {
    /// Sends one request: API calls only to `kaa.lt`, everything over https, one at a time with the interval between.
    async fn send(&self, request: Request, api: bool) -> Result<HttpResponse, SourceError> {
        let check = |url: &Url| {
            if url.scheme() != "https" || (api && url.host_str() != Some(HOST)) {
                return Err(SourceError::Url(format!("refused address {url}")));
            }
            Ok(())
        };
        check(&request.url)?;
        let mut last = self.last_done.lock().await;
        if let Some(done) = *last {
            tokio::time::sleep_until(done + self.interval).await;
        }
        let outcome = self.core.client().execute(request).await.map_err(map_net_error);
        let response = match outcome {
            Ok(response) => HttpResponse::read(response).await,
            Err(err) => Err(err),
        };
        *last = Some(Instant::now());
        drop(last);
        let response = response?;
        check(&response.url)?;
        Ok(response)
    }

    async fn api(&self, url: &str) -> Result<HttpResponse, SourceError> {
        self.send(request::get(url, &self.headers)?, true).await
    }

    async fn get(&self, url: &str, headers: &HeaderMap) -> Result<String, SourceError> {
        Ok(self.send(request::get(url, headers)?, false).await?.text())
    }

    /// The JSON headers of a search request (Kotlin `searchAnimeRequest`).
    fn search_headers(&self, query: &str) -> HeaderMap {
        let mut headers = self.headers.clone();
        header(&mut headers, ACCEPT, "application/json, text/plain, */*");
        let referer =
            url::form_urlencoded::Serializer::new(format!("{BASE_URL}/search?")).append_pair("q", query).finish();
        header(&mut headers, REFERER, &referer);
        headers
    }

    async fn languages(&self, anime_url: &str) -> Result<Vec<String>, SourceError> {
        api::parse_languages(&self.api(&api::language_url(anime_url)).await?.body)
    }

    async fn details_of(&self, anime_url: &str) -> Result<SAnime, SourceError> {
        let body = self.api(&api::details_url(anime_url)).await?.body;
        let languages = self.languages(anime_url).await?;
        api::parse_details(&body, &languages, USE_ENGLISH)
    }

    /// The servers of an episode as they come from the site.
    pub async fn servers(&self, episode: &SEpisode) -> Result<Vec<api::Server>, SourceError> {
        api::parse_servers(&self.api(&api::servers_url(&episode.url)).await?.body)
    }

    /// The videos of one server (Kotlin `videosFromUrl`). A server the extractor cannot read gives no videos.
    pub async fn server_videos(&self, name: &str, src: &str) -> Result<Vec<Video>, SourceError> {
        let player = extract::player_url(src)?;
        let html = self.get(player.as_str(), &self.headers).await?;
        let video_headers = extract::video_headers(&player);
        if let Some(data) = extract::parse_new_player(&html, &player) {
            return Ok(self.manifest_videos(name, &data.manifest, &data.subtitles, &video_headers).await);
        }
        if !html.contains("cid: '") {
            return Ok(Vec::new());
        }
        let agent = self.headers.get(USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or_default();
        let (Some(key), Some(source_url)) =
            (extract::legacy_key(name), extract::legacy_source_url(&html, &player, name, agent, now_secs()))
        else {
            return Ok(Vec::new());
        };
        let mut headers = self.headers.clone();
        header(&mut headers, REFERER, player.as_str());
        header(&mut headers, ORIGIN, &format!("https://{}", player.host_str().unwrap_or_default()));
        let answer = self.get(&source_url, &headers).await?;
        let dto = match extract::decrypt_source(&answer, key) {
            Ok(dto) => dto,
            Err(err) => {
                tracing::warn!(%err, name, "kickassanime: older player answer did not decrypt");
                return Ok(Vec::new());
            }
        };
        let subtitles = extract::legacy_subtitles(&dto, &player);
        let manifest = extract::fix_url(if dto.hls.is_empty() { &dto.dash } else { &dto.hls }, &player);
        let dash = dto.hls.trim().is_empty();
        Ok(self.playlist_videos(name, &manifest, dash, &subtitles, &video_headers).await)
    }

    async fn manifest_videos(
        &self,
        name: &str,
        manifest: &str,
        subs: &[kuro_source::anime::Track],
        headers: &[(String, String)],
    ) -> Vec<Video> {
        self.playlist_videos(name, manifest, !manifest.contains(".m3u8"), subs, headers).await
    }

    /// Fetches the manifest with the video headers and splits it per quality. A failure gives no videos (Kotlin).
    async fn playlist_videos(
        &self,
        name: &str,
        manifest: &str,
        dash: bool,
        subtitles: &[kuro_source::anime::Track],
        headers: &[(String, String)],
    ) -> Vec<Video> {
        let mut map = HeaderMap::new();
        for (k, v) in headers {
            if let Ok(k) = HeaderName::from_bytes(k.as_bytes()) {
                header(&mut map, k, v);
            }
        }
        match self.get(manifest, &map).await {
            Ok(text) if dash => extract::dash_videos(name, manifest, &text, subtitles, headers),
            Ok(text) => extract::hls_videos(name, manifest, &text, subtitles, headers),
            Err(err) => {
                tracing::warn!(%err, name, manifest, "kickassanime: manifest could not be fetched");
                Vec::new()
            }
        }
    }
}

#[async_trait]
impl AnimeSource for KickAssAnime {
    fn id(&self) -> i64 {
        self.core.id(NAME, LANG, 1)
    }

    fn name(&self) -> &str {
        NAME
    }

    fn lang(&self) -> &str {
        LANG
    }

    fn supports_latest(&self) -> bool {
        true
    }

    fn supports_season_list(&self) -> bool {
        false
    }

    fn filter_list(&self) -> FilterList {
        api::filter_list()
    }

    async fn popular(&self, page: i32) -> Result<AnimesPage, SourceError> {
        api::parse_popular(&self.api(&api::popular_url(page)).await?.body, page, USE_ENGLISH)
    }

    async fn latest(&self, page: i32) -> Result<AnimesPage, SourceError> {
        api::parse_latest(&self.api(&api::latest_url(page)).await?.body, USE_ENGLISH)
    }

    async fn search(&self, page: i32, query: &str, filters: &FilterList) -> Result<AnimesPage, SourceError> {
        if let Some(slug) = api::slug_of_query(query)? {
            let anime = self.details_of(&format!("/{slug}")).await?;
            return Ok(AnimesPage { animes: vec![anime], has_next_page: false });
        }
        let params = api::search_params(filters);
        match api::search_request(page, query, &params) {
            SearchRequest::SubPage(url) => {
                let response = self.send(request::get(&url, &self.search_headers(query))?, true).await?;
                api::parse_sub_page(response.url.path(), &response.body, page, USE_ENGLISH)
            }
            SearchRequest::Browse { url } => {
                let response = self.send(request::get(&url, &self.search_headers(query))?, true).await?;
                api::parse_search(&response.body, page, USE_ENGLISH)
            }
            SearchRequest::Text { body } => {
                let mut headers = self.search_headers(query);
                header(&mut headers, CONTENT_TYPE, "application/json");
                let body = Body::Bytes { content_type: "application/json".into(), data: body.into_bytes().into() };
                let post =
                    kuro_net::post(&format!("{BASE_URL}/api/fsearch"), body).map_err(map_net_error)?.headers(headers);
                api::parse_search(&self.send(post, true).await?.body, page, USE_ENGLISH)
            }
        }
    }

    async fn anime_details(&self, anime: &SAnime) -> Result<SAnime, SourceError> {
        self.details_of(&anime.url).await
    }

    /// The episodes in the first language that has any: the preferred one, the second, then the site's order. Newest
    /// first. A page after the first that fails is skipped, as in Kotlin.
    async fn episode_list(&self, anime: &SAnime) -> Result<Vec<SEpisode>, SourceError> {
        let languages = api::language_order(self.languages(&anime.url).await?, AUDIO_FIRST, AUDIO_SECOND);
        for lang in languages {
            let first = match self
                .api(&api::episodes_url(&anime.url, 1, &lang))
                .await
                .and_then(|r| api::parse_episode_page(&r.body))
            {
                Ok(page) if !page.is_empty() => page,
                _ => continue,
            };
            let mut episodes = first.episodes(&anime.url, &lang);
            for number in 2..=first.page_count() {
                match self
                    .api(&api::episodes_url(&anime.url, number, &lang))
                    .await
                    .and_then(|r| api::parse_episode_page(&r.body))
                {
                    Ok(page) => episodes.extend(page.episodes(&anime.url, &lang)),
                    Err(err) => tracing::warn!(%err, number, "kickassanime: an episode page failed"),
                }
            }
            episodes.reverse();
            return Ok(episodes);
        }
        Ok(Vec::new())
    }

    fn provides_hosters(&self) -> bool {
        true
    }

    /// One hoster per server; its videos are fetched when it is loaded.
    async fn hosters(&self, episode: &SEpisode) -> Result<Vec<Hoster>, SourceError> {
        Ok(self
            .servers(episode)
            .await?
            .into_iter()
            .map(|s| Hoster { hoster_url: s.src, hoster_name: s.name, ..Hoster::default() })
            .collect())
    }

    async fn videos_for_hoster(&self, hoster: &Hoster) -> Result<Vec<Video>, SourceError> {
        self.server_videos(&hoster.hoster_name, &hoster.hoster_url).await.map(|v| self.sort_videos(v))
    }

    /// The legacy path: every server's videos. A server that fails is skipped, as in Kotlin.
    async fn videos_for_episode(&self, episode: &SEpisode) -> Result<Vec<Video>, SourceError> {
        let mut videos = Vec::new();
        for server in self.servers(episode).await? {
            match self.server_videos(&server.name, &server.src).await {
                Ok(found) => videos.extend(found),
                Err(err) => tracing::warn!(%err, server = server.name, "kickassanime: a server failed"),
            }
        }
        Ok(videos)
    }

    /// The preferred server first.
    fn sort_hosters(&self, mut hosters: Vec<Hoster>) -> Vec<Hoster> {
        hosters.sort_by_key(|h| h.hoster_name != SERVER);
        hosters
    }

    fn sort_videos(&self, videos: Vec<Video>) -> Vec<Video> {
        extract::sort_videos(videos, SERVER, QUALITY)
    }
}

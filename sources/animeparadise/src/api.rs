//! AnimeParadise's API (`https://api.animeparadise.moe`): the addresses and the parsers. No network here; `lib.rs` sends
//! the requests through the host.
//!
//! The site changed since the Kotlin extension: its list is `/search` (with `q`, `sort` and `page`), the details are
//! `/anime/{slug}`, and an episode's stream comes from `/ep/{uid}?origin={id}` as a `streamLink` the stream host turns
//! into an HLS playlist. The anime and episode urls stay in the Kotlin's form, so library entries carry over.

use serde::{Deserialize, Serialize};

pub type Res<T> = Result<T, String>;

pub const SITE: &str = "https://www.animeparadise.moe";
pub const API: &str = "https://api.animeparadise.moe";
const STREAM: &str = "https://stream.animeparadise.moe/m3u8?url=";

// ============================ Output (wire) ============================

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anime {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimesPage {
    pub animes: Vec<Anime>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub url: String,
    pub name: String,
    pub episode_number: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub url: String,
    pub lang: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub video_url: String,
    pub video_title: String,
    pub headers: Vec<(String, String)>,
    pub preferred: bool,
    pub subtitle_tracks: Vec<Track>,
}

/// The one hoster of an episode, with its video already read (`videoList`), so the app asks nothing more.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hoster {
    pub hoster_url: String,
    pub hoster_name: String,
    pub video_list: Vec<Video>,
}

fn parse<'a, T: Deserialize<'a>>(body: &'a str, what: &str) -> Res<T> {
    serde_json::from_str(body).map_err(|e| format!("unexpected AnimeParadise {what}: {e}"))
}

// ================================ Urls =================================

/// The Kotlin's anime url: `{"slug":...,"id":...}`. The slug names the details page, the id the episode list.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct LinkData {
    pub slug: String,
    pub id: String,
}

pub fn anime_url(slug: &str, id: &str) -> String {
    serde_json::to_string(&LinkData { slug: slug.to_owned(), id: id.to_owned() }).unwrap_or_default()
}

pub fn link_of(anime_url: &str) -> Res<LinkData> {
    serde_json::from_str(anime_url).map_err(|_| format!("not an AnimeParadise entry: {anime_url}"))
}

/// `/search`: `sort` for the popular and latest lists, `q` for a search; pages count from 1, as the app's.
pub fn search_url(page: i32, query: &str, sort: &str) -> String {
    let mut url = format!("{API}/search?page={}", page.max(1));
    if !query.trim().is_empty() {
        url.push_str("&q=");
        url.push_str(&encode(query.trim()));
    }
    if !sort.is_empty() {
        url.push_str("&sort=");
        url.push_str(sort);
    }
    url.push_str("&v=1");
    url
}

/// Percent-encodes a query value (`application/x-www-form-urlencoded` style).
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn details_url(slug: &str) -> String {
    format!("{API}/anime/{slug}")
}

pub fn episodes_url(id: &str) -> String {
    format!("{API}/anime/{id}/episode")
}

/// The API address of an episode url (`/watch/{uid}?origin={id}`): `/ep/{uid}?origin={id}`.
pub fn episode_api_url(episode_url: &str) -> Res<String> {
    let rest = episode_url.strip_prefix("/watch/").ok_or_else(|| format!("not an AnimeParadise episode: {episode_url}"))?;
    Ok(format!("{API}/ep/{rest}"))
}

// =============================== Parsers ===============================

#[derive(Deserialize)]
struct ListDto {
    #[serde(default)]
    data: Vec<AnimeDto>,
    pagination: Option<PaginationDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaginationDto {
    #[serde(default)]
    has_next: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnimeDto {
    #[serde(rename = "_id")]
    id: String,
    title: String,
    link: String,
    #[serde(default)]
    poster_image: Option<ImageDto>,
    #[serde(default)]
    age_rating: Option<String>,
}

#[derive(Deserialize)]
struct ImageDto {
    original: Option<String>,
    large: Option<String>,
    medium: Option<String>,
    small: Option<String>,
}

impl ImageDto {
    /// The biggest picture, as the Kotlin picks it.
    fn best(&self) -> Option<String> {
        [&self.original, &self.large, &self.medium, &self.small].into_iter().flatten().find(|url| !url.is_empty()).cloned()
    }
}

/// An adult title by the site's age rating (`R18`, as Kitsu rates them).
fn is_adult(rating: Option<&str>) -> bool {
    rating.is_some_and(|r| r.trim().eq_ignore_ascii_case("R18") || r.trim().eq_ignore_ascii_case("RX"))
}

/// A list page. Adult titles are left out unless the user shows them (the app's `showAdult`).
pub fn parse_list(body: &str, show_adult: bool) -> Res<AnimesPage> {
    let dto: ListDto = parse(body, "list")?;
    let animes = dto
        .data
        .into_iter()
        .filter(|anime| show_adult || !is_adult(anime.age_rating.as_deref()))
        .map(|anime| Anime {
            url: anime_url(&anime.link, &anime.id),
            title: anime.title,
            description: None,
            thumbnail_url: anime.poster_image.as_ref().and_then(ImageDto::best),
            genre: None,
        })
        .collect();
    Ok(AnimesPage { animes, has_next_page: dto.pagination.is_some_and(|p| p.has_next) })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsDto {
    data: DetailsDataDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsDataDto {
    #[serde(rename = "_id")]
    id: String,
    title: String,
    link: String,
    synopsys: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    poster_image: Option<ImageDto>,
}

pub fn parse_details(body: &str) -> Res<Anime> {
    let dto: DetailsDto = parse(body, "details")?;
    let data = dto.data;
    Ok(Anime {
        url: anime_url(&data.link, &data.id),
        title: data.title,
        description: data.synopsys.filter(|s| !s.trim().is_empty()),
        thumbnail_url: data.poster_image.as_ref().and_then(ImageDto::best),
        genre: Some(data.genres.join(", ")).filter(|g| !g.is_empty()),
    })
}

#[derive(Deserialize)]
struct EpisodesDto {
    #[serde(default)]
    data: Vec<EpisodeDto>,
}

#[derive(Deserialize)]
struct EpisodeDto {
    uid: String,
    origin: String,
    number: Option<String>,
    title: Option<String>,
}

/// The episodes newest first, named and numbered as the Kotlin does ("Ep. 3 - Title").
pub fn parse_episodes(body: &str) -> Res<Vec<Episode>> {
    let dto: EpisodesDto = parse(body, "episode list")?;
    let mut episodes: Vec<Episode> = dto
        .data
        .into_iter()
        .map(|episode| {
            let number = episode.number.as_deref().map(str::trim).filter(|n| !n.is_empty());
            let mut name = number.map_or_else(|| "Episode".to_owned(), |n| format!("Ep. {n}"));
            if let Some(title) = episode.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                name.push_str(" - ");
                name.push_str(title);
            }
            Episode {
                url: format!("/watch/{}?origin={}", episode.uid, episode.origin),
                name,
                episode_number: number.and_then(|n| n.parse().ok()).unwrap_or(1.0),
            }
        })
        .collect();
    episodes.reverse();
    Ok(episodes)
}

#[derive(Deserialize)]
struct WatchDto {
    data: WatchDataDto,
}

#[derive(Deserialize)]
struct WatchDataDto {
    episode: WatchEpisodeDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchEpisodeDto {
    stream_link: Option<String>,
    #[serde(default)]
    sub_data: Vec<SubDto>,
}

#[derive(Deserialize)]
struct SubDto {
    src: String,
    label: String,
}

/// The headers the stream host and the subtitle files expect: the site's own.
pub fn media_headers() -> Vec<(String, String)> {
    vec![("Referer".to_owned(), format!("{SITE}/")), ("Origin".to_owned(), SITE.to_owned())]
}

/// The episode's video: the HLS playlist of its stream link, with the subtitle files the site lists.
pub fn parse_video(body: &str) -> Res<Video> {
    let dto: WatchDto = parse(body, "episode")?;
    let episode = dto.data.episode;
    let link = episode.stream_link.filter(|l| !l.is_empty()).ok_or("AnimeParadise has no stream for this episode")?;
    Ok(Video {
        video_url: format!("{STREAM}{link}"),
        video_title: "AnimeParadise".to_owned(),
        headers: media_headers(),
        preferred: true,
        subtitle_tracks: episode.sub_data.into_iter().map(|sub| Track { url: format!("{API}/stream/file/{}", sub.src), lang: sub.label }).collect(),
    })
}

/// The error text of a refused request, when the API gives one.
pub fn error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value.get("message")?.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../fixtures/search.json");
    const DETAILS: &str = include_str!("../fixtures/details.json");
    const EPISODES: &str = include_str!("../fixtures/episodes.json");
    const EPISODE: &str = include_str!("../fixtures/episode.json");

    #[test]
    fn search_urls_carry_the_page_query_and_sort() {
        assert_eq!(search_url(1, "", "POPULARITY"), "https://api.animeparadise.moe/search?page=1&sort=POPULARITY&v=1");
        assert_eq!(search_url(2, " sousou no frieren ", ""), "https://api.animeparadise.moe/search?page=2&q=sousou+no+frieren&v=1");
        assert_eq!(search_url(0, "a&b", ""), "https://api.animeparadise.moe/search?page=1&q=a%26b&v=1");
    }

    #[test]
    fn a_list_gives_the_kotlin_urls_and_posters() {
        let page = parse_list(SEARCH, false).unwrap();
        let first = &page.animes[0];
        assert_eq!(first.title, "Sousou no Frieren");
        assert_eq!(first.url, r#"{"slug":"sousou-no-frieren","id":"NIUsb960SxtXls4h"}"#);
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://media.kitsu.app/"), "the original poster first, as the Kotlin picks");
        assert!(!page.has_next_page);
        assert_eq!(link_of(&first.url).unwrap(), LinkData { slug: "sousou-no-frieren".into(), id: "NIUsb960SxtXls4h".into() });
    }

    #[test]
    fn adult_titles_are_left_out_unless_shown() {
        let body = r#"{"data":[{"_id":"a","title":"A","link":"a","ageRating":"R18"},{"_id":"b","title":"B","link":"b","ageRating":"PG"}]}"#;
        assert_eq!(parse_list(body, false).unwrap().animes.len(), 1);
        assert_eq!(parse_list(body, true).unwrap().animes.len(), 2);
    }

    #[test]
    fn details_read_the_summary_genres_and_poster() {
        let anime = parse_details(DETAILS).unwrap();
        assert_eq!(anime.title, "Sousou no Frieren");
        assert_eq!(anime.genre.as_deref(), Some("Adventure, Drama, Fantasy"));
        assert!(anime.description.as_deref().unwrap().contains("elf mage"));
        assert_eq!(details_url("sousou-no-frieren"), "https://api.animeparadise.moe/anime/sousou-no-frieren");
    }

    #[test]
    fn episodes_come_newest_first_with_watch_urls() {
        let episodes = parse_episodes(EPISODES).unwrap();
        assert_eq!(episodes.len(), 3);
        let first = episodes.last().unwrap();
        assert_eq!(first.name, "Ep. 1 - The Journey's End");
        assert_eq!(first.episode_number, 1.0);
        assert_eq!(first.url, "/watch/bed82e58-72ec-4427-8900-9e6273e5e24d?origin=NIUsb960SxtXls4h");
        assert_eq!(
            episode_api_url(&first.url).unwrap(),
            "https://api.animeparadise.moe/ep/bed82e58-72ec-4427-8900-9e6273e5e24d?origin=NIUsb960SxtXls4h"
        );
    }

    #[test]
    fn the_video_is_the_stream_playlist_with_its_subtitles() {
        let video = parse_video(EPISODE).unwrap();
        assert_eq!(video.video_url, "https://stream.animeparadise.moe/m3u8?url=STREAMLINK");
        assert_eq!(video.subtitle_tracks[0], Track { url: "https://api.animeparadise.moe/stream/file/1ZvDDo79ZE7bc8UFxs0Yn3ShLJP-V_4Ng".into(), lang: "English".into() });
        assert!(video.subtitle_tracks.len() > 1);
    }
}

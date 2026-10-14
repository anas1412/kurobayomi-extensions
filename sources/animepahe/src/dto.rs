//! JSON shapes of the AnimePahe API (`/api?m=airing`, `m=search`, `m=release`), from dto/AnimePaheDto.kt.
//! Field names follow the wire names; the Kotlin property names are kept where they differ.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseDto<T> {
    pub current_page: i64,
    pub last_page: i64,
    #[serde(rename = "data", default = "Vec::new")]
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LatestAnimeDto {
    #[serde(rename = "anime_id")]
    pub id: i64,
    #[serde(rename = "anime_session")]
    pub session: String,
    #[serde(rename = "anime_title")]
    pub title: String,
    pub snapshot: String,
    #[serde(default)]
    #[allow(dead_code)] // the airing list's fansub: not shown
    pub fansub: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResultDto {
    pub id: i64,
    pub title: String,
    pub poster: String,
    pub session: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EpisodeDto {
    pub created_at: String,
    pub session: String,
    #[serde(rename = "episode")]
    pub episode_number: f32,
    #[allow(dead_code)] // the episode's anime: the plug-in already knows it
    pub anime_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airing_page_parses_with_optional_fansub() {
        let json = r#"{"current_page":1,"last_page":3,"data":[{"anime_id":7,"anime_session":"abc","anime_title":"Title","snapshot":"https://img.example/x.jpg","fansub":"SubsPlease"},{"anime_id":8,"anime_session":"def","anime_title":"Other","snapshot":"https://img.example/y.jpg"}]}"#;
        let page: ResponseDto<LatestAnimeDto> = serde_json::from_str(json).unwrap();
        assert_eq!((page.current_page, page.last_page, page.items.len()), (1, 3, 2));
        assert_eq!(page.items[0].session, "abc");
        assert_eq!(page.items[0].fansub.as_deref(), Some("SubsPlease"));
        assert_eq!(page.items[1].fansub, None);
    }

    #[test]
    fn search_and_episode_shapes_parse() {
        let search: ResponseDto<SearchResultDto> =
            serde_json::from_str(r#"{"current_page":1,"last_page":1,"data":[{"id":7,"title":"T","poster":"p","session":"s"}]}"#).unwrap();
        assert_eq!(search.items[0].id, 7);
        let ep: EpisodeDto =
            serde_json::from_str(r#"{"created_at":"2026-01-01 00:00:00","session":"ep1","episode":1.5,"anime_id":7}"#).unwrap();
        assert_eq!((ep.episode_number, ep.anime_id), (1.5, 7));
        let whole: EpisodeDto =
            serde_json::from_str(r#"{"created_at":"x","session":"ep2","episode":2,"anime_id":7}"#).unwrap();
        assert_eq!(whole.episode_number, 2.0);
    }

    #[test]
    fn missing_data_defaults_to_empty() {
        let page: ResponseDto<SearchResultDto> = serde_json::from_str(r#"{"current_page":1,"last_page":1}"#).unwrap();
        assert!(page.items.is_empty());
    }
}

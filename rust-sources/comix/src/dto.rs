//! The API's JSON (`Dto.kt`) and its mapping to the source models, with the Kotlin preference defaults: large posters,
//! score on top, extra info on, alternative names and narrative tags off.

use std::collections::BTreeMap;

use chrono::{DateTime, Months, TimeDelta, Utc};
use kuro_source::model::{SChapter, SManga, manga};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Term {
    pub title: String,
}

#[derive(Deserialize, Default)]
pub struct Poster {
    small: Option<String>,
    medium: Option<String>,
    large: Option<String>,
}

impl Poster {
    fn large(&self) -> Option<String> {
        self.large
            .clone()
            .or_else(|| self.medium.clone())
            .or_else(|| self.small.clone())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manga {
    pub hid: String,
    title: String,
    synopsis: Option<String>,
    #[serde(default, rename = "type")]
    kind: String,
    poster: Option<Poster>,
    #[serde(default)]
    status: String,
    #[serde(default = "safe")]
    content_rating: String,
    authors: Option<Vec<Term>>,
    #[serde(rename = "author")]
    author_old: Option<Vec<Term>>,
    artists: Option<Vec<Term>>,
    #[serde(rename = "artist")]
    artist_old: Option<Vec<Term>>,
    genres: Option<Vec<Term>>,
    #[serde(rename = "genre")]
    genre_old: Option<Vec<Term>>,
    demographics: Option<Vec<Term>>,
    #[serde(rename = "demographic")]
    demographic_old: Option<Vec<Term>>,
    #[serde(default)]
    rated_avg: f64,
    #[serde(default)]
    rated_count: i64,
    #[serde(default)]
    follows_total: i64,
    #[serde(default)]
    rank: i64,
    year: Option<i64>,
    original_language: Option<String>,
    links: Option<BTreeMap<String, Option<String>>>,
    url: Option<String>,
}

fn safe() -> String {
    "safe".to_owned()
}

fn titles(terms: &Option<Vec<Term>>, old: &Option<Vec<Term>>) -> Option<Vec<String>> {
    terms
        .as_ref()
        .or(old.as_ref())
        .map(|t| t.iter().map(|t| t.title.clone()).collect())
}

impl Manga {
    /// `/x0ynk-villains-are-destined-to-die`: the site URL after `/title`, as the Kotlin source stores it.
    pub fn url(&self) -> String {
        match &self.url {
            Some(url) => url
                .split_once("/title")
                .map_or(url.as_str(), |(_, rest)| rest)
                .to_owned(),
            None => format!("/{}", self.hid),
        }
    }

    pub fn to_basic(&self) -> SManga {
        SManga {
            thumbnail_url: self.poster.as_ref().and_then(Poster::large),
            ..SManga::new(self.url(), &self.title)
        }
    }

    pub fn to_details(&self) -> SManga {
        let mut out = self.to_basic();
        out.author = titles(&self.authors, &self.author_old).map(|t| t.join(", "));
        out.artist = titles(&self.artists, &self.artist_old).map(|t| t.join(", "));
        out.description = Some(self.description());
        out.status = match self.status.as_str() {
            "releasing" => manga::ONGOING,
            "on_hiatus" => manga::ON_HIATUS,
            "finished" => manga::COMPLETED,
            "discontinued" => manga::CANCELLED,
            _ => manga::UNKNOWN,
        };
        out.genre = Some(self.genres());
        out.initialized = true;
        out
    }

    fn fancy_score(&self) -> String {
        if self.rated_avg == 0.0 {
            return String::new();
        }
        let stars = ((self.rated_avg / 2.0) + 0.5).floor().clamp(0.0, 5.0) as usize;
        format!(
            "{}{} {}",
            "★".repeat(stars),
            "☆".repeat(5 - stars),
            self.rated_avg
        )
    }

    fn description(&self) -> String {
        let mut out = String::new();
        let score = self.fancy_score();
        if !score.is_empty() {
            out.push_str(&score);
            out.push_str("\n\n");
        }
        out.push_str(self.synopsis.as_deref().unwrap_or(""));
        let mut extras = Vec::new();
        if let Some(year) = self.year.filter(|y| *y > 0) {
            extras.push(format!("**Year**: {year}"));
        }
        if let Some(lang) = self
            .original_language
            .as_deref()
            .filter(|l| !l.trim().is_empty())
        {
            extras.push(format!("**Language**: {}", lang.to_uppercase()));
        }
        if let Some(first) = self.content_rating.chars().next() {
            extras.push(format!(
                "**Content rating**: {}{}",
                first.to_uppercase(),
                &self.content_rating[first.len_utf8()..]
            ));
        }
        if self.rank > 0 {
            extras.push(format!("**Rank**: #{}", self.rank));
        }
        if self.rated_count > 0 {
            extras.push(format!("**Rated by**: {}", self.rated_count));
        }
        if self.follows_total > 0 {
            extras.push(format!("**Followed by**: {}", self.follows_total));
        }
        if !extras.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&extras.join("\n"));
        }
        let trackers = self.tracker_links();
        if !trackers.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str("**Trackers**:\n");
            out.push_str(
                &trackers
                    .iter()
                    .map(|t| format!("- {t}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        out
    }

    fn tracker_links(&self) -> Vec<String> {
        const SITES: [(&str, &str); 5] = [
            ("al", "AniList"),
            ("mal", "MyAnimeList"),
            ("mu", "MangaUpdates"),
            ("md", "MangaDex"),
            ("mb", "MangaBaka"),
        ];
        let Some(links) = &self.links else {
            return Vec::new();
        };
        let known = SITES.iter().filter_map(|(key, name)| {
            links
                .get(*key)?
                .as_deref()
                .filter(|u| !u.trim().is_empty())
                .map(|u| format!("[{name}]({u})"))
        });
        let unknown = links
            .iter()
            .filter(|(k, _)| !SITES.iter().any(|(s, _)| s == k))
            .filter_map(|(k, u)| {
                u.as_deref()
                    .filter(|u| u.starts_with("http"))
                    .map(|u| format!("[{}]({u})", k.to_uppercase()))
            });
        known.chain(unknown).collect()
    }

    fn genres(&self) -> String {
        let mut out = vec![
            match self.kind.as_str() {
                "manhwa" => "Manhwa",
                "manhua" => "Manhua",
                "manga" => "Manga",
                _ => "Other",
            }
            .to_owned(),
        ];
        out.extend(titles(&self.genres, &self.genre_old).unwrap_or_default());
        out.extend(titles(&self.demographics, &self.demographic_old).unwrap_or_default());
        if matches!(self.content_rating.as_str(), "erotica" | "pornographic") {
            out.push("NSFW".to_owned());
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|g| seen.insert(g.clone()));
        out.join(", ")
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(default = "one")]
    page: i64,
    #[serde(default = "one")]
    last_page: i64,
    #[serde(default = "one", rename = "last_page")]
    last_page_old: i64,
}

fn one() -> i64 {
    1
}

#[derive(Deserialize)]
pub struct Items<T> {
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
    meta: Option<Meta>,
    pagination: Option<Meta>,
}

impl<T> Items<T> {
    pub fn has_next_page(&self) -> bool {
        self.meta
            .as_ref()
            .or(self.pagination.as_ref())
            .is_some_and(|m| m.page < m.last_page.max(m.last_page_old))
    }
}

#[derive(Deserialize)]
pub struct Envelope<T> {
    pub result: T,
}

#[derive(Deserialize)]
pub struct Group {
    pub name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub id: i64,
    #[serde(default)]
    pub url: String,
    pub number: f64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    created_at_formatted: String,
    group: Option<Group>,
    #[serde(default)]
    is_official: bool,
}

impl Chapter {
    /// `title/{slug}/{id}-chapter-{n}` with no leading slash, as the Kotlin source stores it.
    pub fn to_chapter(&self, manga_slug: &str, now: DateTime<Utc>) -> SChapter {
        let number = format!("{}", self.number);
        let url = match self.url.find("/title/") {
            Some(i) => self.url[i + 1..].to_owned(),
            None => format!("title/{manga_slug}/{}-chapter-{number}", self.id),
        };
        let name = if self.name.is_empty() {
            format!("Chapter {number}")
        } else {
            format!("Chapter {number}: {}", self.name)
        };
        let scanlator = match (&self.group, self.is_official) {
            (Some(group), _) => group.name.clone(),
            (None, true) => "Official".to_owned(),
            (None, false) => "Unknown".to_owned(),
        };
        SChapter {
            date_upload: relative_date(&self.created_at_formatted, now),
            chapter_number: self.number as f32,
            scanlator: Some(scanlator),
            ..SChapter::new(url, name)
        }
    }
}

/// `"5d ago"` as milliseconds before `now`; 0 when the text is not a relative date.
pub fn relative_date(text: &str, now: DateTime<Utc>) -> i64 {
    let text = text.trim().to_lowercase();
    let text = text.strip_suffix(" ago").unwrap_or(&text);
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let (Ok(amount), unit) = (text[..digits].parse::<i64>(), text[digits..].trim_start()) else {
        return 0;
    };
    let back = |delta: TimeDelta| now - delta;
    let months = |n: i64| {
        u32::try_from(n)
            .ok()
            .and_then(|n| now.checked_sub_months(Months::new(n)))
    };
    let at = match unit {
        "s" | "sec" | "secs" => Some(back(TimeDelta::seconds(amount))),
        "m" | "min" | "mins" => Some(back(TimeDelta::minutes(amount))),
        "h" | "hr" | "hrs" => Some(back(TimeDelta::hours(amount))),
        "d" | "day" | "days" => Some(back(TimeDelta::days(amount))),
        "w" | "week" | "weeks" => Some(back(TimeDelta::weeks(amount))),
        "mo" | "mos" | "month" | "months" => months(amount),
        "y" | "yr" | "yrs" | "year" | "years" => months(amount * 12),
        _ => None,
    };
    at.map_or(0, |t| t.timestamp_millis())
}

#[derive(Deserialize)]
pub struct PageItem {
    pub url: String,
    #[serde(default)]
    pub s: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pages {
    #[serde(default)]
    pub base_url: String,
    pub items: Vec<PageItem>,
    alt: Option<Vec<String>>,
    node: Option<String>,
}

impl Pages {
    /// The hosts the site's reader retries a page on, in its order (`Dto.kt` `altHosts`).
    pub fn alt_hosts(&self, host: &str, index: usize) -> Vec<String> {
        let Some(alt) = self.alt.as_ref().filter(|a| !a.is_empty()) else {
            return Vec::new();
        };
        let label = host.split('.').next().unwrap_or(host);
        let start = index % alt.len();
        let domains: Vec<&String> = alt[start..].iter().chain(&alt[..start]).collect();
        let node = self.node.as_deref().unwrap_or(label);
        let hosts: Vec<String> = domains
            .iter()
            .map(|d| format!("{label}.{d}"))
            .chain(domains.iter().map(|d| format!("{node}.{d}")))
            .collect();
        (0..6).map(|i| hosts[i % hosts.len()].clone()).collect()
    }
}

#[derive(Deserialize)]
pub struct ChapterResult {
    pub pages: Pages,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-10T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn relative_dates_count_back_from_now() {
        let base = now().timestamp_millis();
        assert_eq!(relative_date("5d ago", now()), base - 5 * 86_400_000);
        assert_eq!(relative_date("3w ago", now()), base - 21 * 86_400_000);
        assert_eq!(relative_date("2 hrs ago", now()), base - 2 * 3_600_000);
        assert_eq!(
            relative_date("1mo ago", now()),
            DateTime::parse_from_rfc3339("2026-09-10T12:00:00Z")
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(
            relative_date("2y", now()),
            DateTime::parse_from_rfc3339("2024-10-10T12:00:00Z")
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(relative_date("yesterday", now()), 0);
        assert_eq!(relative_date("", now()), 0);
    }

    /// The first chapter item of the real decrypted response for `x0ynk` (seen in a browser on 2026-10-10).
    #[test]
    fn a_chapter_keeps_the_kotlin_url_name_and_group() {
        let item = r#"{"id":11452041,"mangaId":17667,"number":214,"volume":0,"name":"","language":"en","isOfficial":false,"votes":5,"createdAtFormatted":"5d ago","groupId":9316,"group":{"id":9316,"name":"Eva Scans"},"creator":null,"url":"/title/x0ynk-villains-are-destined-to-die/11452041-chapter-214"}"#;
        let chapter: Chapter = serde_json::from_str(item).unwrap();
        let out = chapter.to_chapter("x0ynk-villains-are-destined-to-die", now());
        assert_eq!(
            out.url,
            "title/x0ynk-villains-are-destined-to-die/11452041-chapter-214"
        );
        assert_eq!(
            (
                out.name.as_str(),
                out.chapter_number,
                out.scanlator.as_deref()
            ),
            ("Chapter 214", 214.0, Some("Eva Scans"))
        );
        assert_eq!(out.date_upload, now().timestamp_millis() - 5 * 86_400_000);

        let bare: Chapter =
            serde_json::from_str(r#"{"id":7,"number":5.5,"name":"Side","isOfficial":true}"#)
                .unwrap();
        let out = bare.to_chapter("abc-slug", now());
        assert_eq!(
            (
                out.url.as_str(),
                out.name.as_str(),
                out.scanlator.as_deref()
            ),
            (
                "title/abc-slug/7-chapter-5.5",
                "Chapter 5.5: Side",
                Some("Official")
            )
        );
    }

    #[test]
    fn alt_hosts_follow_the_reader_order() {
        let pages: Pages =
            serde_json::from_str(r#"{"alt":["a.site","b.site","c.site"],"node":"lov","items":[]}"#)
                .unwrap();
        assert_eq!(
            pages.alt_hosts("8pl.x.site", 1),
            [
                "8pl.b.site",
                "8pl.c.site",
                "8pl.a.site",
                "lov.b.site",
                "lov.c.site",
                "lov.a.site"
            ]
        );
        let none: Pages = serde_json::from_str(r#"{"items":[]}"#).unwrap();
        assert!(none.alt_hosts("8pl.x.site", 0).is_empty());
    }
}

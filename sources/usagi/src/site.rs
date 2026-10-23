//! The Usagi site (<https://web.usagi.one>) as pure functions: URLs and one parser per page. Ported from keiyoushi's
//! GroupLe multisrc (`lib-multisrc/grouple`, Apache-2.0), which `ru/usagi` uses unchanged. Without the filters, the
//! sign-in and the user hash.

use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

pub type Res<T> = Result<T, String>;

pub const BASE: &str = "https://web.usagi.one";
const HOST: &str = "web.usagi.one";

// ============================ Output (wire) ============================

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manga {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// SManga status: 0 unknown, 1 ongoing, 2 completed, 4 publishing finished, 6 on hiatus.
    pub status: i32,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangasPage {
    pub mangas: Vec<Manga>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub url: String,
    pub name: String,
    pub date_upload: i64,
    pub chapter_number: f32,
    pub scanlator: String,
}

// ================================ URLs =================================

/// The catalog API behind popular (`RATING`), latest (`DATE_UPDATE`) and search, 50 titles a page.
pub fn catalog_url(sort: &str, page: i32, query: &str) -> String {
    let mut url = Url::parse(&format!("{BASE}/api/catalog/search")).expect("a valid base");
    url.query_pairs_mut().append_pair("offset", &(50 * (page.max(1) - 1)).to_string());
    if !query.trim().is_empty() {
        url.query_pairs_mut().append_pair("q", query.trim());
    }
    url.query_pairs_mut().append_pair("sortType", sort);
    url.to_string()
}

/// A page of the site from a stored address (`/33450`, `/33450/vol1/41?mtr=true`), refused when it is another site's.
pub fn page(path: &str) -> Res<String> {
    let url = Url::parse(BASE).and_then(|base| base.join(path)).map_err(|e| format!("invalid address {path}: {e}"))?;
    if url.host_str() != Some(HOST) {
        return Err(format!("not a Usagi address: {path}"));
    }
    Ok(url.to_string())
}

/// Whether a final address after redirects is still the site (the Kotlin refuses a chapter that lands elsewhere).
pub fn on_site(url: &str) -> bool {
    Url::parse(url).is_ok_and(|u| u.host_str() == Some(HOST))
}

// =============================== Catalog ===============================

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    total: i64,
    offset: i64,
    limit: i64,
    list: Vec<CatalogItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogItem {
    name: String,
    pic_url: Option<String>,
    element_id: ElementId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ElementId {
    link_name: String,
}

pub fn parse_catalog(body: &str) -> Res<MangasPage> {
    let catalog: Catalog = serde_json::from_str(body).map_err(|e| format!("unexpected Usagi catalog: {e}"))?;
    Ok(MangasPage {
        has_next_page: catalog.offset + catalog.limit < catalog.total,
        mangas: catalog
            .list
            .into_iter()
            .map(|item| Manga {
                url: format!("/{}", item.element_id.link_name),
                title: item.name,
                artist: None,
                author: None,
                description: None,
                genre: None,
                status: 0,
                thumbnail_url: item.pic_url,
            })
            .collect(),
    })
}

// =============================== Parsers ===============================

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("a valid selector")
}

fn text_of(element: ElementRef) -> String {
    element.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_text(scope: ElementRef, css: &str) -> Option<String> {
    scope.select(&sel(css)).next().map(text_of)
}

fn title_of(doc: &Html) -> Option<String> {
    doc.select(&sel(".cr-hero-names__main")).next().map(text_of).or_else(|| {
        doc.select(&sel("meta[itemprop=name]")).next().and_then(|m| m.value().attr("content")).map(str::to_owned)
    })
}

fn status_of(release: &str, translation: &str) -> i32 {
    if release.contains("продолж") || release.contains("начат") {
        1
    } else if release.contains("заверш") {
        if translation.contains("заверш") { 2 } else { 4 }
    } else if release.contains("приост") || release.contains("заморож") {
        6
    } else {
        0
    }
}

fn normalize_age(age: &str) -> &str {
    match age {
        "NC-17" | "R18+" => "18+",
        "R" | "G" | "PG" => "16+",
        "PG-13" => "12+",
        other => other,
    }
}

fn stars(rating: f32) -> &'static str {
    const STEPS: [(f32, &str); 10] = [
        (9.5, "★★★★★"),
        (8.5, "★★★★✬"),
        (7.5, "★★★★☆"),
        (6.5, "★★★✬☆"),
        (5.5, "★★★☆☆"),
        (4.5, "★★✬☆☆"),
        (3.5, "★★☆☆☆"),
        (2.5, "★✬☆☆☆"),
        (1.5, "★☆☆☆☆"),
        (0.5, "✬☆☆☆☆"),
    ];
    STEPS.iter().find(|(min, _)| rating > *min).map_or("☆☆☆☆☆", |(_, s)| s)
}

fn joined(names: Vec<String>) -> Option<String> {
    let mut seen = Vec::new();
    for name in names {
        if !name.is_empty() && !seen.contains(&name) {
            seen.push(name);
        }
    }
    Some(seen.join(", ")).filter(|s| !s.is_empty())
}

pub fn parse_details(html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let title = title_of(&doc).ok_or("the manga page has no title")?;

    // the "Выпуск" (release) and "Перевод" (translation) rows of the info grid, first one of each
    let (mut release, mut translation) = (None, None);
    for item in doc.select(&sel(".cr-hero .cr-info-details > *")) {
        let label = first_text(item, ".cr-info-details-item__title").unwrap_or_default().to_lowercase();
        let value = first_text(item, ".cr-info-details-item__status").unwrap_or_default().to_lowercase();
        let slot = match label.as_str() {
            "выпуск" => &mut release,
            "перевод" => &mut translation,
            _ => continue,
        };
        if slot.is_none() && !value.is_empty() {
            *slot = Some(value);
        }
    }

    let (mut authors, mut artists) = (Vec::new(), Vec::new());
    for person in doc.select(&sel(".cr-main-person-item")) {
        let role = first_text(person, ".cr-main-person-item__role").unwrap_or_default().to_lowercase();
        let names: Vec<String> = person.select(&sel(".cr-main-person-item__name")).map(text_of).collect();
        if role.contains("автор") || role.contains("сценар") {
            authors.extend(names);
        } else if role.contains("худож") || role.contains("иллюст") {
            artists.extend(names);
        }
    }

    let mut genres = Vec::new();
    genres.extend(first_text(root, ".cr-hero-short-details a[href*='/list/category/']"));
    genres.extend(first_text(root, ".cr-hero-short-details a[href*='/list/limitation/']").map(|a| normalize_age(&a).to_owned()));
    genres.extend(
        doc.select(&sel(".creation-element-tags .creation-element-tags__item:not(.creation-element-tags__item--misc) span:not(.text-secondary)"))
            .map(text_of),
    );
    let genre = joined(genres.into_iter().map(|g| g.to_lowercase()).collect());

    let mut description = Vec::new();
    if let Some(rating) = first_text(root, ".cr-hero-rating .cr-hero-rating__value").and_then(|r| r.parse::<f32>().ok()) {
        let votes: String = first_text(root, ".cr-hero-rating__text").unwrap_or_default().chars().filter(char::is_ascii_digit).collect();
        let votes = if votes.is_empty() { "0".to_owned() } else { votes };
        description.push(format!("{} {rating} (голосов: {votes})", stars(rating)));
    }
    description.extend(first_text(root, ".cr-description__content"));
    let mut alt: Vec<String> = Vec::new();
    for name in doc.select(&sel("#alt-names-dialog .modal-body .py-1")).map(text_of) {
        if !name.is_empty() && !alt.contains(&name) {
            alt.push(name);
        }
    }
    if !alt.is_empty() {
        description.push(format!("**Альтернативные названия**:\n{}", alt.iter().map(|a| format!("- {a}")).collect::<Vec<_>>().join("\n")));
    }

    let thumbnail_url = doc.select(&sel(".cr-hero-poster__img")).next().or_else(|| doc.select(&sel(".cr-hero-overlay__bg")).next()).and_then(|img| {
        ["src", "data-src", "data-original", "data-bg"].iter().filter_map(|a| img.value().attr(a)).find(|v| !v.is_empty()).and_then(|v| page_absolute(v))
    });

    Ok(Manga {
        url: String::new(),
        title,
        artist: joined(artists),
        author: joined(authors),
        description: Some(description.join("\n")).filter(|d| !d.is_empty()),
        genre,
        status: status_of(release.as_deref().unwrap_or(""), translation.as_deref().unwrap_or("")),
        thumbnail_url,
    })
}

fn page_absolute(href: &str) -> Option<String> {
    Url::parse(BASE).and_then(|base| base.join(href)).ok().map(String::from)
}

/// The Kotlin `authGuard`: the page hides its content from guests when the `blockedForAnonymous` view setting is on.
fn blocked_for_guests(html: &str) -> bool {
    let Some(at) = html.find("'blockedForAnonymous'").or_else(|| html.find("\"blockedForAnonymous\"")) else {
        return false;
    };
    let end = html[at..].find('}').map_or(html.len(), |e| at + e);
    let setting = &html[at..end];
    let guest = !html.contains("window.current_user_id");
    guest && setting.split("enabled:").nth(1).is_some_and(|v| v.trim_start().starts_with("true"))
}

/// A chapter number as the Kotlin's `DecimalFormat("#,###.##")` prints it with '.' as separator: at most two decimals.
fn number_text(n: f32) -> String {
    let text = format!("{n:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// "12 Экстра" anywhere in the name (the Kotlin `EXTRA_REGEX`).
fn has_numbered_extra(name: &str) -> bool {
    name.match_indices("Экстра").any(|(i, _)| {
        let before = &name[..i];
        let mut chars = before.chars().rev();
        chars.next().is_some_and(char::is_whitespace) && chars.next().is_some_and(|c| c.is_ascii_digit())
    })
}

/// The chapter name as the Kotlin cleans it: long titles lose their own words at the front, a "…" ahead of the
/// numbers is cut, a bare "N Экстра" or "Сингл" gets the chapter number.
fn chapter_name(raw: &str, title: &str, number: f32) -> String {
    let mut name = raw.trim().to_owned();
    if title.chars().count() > 25 {
        for word in title.split(' ') {
            name = name.strip_prefix(word).unwrap_or(&name).trim().to_owned();
        }
    }
    let first_digit = name.find(|c: char| c.is_ascii_digit()).unwrap_or(0);
    if let Some(dots) = name.find('…').filter(|d| *d < first_digit) {
        name = name[dots + '…'.len_utf8()..].trim().to_owned();
    }
    if has_numbered_extra(&name) {
        if name.split_once("Экстра").is_none_or(|(_, after)| after.trim().is_empty()) {
            name = name.replacen(' ', &format!(" - {} ", number_text(number)), 1);
        }
    } else if name.contains("Сингл") && name.split_once("Сингл").is_none_or(|(_, after)| after.trim().is_empty()) {
        name = format!("{} {name}", number_text(number));
    }
    name
}

fn scanlator_of(title: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        return String::new();
    }
    let joined = title.replace("(Переводчик),", "&").replace("Переводчик,", "&");
    let joined = joined.strip_suffix(" (Переводчик)").unwrap_or(&joined);
    joined.strip_suffix(" Переводчик").unwrap_or(joined).to_owned()
}

/// `dd.MM.yy` (or `d.MM.yy`) at midnight UTC, in milliseconds; 0 when it does not parse.
fn date_of(text: &str) -> i64 {
    chrono::NaiveDate::parse_from_str(text.trim(), "%d.%m.%y")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map_or(0, |t| t.and_utc().timestamp_millis())
}

/// The chapters of a manga page, newest first as the site lists them.
pub fn parse_chapters(html: &str) -> Res<Vec<Chapter>> {
    let doc = Html::parse_document(html);
    if doc.select(&sel(".alert.alert-warning")).any(|a| text_of(a).contains("Запрещена публикация произведения по копирайту")) {
        return Err("Лицензировано - Главы удалены по требованию правообладателя.".into());
    }
    if blocked_for_guests(html) {
        return Err("Для просмотра контента необходима авторизация на сайте; Kurobayomi пока не умеет входить на Usagi.".into());
    }
    let title = title_of(&doc).unwrap_or_default();
    let mut chapters = Vec::new();
    for row in doc.select(&sel("tr.item-row")) {
        // the Kotlin skips rows without a link and rows whose date cell is `.text-info` (announced, not out yet)
        if row.select(&sel("td > a")).next().is_none() || row.select(&sel("td.date.text-info")).next().is_some() {
            continue;
        }
        let (Some(link), Some(info)) = (row.select(&sel("a.chapter-link")).next(), row.select(&sel("td.item-title")).next()) else {
            continue;
        };
        let Some(href) = link.value().attr("href") else { continue };
        let number = info.value().attr("data-num").and_then(|n| n.parse::<f32>().ok()).map_or(-1.0, |n| n / 10.0);
        let own_text: String = link.children().filter_map(|c| c.value().as_text().map(|t| t.to_string())).collect();
        let own_text = own_text.split_whitespace().collect::<Vec<_>>().join(" ");
        let path = Url::parse(BASE).and_then(|b| b.join(href)).map(|u| u.path().to_owned()).unwrap_or_else(|_| href.to_owned());
        chapters.push(Chapter {
            url: format!("{path}?mtr=true"),
            name: chapter_name(&own_text, &title, number),
            date_upload: row.select(&sel("td.d-none")).last().map_or(0, |d| date_of(&text_of(d))),
            chapter_number: number,
            scanlator: scanlator_of(link.value().attr("title").unwrap_or("")),
        });
    }
    Ok(chapters)
}

/// The quoted strings of a JavaScript array literal `['a','b',"c",900,...]` starting at `s`, up to its `]`.
fn quoted_strings(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.char_indices();
    while let Some((_, c)) = chars.next() {
        match c {
            ']' => break,
            '\'' | '"' => {
                let mut value = String::new();
                for (_, d) in chars.by_ref() {
                    if d == c {
                        break;
                    }
                    value.push(d);
                }
                out.push(value);
            }
            _ => {}
        }
    }
    out
}

/// The page images of a chapter from the `rm_h.readerInit(chapterInfo, [[host, middle, path, w, h, ...], ...])` call
/// (the Kotlin `PAGES_REGEX` and its address rules).
pub fn parse_pages(html: &str) -> Res<Vec<String>> {
    let doc = Html::parse_document(html);
    if doc.select(&sel("div.alert")).next().is_some() || doc.select(&sel("form.purchase-form")).next().is_some() {
        return Err("Эта глава платная. Используйте сайт, чтобы купить и прочитать ее.".into());
    }
    let script = doc
        .select(&sel("script"))
        .map(|s| s.text().collect::<String>())
        .find(|s| s.contains("chapterInfo"))
        .ok_or("Дизайн сайта обновлен, для дальнейшей работы необходимо обновление дополнения")?;
    let start = ["rm_h.readerInit(", "rm_h.readerDoInit("]
        .iter()
        .find_map(|mark| script.find(mark))
        .ok_or("Дизайн сайта обновлен, для дальнейшей работы необходимо обновление дополнения")?;
    let call = &script[start..script[start..].find(");").map_or(script.len(), |e| start + e)];
    // the page list is the second argument: skip `chapterInfo, [` and read each inner `[...]`
    let list = call.find('[').map_or("", |i| &call[i + 1..]);
    let mut pages = Vec::new();
    let mut rest = list;
    while let Some(open) = rest.find('[') {
        let entry = &rest[open + 1..];
        let fields = quoted_strings(entry);
        rest = entry.find(']').map_or("", |close| &entry[close + 1..]);
        let [host, middle, end, ..] = fields.as_slice() else { continue };
        let mut url = if middle.is_empty() && end.starts_with("/static/") {
            format!("{BASE}{end}")
        } else if middle.ends_with("/manga/") {
            format!("{host}{end}")
        } else {
            format!("{middle}{host}{end}")
        };
        if !url.contains("://") {
            url = format!("https:{url}");
        }
        if url.contains("one-way.work") {
            // a host that needs no token
            url.truncate(url.find('?').unwrap_or(url.len()));
        }
        pages.push(url.replace("//resh", "//h"));
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPULAR: &str = include_str!("../fixtures/popular.json");
    const LATEST: &str = include_str!("../fixtures/latest.json");
    const SEARCH: &str = include_str!("../fixtures/search.json");
    const MANGA: &str = include_str!("../fixtures/manga.html");
    const CHAPTER: &str = include_str!("../fixtures/chapter.html");

    #[test]
    fn the_catalog_gives_cards_and_pages_on() {
        let page = parse_catalog(POPULAR).unwrap();
        assert!(page.has_next_page);
        assert_eq!(page.mangas[0].url, "/podniatie_urovnia_v_odinochku__A3b2ec");
        assert_eq!(page.mangas[0].title, "Поднятие уровня в одиночку");
        assert!(page.mangas[0].thumbnail_url.as_deref().unwrap().starts_with("https://"));
        assert_eq!(parse_catalog(LATEST).unwrap().mangas.len(), 6);
        let search = parse_catalog(SEARCH).unwrap();
        assert!(!search.has_next_page, "8 results fit on one page");
        assert_eq!(search.mangas[0].url, "/nenasytnyi_berserk");
        assert_eq!(
            catalog_url("RATING", 2, " берсерк "),
            "https://web.usagi.one/api/catalog/search?offset=50&q=%D0%B1%D0%B5%D1%80%D1%81%D0%B5%D1%80%D0%BA&sortType=RATING"
        );
        assert_eq!(catalog_url("DATE_UPDATE", 1, ""), "https://web.usagi.one/api/catalog/search?offset=0&sortType=DATE_UPDATE");
    }

    #[test]
    fn the_manga_page_has_title_people_genres_status_summary_and_cover() {
        let manga = parse_details(MANGA).unwrap();
        assert_eq!(manga.title, "Второе прохождение берсерка");
        assert!(manga.author.is_some(), "{manga:?}");
        let genre = manga.genre.unwrap();
        assert!(genre.starts_with("манхва") && genre.contains("боевик") && !genre.contains("в цвете"), "{genre}");
        let description = manga.description.unwrap();
        assert!(description.starts_with("★★★★✬ 8.9 (голосов: 115)"), "{description}");
        assert!(description.contains("**Альтернативные названия**:\n- "), "{description}");
        assert!(manga.thumbnail_url.unwrap().starts_with("https://"));
        assert_ne!(manga.status, 0);
    }

    #[test]
    fn the_chapters_are_listed_newest_first_with_numbers_dates_and_translators() {
        let chapters = parse_chapters(MANGA).unwrap();
        assert_eq!(chapters.len(), 41);
        assert_eq!(chapters[0].url, "/33450/vol1/41?mtr=true");
        assert_eq!(chapters[0].chapter_number, 41.0);
        assert!(chapters[0].name.starts_with("1 - 41"), "{}", chapters[0].name);
        assert!(chapters.iter().all(|c| c.date_upload > 1_500_000_000_000), "every row has a date");
        assert_eq!(chapters.last().unwrap().chapter_number, 1.0);
    }

    #[test]
    fn guests_are_told_when_the_site_hides_a_title() {
        let blocked = "<script>var viewSettings = [{ name: 'blockedForAnonymous', rules: ['hide'], enabled: true && window.is_from_search !== 'yes', onlyGuest: true }];</script>";
        assert!(parse_chapters(blocked).is_err());
        assert!(!blocked_for_guests(&blocked.replace("enabled: true", "enabled: false")));
        assert!(!blocked_for_guests(MANGA));
    }

    #[test]
    fn chapter_names_and_translators_are_cleaned_as_the_kotlin_does() {
        assert_eq!(chapter_name("Очень длинное название этой манги 2 - 15", "Очень длинное название этой манги", 15.0), "2 - 15");
        assert_eq!(chapter_name("… 3 - 7", "x", 7.0), "3 - 7");
        assert_eq!(chapter_name("1 Экстра", "x", 12.5), "1 - 12.5 Экстра");
        assert_eq!(chapter_name("Сингл", "x", 3.0), "3 Сингл");
        assert_eq!(scanlator_of("Dead Inside Team Переводчик"), "Dead Inside Team");
        assert_eq!(scanlator_of("A (Переводчик), B (Переводчик)"), "A & B");
        assert_eq!(date_of("31.08.23"), 1_693_440_000_000);
        assert_eq!(date_of("1.09.23"), 1_693_526_400_000);
    }

    #[test]
    fn the_chapter_page_gives_its_images_in_order_without_tokens() {
        let pages = parse_pages(CHAPTER).unwrap();
        assert!(pages.len() >= 5, "{pages:?}");
        assert_eq!(pages[0], "https://rm.one-way.work/auto/126/35/18/1.jpg");
        assert!(pages[1].ends_with("/2.jpg"));
        let deleted = r#"<script>var chapterInfo = {}; rm_h.readerInit(chapterInfo, [['','',"/static/deleted1.png?t=1",600,800, '']], false);</script>"#;
        assert_eq!(parse_pages(deleted).unwrap(), ["https://web.usagi.one/static/deleted1.png?t=1"]);
        let other = r#"<script>var chapterInfo = {}; rm_h.readerDoInit(chapterInfo, [['//h1.example/','https:',"x/1.png?t=2",1,1]]);</script>"#;
        assert_eq!(parse_pages(other).unwrap(), ["https://h1.example/x/1.png?t=2"]);
    }

    #[test]
    fn only_addresses_of_the_site_are_fetched() {
        assert_eq!(page("/33450/vol1/41?mtr=true").unwrap(), "https://web.usagi.one/33450/vol1/41?mtr=true");
        assert!(page("https://evil.test/x").is_err());
        assert!(on_site("https://web.usagi.one/33450") && !on_site("https://readmanga.io/x"));
        assert_eq!(status_of("завершён", "продолжается"), 4);
    }
}

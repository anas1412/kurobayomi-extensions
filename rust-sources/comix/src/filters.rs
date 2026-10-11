//! The search filters (`Filters.kt`) with the Kotlin preference defaults: content up to "suggestive", every type and
//! demographic, no blocked genre. Tags, author and artist (free text the Kotlin source resolves to ids with extra
//! requests) are left out.

use chrono::{Datelike, Utc};
use kuro_source::{Filter, FilterList, SortSelection, SourceError, TriStateState};

use crate::api::Params;

const CONTENT_RATINGS: [(&str, &str); 5] = [
    ("Show all", ""),
    ("Safe only", "safe"),
    ("Up to Suggestive", "suggestive"),
    ("Up to Erotica", "erotica"),
    ("Up to Pornographic", "pornographic"),
];
const DEFAULT_CONTENT_RATING: &str = "suggestive";
const TYPES: [(&str, &str); 4] = [
    ("Manga", "manga"),
    ("Manhwa", "manhwa"),
    ("Manhua", "manhua"),
    ("Other", "other"),
];
const DEMOGRAPHICS: [(&str, &str); 4] = [
    ("Josei", "3"),
    ("Seinen", "4"),
    ("Shoujo", "1"),
    ("Shounen", "2"),
];
const STATUSES: [(&str, &str); 5] = [
    ("Releasing", "releasing"),
    ("Finished", "finished"),
    ("On Hiatus", "on_hiatus"),
    ("Discontinued", "discontinued"),
    ("Not Yet Released", "not_yet_released"),
];
const GENRES: [(&str, &str); 31] = [
    ("Action", "6"),
    ("Adult", "87264"),
    ("Adventure", "7"),
    ("Boys Love", "8"),
    ("Comedy", "9"),
    ("Crime", "10"),
    ("Drama", "11"),
    ("Ecchi", "87265"),
    ("Fantasy", "12"),
    ("Girls Love", "13"),
    ("Harem", "40"),
    ("Hentai", "87266"),
    ("Historical", "14"),
    ("Horror", "15"),
    ("Isekai", "16"),
    ("Magical Girls", "17"),
    ("Mature", "87267"),
    ("Mecha", "18"),
    ("Medical", "19"),
    ("Mystery", "20"),
    ("Philosophical", "21"),
    ("Psychological", "22"),
    ("Romance", "23"),
    ("Sci-Fi", "24"),
    ("Slice of Life", "25"),
    ("Smut", "87268"),
    ("Sports", "26"),
    ("Superhero", "27"),
    ("Thriller", "28"),
    ("Tragedy", "29"),
    ("Wuxia", "30"),
];
const FORMATS: [(&str, &str); 9] = [
    ("4-Koma", "93164"),
    ("Adaptation", "93167"),
    ("Anthology", "93165"),
    ("Award Winning", "93166"),
    ("Doujinshi", "93168"),
    ("Full Color", "93172"),
    ("Long Strip", "93170"),
    ("Oneshot", "93169"),
    ("Web Comic", "93171"),
];
const SORTS: [(&str, &str); 11] = [
    ("Relevance", "relevance"),
    ("Latest update", "chapter_updated_at"),
    ("Recently added", "created_at"),
    ("Title", "title"),
    ("Year", "year"),
    ("Highest rated", "score"),
    ("Most viewed · 7 days", "views_7d"),
    ("Most viewed · 30 days", "views_30d"),
    ("Most viewed · 90 days", "views_90d"),
    ("Most viewed · all time", "views_total"),
    ("Most followed", "follows_total"),
];
const MATCH: [(&str, &str); 2] = [("All (AND)", "and"), ("Any (OR)", "or")];
const OLDEST_YEAR: i32 = 1928;

fn names(pairs: &[(&str, &str)]) -> Vec<String> {
    pairs.iter().map(|(n, _)| (*n).to_owned()).collect()
}

fn checks(pairs: &[(&str, &str)], on: bool) -> Vec<Filter> {
    pairs
        .iter()
        .map(|(n, _)| Filter::CheckBox {
            name: (*n).to_owned(),
            state: on,
        })
        .collect()
}

/// The years newest first (next year down to 1928); the "From" list ends with "Any", the "To" list starts with it.
fn years() -> Vec<String> {
    (OLDEST_YEAR..=Utc::now().year() + 1)
        .rev()
        .map(|y| y.to_string())
        .collect()
}

pub fn filter_list() -> FilterList {
    let rating = CONTENT_RATINGS
        .iter()
        .position(|(_, v)| *v == DEFAULT_CONTENT_RATING)
        .unwrap_or(0) as i32;
    let from = [years(), vec!["Any".to_owned()]].concat();
    let from_any = from.len() as i32 - 1;
    let to = [vec!["Any".to_owned()], years()].concat();
    FilterList(vec![
        Filter::Sort {
            name: "Sort By".into(),
            values: names(&SORTS),
            state: Some(SortSelection {
                index: 0,
                ascending: false,
            }),
        },
        Filter::select_at("Content rating", names(&CONTENT_RATINGS), rating),
        Filter::group("Type", checks(&TYPES, true)),
        Filter::separator(""),
        Filter::header("Match: AND requires every selection, OR matches any"),
        Filter::select("Match", names(&MATCH)),
        Filter::group(
            "Genres",
            GENRES.iter().map(|(n, _)| Filter::tri_state(*n)).collect(),
        ),
        Filter::group(
            "Formats",
            FORMATS.iter().map(|(n, _)| Filter::tri_state(*n)).collect(),
        ),
        Filter::separator(""),
        Filter::group("Demographic", checks(&DEMOGRAPHICS, true)),
        Filter::group("Status", checks(&STATUSES, false)),
        Filter::text("Minimum Chapter Length"),
        Filter::separator(""),
        Filter::header("Release Year"),
        Filter::select_at("From", from, from_any),
        Filter::select("To", to),
    ])
}

fn add(params: &mut Params, name: &str, value: impl Into<String>) {
    params
        .entry(name.to_owned())
        .or_default()
        .push(value.into());
}

fn find<'a>(filters: &'a FilterList, name: &str) -> Option<&'a Filter> {
    filters.0.iter().find(|f| f.name() == name)
}

fn content_ratings_up_to(max: &str) -> Vec<&'static str> {
    let all = ["safe", "suggestive", "erotica", "pornographic"];
    all.iter()
        .position(|r| *r == max)
        .map_or(Vec::new(), |i| all[..=i].to_vec())
}

/// The parameters the preference filters add to every listing (Kotlin `applyPreferenceFilters` with the defaults).
pub fn preference_params(params: &mut Params) {
    for rating in content_ratings_up_to(DEFAULT_CONTENT_RATING) {
        add(params, "content_rating[]", rating);
    }
}

/// Kotlin `getSearchMangaList` without the tag, author and artist lookups. An empty list means the defaults.
pub fn search_params(filters: &FilterList, query: &str) -> Result<Params, SourceError> {
    let defaults;
    let filters = if filters.0.is_empty() {
        defaults = filter_list();
        &defaults
    } else {
        filters
    };
    let mut params = Params::new();
    if let Some(Filter::Sort {
        state: Some(sel), ..
    }) = find(filters, "Sort By")
    {
        let selected = usize::try_from(sel.index)
            .ok()
            .and_then(|i| SORTS.get(i))
            .map_or("relevance", |(_, v)| v);
        let order = if selected == "relevance" && query.trim().is_empty() {
            "chapter_updated_at"
        } else {
            selected
        };
        add(
            &mut params,
            &format!("order[{order}]"),
            if sel.ascending { "asc" } else { "desc" },
        );
    }
    if let Some(Filter::Select { state, .. }) = find(filters, "Content rating") {
        let max = usize::try_from(*state)
            .ok()
            .and_then(|i| CONTENT_RATINGS.get(i))
            .map_or("", |(_, v)| v);
        for rating in content_ratings_up_to(max) {
            add(&mut params, "content_rating[]", rating);
        }
    }
    let mut term_selected = false;
    for (group, options) in [("Genres", &GENRES[..]), ("Formats", &FORMATS[..])] {
        if let Some(Filter::Group { state, .. }) = find(filters, group) {
            for (filter, (_, id)) in state.iter().zip(options) {
                match filter {
                    Filter::TriState {
                        state: TriStateState::Include,
                        ..
                    } => add(&mut params, "genres_in[]", *id),
                    Filter::TriState {
                        state: TriStateState::Exclude,
                        ..
                    } => add(&mut params, "genres_ex[]", *id),
                    _ => continue,
                }
                term_selected = true;
            }
        }
    }
    if term_selected && let Some(value) = find(filters, "Match").and_then(Filter::select_value) {
        let id = MATCH
            .iter()
            .find(|(n, _)| *n == value)
            .map_or("and", |(_, v)| v);
        add(&mut params, "genres_mode", id);
    }
    for (group, options, param, only_if_some) in [
        ("Type", &TYPES[..], "types[]", true),
        ("Demographic", &DEMOGRAPHICS[..], "demographics[]", true),
        ("Status", &STATUSES[..], "statuses[]", false),
    ] {
        if let Some(Filter::Group { state, .. }) = find(filters, group) {
            let checked: Vec<&str> = state
                .iter()
                .zip(options)
                .filter(|(f, _)| matches!(f, Filter::CheckBox { state: true, .. }))
                .map(|(_, (_, v))| *v)
                .collect();
            if only_if_some && checked.len() == options.len() {
                continue;
            }
            for value in checked {
                add(&mut params, param, value);
            }
        }
    }
    if let Some(Filter::Text { state, .. }) = find(filters, "Minimum Chapter Length")
        && !state.is_empty()
    {
        let value = state
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|v| *v > 0)
            .ok_or_else(|| {
                SourceError::Url(
                    "Minimum chapter length must be a positive integer greater than 0".into(),
                )
            })?;
        add(&mut params, "min_chap", value.to_string());
    }
    if let Some(year) = find(filters, "From")
        .and_then(Filter::select_value)
        .filter(|v| *v != "Any")
    {
        add(&mut params, "year_from", year);
    }
    if let Some(Filter::Select { state, .. }) = find(filters, "To")
        && *state > 0
        && let Some(year) = find(filters, "To").and_then(Filter::select_value)
    {
        add(&mut params, "year_to", year);
    }
    if !query.trim().is_empty() {
        add(&mut params, "keyword", query.trim());
    }
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::canonical;

    fn text(params: &Params) -> String {
        canonical(params)
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    #[test]
    fn default_filters_give_relevance_or_latest_and_the_default_rating() {
        let p = search_params(&FilterList::default(), "villains").unwrap();
        assert_eq!(
            text(&p),
            "content_rating[0]=safe&content_rating[1]=suggestive&keyword=villains&order[relevance]=desc"
        );
        let p = search_params(&filter_list(), " ").unwrap();
        assert_eq!(
            text(&p),
            "content_rating[0]=safe&content_rating[1]=suggestive&order[chapter_updated_at]=desc"
        );
    }

    #[test]
    fn chosen_filters_become_parameters() {
        let mut list = filter_list();
        for f in &mut list.0 {
            match f {
                Filter::Sort { state, .. } => {
                    *state = Some(SortSelection {
                        index: 5,
                        ascending: true,
                    })
                }
                Filter::Select { name, state, .. } if name == "Content rating" => *state = 0,
                Filter::Select { name, state, .. } if name == "Match" => *state = 1,
                Filter::Select { name, state, .. } if name == "From" => *state = 2,
                Filter::Group { name, state } if name == "Genres" => {
                    state[0] = Filter::TriState {
                        name: "Action".into(),
                        state: TriStateState::Include,
                    };
                    state[7] = Filter::TriState {
                        name: "Ecchi".into(),
                        state: TriStateState::Exclude,
                    };
                }
                Filter::Group { name, state } if name == "Type" => {
                    state[1] = Filter::CheckBox {
                        name: "Manhwa".into(),
                        state: false,
                    }
                }
                Filter::Group { name, state } if name == "Status" => {
                    state[1] = Filter::CheckBox {
                        name: "Finished".into(),
                        state: true,
                    }
                }
                Filter::Text { state, .. } => *state = "10".into(),
                _ => {}
            }
        }
        let year = (Utc::now().year() - 1).to_string();
        let p = search_params(&list, "").unwrap();
        assert_eq!(
            text(&p),
            format!(
                "genres_ex[0]=87265&genres_in[0]=6&genres_mode=or&min_chap=10&order[score]=asc&statuses[0]=finished&types[0]=manga&types[1]=manhua&types[2]=other&year_from={year}"
            )
        );
    }

    #[test]
    fn match_mode_needs_a_genre_and_a_bad_minimum_is_an_error() {
        let mut list = filter_list();
        for f in &mut list.0 {
            match f {
                Filter::Select { name, state, .. } if name == "Match" => *state = 1,
                Filter::Text { state, .. } => *state = "0".into(),
                _ => {}
            }
        }
        assert!(search_params(&list, "x").is_err());
        if let Some(Filter::Text { state, .. }) = list
            .0
            .iter_mut()
            .find(|f| f.name() == "Minimum Chapter Length")
        {
            state.clear();
        }
        assert!(!text(&search_params(&list, "x").unwrap()).contains("genres_mode"));
    }
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;
use crate::cache;
use crate::data;

const DIRECTORY_URL: &str = "https://itunes.apple.com/search";
const LOOKUP_URL: &str = "https://itunes.apple.com/lookup";
const MAX_FEED_BYTES: u64 = 32 * 1024 * 1024;
const DIRECTORY_CACHE_AGE: Duration = Duration::from_secs(30 * 60);
const FEED_CACHE_AGE: Duration = Duration::from_secs(15 * 60);
const FEED_CACHE_KIND: &str = "feeds-v2";
const OLD_FEED_CACHE_KIND: &str = "feeds-v1";
pub(crate) const USER_AGENT: &str = "ElitePodcasts/0.1 (io.github.elite.Podcasts; native GNOME podcast app)";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Show {
    pub title: String,
    pub author: String,
    pub feed_url: String,
    pub artwork_urls: Vec<String>,
    pub updated: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub title: String,
    pub date: String,
    pub description: String,
    pub audio_url: Option<String>,
    pub duration_seconds: Option<u64>,
    pub artwork_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FeedDetails {
    pub title: String,
    pub description: String,
    pub website: Option<String>,
    pub artwork_url: Option<String>,
    pub all_episode_ids: Vec<String>,
    pub episodes: Vec<Episode>,
}

#[derive(Deserialize)]
struct DirectoryResponse {
    #[serde(default)]
    results: Vec<DirectoryFeed>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryFeed {
    collection_id: Option<u64>,
    collection_name: Option<String>,
    artist_name: Option<String>,
    feed_url: Option<String>,
    artwork_url_600: Option<String>,
    artwork_url_100: Option<String>,
    release_date: Option<String>,
}

#[derive(Deserialize)]
struct ChartResponse {
    feed: ChartFeed,
}

#[derive(Deserialize)]
struct ChartFeed {
    #[serde(default)]
    entry: Vec<ChartEntry>,
}

#[derive(Deserialize)]
struct ChartEntry {
    id: ChartId,
}

#[derive(Deserialize)]
struct ChartId {
    attributes: ChartIdAttributes,
}

#[derive(Deserialize)]
struct ChartIdAttributes {
    #[serde(rename = "im:id")]
    id: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()
}

fn request_error(service: &str, error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("{service} returned HTTP {code}."),
        ureq::Error::Transport(error) => format!("Could not reach {service}: {error}"),
    }
}

pub fn search(term: &str) -> Result<Vec<Show>, String> {
    let query = term.trim().to_lowercase();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(cached) = cache::read_json("apple-search-v1", &query, Some(DIRECTORY_CACHE_AGE)) {
        return Ok(cached);
    }
    let mut shows = match directory_request(&query) {
        Ok(shows) => shows,
        Err(error) => return cache::read_json("apple-search-v1", &query, None).ok_or(error),
    };
    let words: Vec<&str> = query.split_whitespace().collect();
    shows.sort_by_key(|show| {
        let title = show.title.to_lowercase();
        let author = show.author.to_lowercase();
        if title == query {
            0
        } else if title.starts_with(&query) {
            1
        } else if title.contains(&query) {
            2
        } else if !words.is_empty() && words.iter().all(|word| title.contains(word)) {
            3
        } else if author.contains(&query) {
            4
        } else {
            5
        }
    });
    cache::write_json("apple-search-v1", &query, &shows);
    Ok(shows)
}

fn directory_request(term: &str) -> Result<Vec<Show>, String> {
    let encoded = percent_encoding::utf8_percent_encode(term, percent_encoding::NON_ALPHANUMERIC);
    let url = format!("{DIRECTORY_URL}?term={encoded}&country=us&media=podcast&entity=podcast&limit=30");
    let response: DirectoryResponse = agent()
        .get(&url)
        .call()
        .map_err(|error| request_error("The podcast directory", error))?
        .into_json()
        .map_err(|error| format!("Could not read podcast directory results: {error}"))?;
    Ok(response.results.into_iter().filter_map(|feed| show_from_directory(feed).map(|(_, show)| show)).collect())
}

fn show_from_directory(feed: DirectoryFeed) -> Option<(Option<u64>, Show)> {
    let title = feed.collection_name?.trim().to_string();
    let feed_url = feed.feed_url?.trim().to_string();
    if title.is_empty() || !usable_feed_url(&feed_url) {
        return None;
    }
    Some((feed.collection_id, Show {
        title,
        author: feed.artist_name.unwrap_or_default(),
        feed_url,
        artwork_urls: artwork_candidates([feed.artwork_url_600, feed.artwork_url_100]),
        updated: feed.release_date,
    }))
}

fn usable_feed_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

fn artwork_candidates(urls: impl IntoIterator<Item = Option<String>>) -> Vec<String> {
    let mut candidates = Vec::new();
    for url in urls.into_iter().flatten() {
        let url = url.trim();
        if usable_feed_url(url) && !candidates.iter().any(|candidate| candidate == url) {
            candidates.push(url.to_string());
        }
    }
    candidates
}

pub fn category(parent: &str, selected: &str) -> Result<Vec<Show>, String> {
    let genre = data::category_genre(parent, selected)
        .ok_or_else(|| "This category is unavailable.".to_string())?;
    let cache_key = genre.to_string();
    if let Some(cached) = cache::read_json("apple-chart-category-v1", &cache_key, Some(DIRECTORY_CACHE_AGE)) {
        return Ok(cached);
    }
    match category_request(genre) {
        Ok(shows) => {
            cache::write_json("apple-chart-category-v1", &cache_key, &shows);
            Ok(shows)
        }
        Err(error) => cache::read_json("apple-chart-category-v1", &cache_key, None).ok_or(error),
    }
}

fn category_request(genre: u32) -> Result<Vec<Show>, String> {
    let chart_url = format!("https://itunes.apple.com/us/rss/toppodcasts/limit=30/genre={genre}/json");
    let chart: ChartResponse = agent()
        .get(&chart_url)
        .call()
        .map_err(|error| request_error("Apple's podcast chart", error))?
        .into_json()
        .map_err(|error| format!("Could not read Apple's podcast chart: {error}"))?;
    let ids: Vec<u64> = chart.feed.entry.into_iter()
        .filter_map(|entry| entry.id.attributes.id.parse().ok())
        .collect();
    if ids.is_empty() {
        return Err("This category has no available shows in Apple's chart.".into());
    }
    let id_list = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
    let lookup_url = format!("{LOOKUP_URL}?id={id_list}&country=us");
    let response: DirectoryResponse = agent()
        .get(&lookup_url)
        .call()
        .map_err(|error| request_error("Apple's podcast directory", error))?
        .into_json()
        .map_err(|error| format!("Could not read podcast details: {error}"))?;
    let mut by_id: HashMap<u64, Show> = response.results.into_iter()
        .filter_map(show_from_directory)
        .filter_map(|(id, show)| id.map(|id| (id, show)))
        .collect();
    let shows: Vec<Show> = ids.into_iter().filter_map(|id| by_id.remove(&id)).collect();
    if shows.is_empty() {
        Err("This category's shows have no public RSS feeds.".into())
    } else {
        Ok(shows)
    }
}

pub fn feed(url: &str) -> Result<FeedDetails, String> {
    if let Some(cached) = cache::read_json(FEED_CACHE_KIND, url, Some(FEED_CACHE_AGE)) {
        return Ok(cached);
    }
    if let Some(cached) = cache::read_json(FEED_CACHE_KIND, url, None) {
        let url = url.to_string();
        std::thread::spawn(move || {
            let _ = feed_fresh(&url);
        });
        return Ok(cached);
    }
    feed_fresh(url).or_else(|error| cache::read_json(OLD_FEED_CACHE_KIND, url, None).ok_or(error))
}

pub fn cached_feed(url: &str) -> Option<FeedDetails> {
    cache::read_json(FEED_CACHE_KIND, url, None)
        .or_else(|| cache::read_json(OLD_FEED_CACHE_KIND, url, None))
}

pub fn feed_fresh(url: &str) -> Result<FeedDetails, String> {
    let details = fetch_feed(url)?;
    cache::write_json(FEED_CACHE_KIND, url, &details);
    Ok(details)
}

fn fetch_feed(url: &str) -> Result<FeedDetails, String> {
    if !usable_feed_url(url) {
        return Err("This show does not have a usable RSS URL.".into());
    }
    let response = agent()
        .get(url)
        .call()
        .map_err(|error| request_error("The podcast host", error))?;
    let mut xml_bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_FEED_BYTES + 1)
        .read_to_end(&mut xml_bytes)
        .map_err(|error| format!("Could not read the RSS feed: {error}"))?;
    if xml_bytes.len() as u64 > MAX_FEED_BYTES {
        return Err("This podcast's RSS feed is larger than the 32 MB limit.".to_string());
    }
    let xml = String::from_utf8(xml_bytes)
        .map_err(|_| "This podcast's RSS feed is not encoded as UTF-8.".to_string())?;
    let document = roxmltree::Document::parse(&xml)
        .map_err(|_| "This podcast's feed did not return valid RSS/XML.".to_string())?;
    let channel = document
        .descendants()
        .find(|node| node.has_tag_name("channel"))
        .ok_or_else(|| "This feed does not contain an RSS channel.".to_string())?;
    let field = |node: roxmltree::Node<'_, '_>, name: &str| -> String {
        node.children()
            .find(|child| child.is_element() && child.tag_name().name() == name)
            .and_then(|child| child.text())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let all_episodes: Vec<Episode> = channel
        .children()
        .filter(|node| node.has_tag_name("item"))
        .map(|node| {
            let title = field(node, "title");
            let date = field(node, "pubDate");
            let guid = field(node, "guid");
            let audio_source = node
                .children()
                .filter(|child| child.is_element() && child.tag_name().name() == "enclosure")
                .find_map(|child| {
                    let url = child.attribute("url")?.trim();
                    let kind = child.attribute("type").unwrap_or_default();
                    (usable_feed_url(url)
                        && (kind.is_empty()
                            || kind.starts_with("audio/")
                            || matches!(
                                kind,
                                "application/octet-stream"
                                    | "application/x-mpegURL"
                                    | "application/vnd.apple.mpegurl"
                            )))
                        .then(|| url.to_string())
                });
            let audio_url = audio_source;
            let artwork_url = episode_artwork(node);
            Episode {
                id: if guid.is_empty() {
                    format!("{title}\u{1f}{date}")
                } else {
                    guid
                },
                title,
                date,
                description: {
                    let description = field(node, "description");
                    if description.is_empty() { field(node, "encoded") } else { description }
                },
                audio_url,
                duration_seconds: parse_duration(&field(node, "duration")),
                artwork_url,
            }
        })
        .filter(|episode| !episode.title.is_empty())
        .collect();
    let all_episode_ids = all_episodes.iter().map(|episode| episode.id.clone()).collect();
    let episodes = all_episodes.into_iter().take(20).collect();
    let artwork_url = channel
        .children()
        .find_map(|node| {
            if node.is_element() && node.tag_name().name() == "image" {
                node.attribute("href")
                    .map(str::to_string)
                    .or_else(|| {
                        node.children()
                            .find(|child| child.has_tag_name("url"))
                            .and_then(|child| child.text())
                            .map(str::to_string)
                    })
            } else {
                None
            }
        });
    Ok(FeedDetails {
        title: field(channel, "title"),
        description: field(channel, "description"),
        website: Some(field(channel, "link")).filter(|link| usable_feed_url(link)),
        artwork_url,
        all_episode_ids,
        episodes,
    })
}

fn episode_artwork(item: roxmltree::Node<'_, '_>) -> Option<String> {
    let image_url = |node: roxmltree::Node<'_, '_>| {
        node.attribute("href")
            .or_else(|| node.attribute("url"))
            .or_else(|| {
                node.children()
                    .find(|child| child.is_element() && child.tag_name().name() == "url")
                    .and_then(|child| child.text())
            })
            .map(str::trim)
            .filter(|url| usable_feed_url(url))
            .map(str::to_string)
    };

    // Prefer item-level artwork, then images within Media RSS groups.
    item.children()
        .filter(|node| node.is_element())
        .filter(|node| matches!(node.tag_name().name(), "image" | "thumbnail"))
        .find_map(image_url)
        .or_else(|| {
            item.descendants()
                .skip(1)
                .filter(|node| node.is_element() && node.tag_name().name() == "thumbnail")
                .find_map(image_url)
        })
        .or_else(|| {
            item.descendants()
                .skip(1)
                .filter(|node| {
                    node.is_element()
                        && node.tag_name().name() == "content"
                        && (node.attribute("medium") == Some("image")
                            || node.attribute("type").is_some_and(|kind| kind.starts_with("image/")))
                })
                .find_map(image_url)
        })
        .or_else(|| {
            item.children()
                .filter(|node| node.is_element() && node.tag_name().name() == "images")
                .filter_map(|node| node.attribute("srcset"))
                .flat_map(|srcset| srcset.split(','))
                .filter_map(|entry| {
                    let mut parts = entry.split_whitespace();
                    let url = parts.next()?;
                    let width = parts.next()?.strip_suffix('w')?.parse::<u32>().ok()?;
                    usable_feed_url(url).then(|| (width, url.to_string()))
                })
                .max_by_key(|(width, _)| *width)
                .map(|(_, url)| url)
        })
}

fn parse_duration(raw: &str) -> Option<u64> {
    if raw.is_empty() {
        return None;
    }
    raw.split(':').try_fold(0u64, |total, part| {
        total.checked_mul(60)?.checked_add(part.trim().parse::<u64>().ok()?)
    })
}

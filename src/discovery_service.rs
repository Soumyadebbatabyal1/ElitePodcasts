use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PODCAST_INDEX_URL: &str = "https://api.podcastindex.org";
const FYYD_SEARCH_URL: &str = "https://api.fyyd.de/0.2/search/podcast";
const APPLE_SEARCH_URL: &str = "https://itunes.apple.com/search";
const MAX_FEED_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) const USER_AGENT: &str = "ElitePodcasts/0.1 (io.github.elite.Podcasts; native GNOME podcast app)";

#[derive(Clone, Debug)]
pub struct Show {
    pub title: String,
    pub author: String,
    pub feed_url: String,
    pub artwork_urls: Vec<String>,
    pub updated: Option<String>,
}

#[derive(Clone)]
pub struct Episode {
    pub id: String,
    pub title: String,
    pub date: String,
    pub description: String,
    pub audio_url: Option<String>,
    pub duration_seconds: Option<u64>,
    pub artwork_url: Option<String>,
}

#[derive(Clone)]
pub struct FeedDetails {
    pub title: String,
    pub description: String,
    pub website: Option<String>,
    pub artwork_url: Option<String>,
    pub all_episode_ids: Vec<String>,
    pub episodes: Vec<Episode>,
}

#[derive(Deserialize)]
struct AppleSearchResponse {
    #[serde(default)]
    results: Vec<AppleSearchFeed>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppleSearchFeed {
    collection_name: Option<String>,
    artist_name: Option<String>,
    feed_url: Option<String>,
    artwork_url600: Option<String>,
    artwork_url100: Option<String>,
    release_date: Option<String>,
}

#[derive(Deserialize)]
struct FyydSearchResponse {
    #[serde(default)]
    data: Vec<FyydFeed>,
}

#[derive(Deserialize)]
struct FyydFeed {
    title: Option<String>,
    author: Option<String>,
    #[serde(rename = "xmlURL")]
    feed_url: Option<String>,
    #[serde(rename = "layoutImageURL")]
    layout_image_url: Option<String>,
    #[serde(rename = "thumbImageURL")]
    thumb_image_url: Option<String>,
    #[serde(rename = "smallImageURL")]
    small_image_url: Option<String>,
    #[serde(rename = "imgURL")]
    image_url: Option<String>,
    status: Option<u16>,
    lastpub: Option<String>,
}

#[derive(Deserialize)]
struct TrendingResponse {
    #[serde(default)]
    feeds: Vec<TrendingFeed>,
}

#[derive(Deserialize)]
struct TrendingFeed {
    title: Option<String>,
    author: Option<String>,
    url: Option<String>,
    image: Option<String>,
    artwork: Option<String>,
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
    let (fyyd, apple) = std::thread::scope(|scope| {
        let fyyd = scope.spawn(|| search_fyyd(term));
        let apple = scope.spawn(|| search_apple(term));
        (
            fyyd.join().unwrap_or_else(|_| Err("fyyd search stopped unexpectedly.".into())),
            apple.join().unwrap_or_else(|_| Err("Apple search stopped unexpectedly.".into())),
        )
    });
    if let (Err(fyyd_error), Err(apple_error)) = (&fyyd, &apple) {
        return Err(format!(
            "Both podcast directories failed. fyyd: {fyyd_error} Apple: {apple_error}"
        ));
    }
    let mut shows = fyyd.unwrap_or_default();
    for show in apple.unwrap_or_default() {
        if let Some(existing) = shows.iter_mut().find(|existing: &&mut Show| {
            existing.feed_url == show.feed_url
                || (existing.title.eq_ignore_ascii_case(&show.title)
                    && existing.author.eq_ignore_ascii_case(&show.author)
                    && !existing.author.is_empty())
        }) {
            let mut artwork_urls = show.artwork_urls;
            artwork_urls.append(&mut existing.artwork_urls);
            existing.artwork_urls = artwork_candidates(artwork_urls.into_iter().map(Some));
            if existing.updated.is_none() {
                existing.updated = show.updated;
            }
        } else {
            shows.push(show);
        }
    }
    let query = term.trim().to_lowercase();
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
    Ok(shows)
}

fn search_fyyd(term: &str) -> Result<Vec<Show>, String> {
    let query = percent_encoding::utf8_percent_encode(term, percent_encoding::NON_ALPHANUMERIC);
    let url = format!("{FYYD_SEARCH_URL}?term={query}&count=15");
    let response: FyydSearchResponse = agent()
        .get(&url)
        .call()
        .map_err(|error| request_error("fyyd", error))?
        .into_json()
        .map_err(|error| format!("Could not read fyyd search results: {error}"))?;

    Ok(response
        .data
        .into_iter()
        .filter_map(|feed| {
            if feed.status.is_some_and(|status| status >= 400) {
                return None;
            }
            let title = feed.title?.trim().to_string();
            let feed_url = feed.feed_url?.trim().to_string();
            if title.is_empty() || !usable_feed_url(&feed_url) {
                return None;
            }
            Some(Show {
                title,
                author: feed.author.unwrap_or_default(),
                feed_url,
                artwork_urls: artwork_candidates([
                    feed.image_url,
                    feed.layout_image_url,
                    feed.small_image_url,
                    feed.thumb_image_url,
                ]),
                updated: feed.lastpub,
            })
        })
        .collect())
}

fn search_apple(term: &str) -> Result<Vec<Show>, String> {
    let query = percent_encoding::utf8_percent_encode(term, percent_encoding::NON_ALPHANUMERIC);
    let url = format!("{APPLE_SEARCH_URL}?term={query}&media=podcast&entity=podcast&limit=15");
    let response: AppleSearchResponse = agent()
        .get(&url)
        .call()
        .map_err(|error| request_error("Apple Podcasts", error))?
        .into_json()
        .map_err(|error| format!("Could not read Apple Podcasts search results: {error}"))?;

    Ok(response
        .results
        .into_iter()
        .filter_map(|feed| {
            let title = feed.collection_name?.trim().to_string();
            let feed_url = feed.feed_url?.trim().to_string();
            if title.is_empty() || !usable_feed_url(&feed_url) {
                return None;
            }
            Some(Show {
                title,
                author: feed.artist_name.unwrap_or_default(),
                feed_url,
                artwork_urls: artwork_candidates([feed.artwork_url600, feed.artwork_url100]),
                updated: feed.release_date,
            })
        })
        .collect())
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

pub fn category(name: &str) -> Result<Vec<Show>, String> {
    if name == "Climate" {
        return search("climate");
    }
    let category_name = if name == "Health" { "Health & Fitness" } else { name };
    let key = std::env::var("PODCAST_INDEX_KEY").unwrap_or_default();
    let secret = std::env::var("PODCAST_INDEX_SECRET").unwrap_or_default();
    if key.is_empty() || secret.is_empty() {
        return Err("Category browsing needs a Podcast Index API key and secret. Set PODCAST_INDEX_KEY and PODCAST_INDEX_SECRET, then restart the app.".into());
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is unavailable.".to_string())?
        .as_secs()
        .to_string();
    let signature = format!("{:x}", Sha1::digest(format!("{key}{secret}{timestamp}").as_bytes()));
    let query = percent_encoding::utf8_percent_encode(category_name, percent_encoding::NON_ALPHANUMERIC);
    let url = format!("{PODCAST_INDEX_URL}/api/1.0/podcasts/trending?cat={query}&max=30");
    let response: TrendingResponse = agent()
        .get(&url)
        .set("X-Auth-Key", &key)
        .set("X-Auth-Date", &timestamp)
        .set("Authorization", &signature)
        .call()
        .map_err(|error| request_error("Podcast Index", error))?
        .into_json()
        .map_err(|error| format!("Could not read category results: {error}"))?;

    Ok(response
        .feeds
        .into_iter()
        .filter_map(|feed| {
            let title = feed.title?.trim().to_string();
            let feed_url = feed.url?.trim().to_string();
            if title.is_empty() || feed_url.is_empty() {
                return None;
            }
            Some(Show {
                title,
                author: feed.author.unwrap_or_default(),
                feed_url,
                artwork_urls: artwork_candidates([feed.image, feed.artwork]),
                updated: None,
            })
        })
        .collect())
}

pub fn feed(url: &str) -> Result<FeedDetails, String> {
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
            let audio_url = node
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
            let artwork_url = node.children().find_map(|child| {
                (child.is_element()
                    && matches!(child.tag_name().name(), "image" | "thumbnail"))
                    .then(|| child.attribute("href").or_else(|| child.attribute("url")))
                    .flatten()
                    .filter(|url| usable_feed_url(url))
                    .map(str::to_string)
            });
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

fn parse_duration(raw: &str) -> Option<u64> {
    if raw.is_empty() {
        return None;
    }
    raw.split(':').try_fold(0u64, |total, part| {
        total.checked_mul(60)?.checked_add(part.trim().parse::<u64>().ok()?)
    })
}

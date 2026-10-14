use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BASE_URL: &str = "https://api.podcastindex.org";
const USER_AGENT: &str = "ElitePodcasts/0.1 (io.github.elite.Podcasts; native GNOME podcast app)";

#[derive(Clone, Debug)]
pub struct Show {
    pub title: String,
    pub author: String,
    pub description: String,
    pub feed_url: String,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchFeed>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchFeed {
    collection_name: Option<String>,
    artist_name: Option<String>,
    feed_url: Option<String>,
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
    description: Option<String>,
    url: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()
}

fn request_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("Podcast Index returned HTTP {code}."),
        ureq::Error::Transport(error) => format!("Could not reach Podcast Index: {error}"),
    }
}

pub fn search(term: &str) -> Result<Vec<Show>, String> {
    let query = percent_encoding::utf8_percent_encode(term, percent_encoding::NON_ALPHANUMERIC);
    let url = format!("{BASE_URL}/search?term={query}");
    let response: SearchResponse = agent()
        .get(&url)
        .call()
        .map_err(request_error)?
        .into_json()
        .map_err(|error| format!("Could not read search results: {error}"))?;

    Ok(response
        .results
        .into_iter()
        .filter_map(|feed| {
            let title = feed.collection_name?.trim().to_string();
            let feed_url = feed.feed_url?.trim().to_string();
            if title.is_empty() || feed_url.is_empty() {
                return None;
            }
            Some(Show {
                title,
                author: feed.artist_name.unwrap_or_default(),
                description: String::new(),
                feed_url,
            })
        })
        .collect())
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
    let url = format!("{BASE_URL}/api/1.0/podcasts/trending?cat={query}&max=30");
    let response: TrendingResponse = agent()
        .get(&url)
        .set("X-Auth-Key", &key)
        .set("X-Auth-Date", &timestamp)
        .set("Authorization", &signature)
        .call()
        .map_err(request_error)?
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
                description: feed.description.unwrap_or_default(),
                feed_url,
            })
        })
        .collect())
}

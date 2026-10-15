use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BASE_URL: &str = "https://api.podcastindex.org";
pub(crate) const USER_AGENT: &str = "ElitePodcasts/0.1 (io.github.elite.Podcasts; native GNOME podcast app)";

#[derive(Clone, Debug)]
pub struct Show {
    pub title: String,
    pub author: String,
    pub description: String,
    pub feed_url: String,
    pub artwork_url: Option<String>,
}

pub struct Episode {
    pub title: String,
    pub date: String,
}

pub struct FeedDetails {
    pub title: String,
    pub description: String,
    pub artwork_url: Option<String>,
    pub episodes: Vec<Episode>,
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
    artwork_url600: Option<String>,
    artwork_url100: Option<String>,
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
    image: Option<String>,
    artwork: Option<String>,
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
                artwork_url: feed.artwork_url600.or(feed.artwork_url100),
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
                artwork_url: feed.image.or(feed.artwork),
            })
        })
        .collect())
}

pub fn feed(url: &str) -> Result<FeedDetails, String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("This show does not have a usable RSS URL.".into());
    }
    let response = agent().get(url).call().map_err(request_error)?;
    let mut xml = String::new();
    response
        .into_reader()
        .take(8 * 1024 * 1024)
        .read_to_string(&mut xml)
        .map_err(|error| format!("Could not read the RSS feed: {error}"))?;
    let document = roxmltree::Document::parse(&xml)
        .map_err(|error| format!("Could not parse the RSS feed: {error}"))?;
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
    let episodes = channel
        .children()
        .filter(|node| node.has_tag_name("item"))
        .take(20)
        .map(|node| Episode {
            title: field(node, "title"),
            date: field(node, "pubDate"),
        })
        .filter(|episode| !episode.title.is_empty())
        .collect();
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
        artwork_url,
        episodes,
    })
}

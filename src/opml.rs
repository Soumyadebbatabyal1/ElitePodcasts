//! Portable subscription lists in OPML 2.0 format.

use std::collections::HashSet;

use crate::discovery_service::Show;

const MAX_OPML_BYTES: usize = 5 * 1024 * 1024;
const MAX_SHOWS: usize = 1_000;

pub struct ImportList {
    pub shows: Vec<Show>,
    pub skipped: usize,
}

pub fn parse(bytes: &[u8]) -> Result<ImportList, String> {
    if bytes.len() > MAX_OPML_BYTES {
        return Err("This OPML file is larger than the 5 MB limit.".into());
    }
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| "This OPML file is not UTF-8 text.".to_string())?;
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("Could not read this OPML file: {error}"))?;
    if !document.root_element().has_tag_name("opml") {
        return Err("This file is not an OPML document.".into());
    }
    let body = document.root_element().children()
        .find(|node| node.has_tag_name("body"))
        .ok_or("This OPML file has no body.")?;
    let mut shows = Vec::new();
    let mut seen = HashSet::new();
    let mut skipped = 0;
    for outline in body.descendants().filter(|node| node.has_tag_name("outline")) {
        let Some(url) = outline.attribute("xmlUrl") else { continue };
        let url = url.trim();
        let is_rss = outline.attribute("type")
            .is_none_or(|kind| kind.eq_ignore_ascii_case("rss"));
        let valid_url = (url.starts_with("https://") || url.starts_with("http://"))
            && !url.chars().any(char::is_whitespace);
        if !is_rss || !valid_url || !seen.insert(url.to_string()) || shows.len() >= MAX_SHOWS {
            skipped += 1;
            continue;
        }
        let title = outline.attribute("text")
            .filter(|value| !value.trim().is_empty())
            .or_else(|| outline.attribute("title").filter(|value| !value.trim().is_empty()))
            .unwrap_or(url)
            .trim()
            .to_string();
        shows.push(Show {
            title, author: String::new(), feed_url: url.to_string(),
            artwork_urls: Vec::new(), updated: None,
        });
    }
    Ok(ImportList { shows, skipped })
}

pub fn export(shows: &[Show]) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <opml version=\"2.0\">\n\
           <head><title>Elite Podcasts Subscriptions</title></head>\n\
           <body>\n",
    );
    for show in shows {
        let title = escape(&show.title);
        let url = escape(&show.feed_url);
        xml.push_str(&format!(
            "    <outline type=\"rss\" text=\"{title}\" title=\"{title}\" xmlUrl=\"{url}\"/>\n",
        ));
    }
    xml.push_str("  </body>\n</opml>\n");
    xml
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}' => escaped.push(ch),
            _ => {},
        }
    }
    escaped
}

"""Refresh the small, manually selected network catalog from public RSS feeds.

Run this only when reviewing the curated selection. It verifies that each feed
contains episodes and that its show artwork responds as an image before writing
assets/curated_networks.json. It does not run when the app starts.
"""

import concurrent.futures
import json
import pathlib
import time
import urllib.request
import xml.etree.ElementTree as ET


ROOT = pathlib.Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "assets" / "curated_networks.json"
ITUNES = "http://www.itunes.com/dtds/podcast-1.0.dtd"
HEADERS = {"User-Agent": "ElitePodcastsCatalogReview/0.1"}

NETWORKS = [
    (
        "The New Yorker",
        "Politics, culture, and stories from The New Yorker.",
        [
            "https://feeds.simplecast.com/TRuO_SRo",
            "https://feeds.megaphone.fm/CNE3915766304",
            "https://feeds.megaphone.fm/CNE4835008151",
        ],
    ),
    (
        "BBC",
        "News, ideas, and documentaries from the BBC.",
        [
            "https://podcasts.files.bbci.co.uk/p02nq0gn.rss",
            "https://podcasts.files.bbci.co.uk/b006qykl.rss",
            "https://podcasts.files.bbci.co.uk/p02nq0lx.rss",
        ],
    ),
    (
        "Pushkin Industries",
        "Stories and ideas from Pushkin Industries.",
        [
            "https://www.omnycontent.com/d/playlist/e73c998e-6e60-432f-8610-ae210140c5b1/0e563f45-9d14-4ce8-8ef0-ae32006cd7e7/0d4cc74d-fff7-4b89-8818-ae32006cd7f0/podcast.rss",
            "https://www.omnycontent.com/d/playlist/e73c998e-6e60-432f-8610-ae210140c5b1/96c5c41e-0bc8-4661-b184-ae32006cd726/d623ef0b-3fee-4c26-b815-ae32006cd739/podcast.rss",
            "https://www.omnycontent.com/d/playlist/e73c998e-6e60-432f-8610-ae210140c5b1/f4e26994-3ddb-4ec7-b855-ae32006cd5de/ea4fae0c-3282-4cfd-b315-ae32006cd5ec/podcast.rss",
        ],
    ),
    (
        "Maximum Fun",
        "Comedy and culture from Maximum Fun.",
        [
            "https://feeds.simplecast.com/wjQvYtdl",
            "https://feeds.simplecast.com/q8x9cVws",
            "https://feeds.simplecast.com/y1LF_sn2",
        ],
    ),
    (
        "Vox Media",
        "News, science, and technology from Vox Media.",
        [
            "https://feeds.megaphone.fm/VMP5705694065",
            "https://feeds.megaphone.fm/vergecast",
            "https://feeds.megaphone.fm/VMP9331026707",
        ],
    ),
    (
        "NPR",
        "News and ideas from NPR-produced podcasts.",
        [
            "https://feeds.npr.org/510318/podcast.xml",
            "https://feeds.npr.org/510289/podcast.xml",
            "https://feeds.npr.org/510312/podcast.xml",
        ],
    ),
    (
        "TED Audio Collective",
        "Ideas and conversations from the TED Audio Collective.",
        [
            "https://feeds.acast.com/public/shows/67587e77c705e441797aff96",
            "https://feeds.acast.com/public/shows/67572b3b2dd88df13220eb3b",
            "https://feeds.npr.org/510298/podcast.xml",
        ],
    ),
    (
        "The Atlantic",
        "Reporting and conversations from The Atlantic.",
        [
            "https://feeds.megaphone.fm/radioatlantic",
            "https://feeds.megaphone.fm/permanentquestions",
            "https://feeds.megaphone.fm/galaxybrain",
        ],
    ),
    (
        "The New York Times",
        "News and opinion from The New York Times. Some episodes require a subscription.",
        [
            "https://feeds.simplecast.com/54nAGcIl",
            "https://feeds.simplecast.com/kphrlDnb",
            "https://feeds.simplecast.com/kEKXbjuJ",
        ],
    ),
    (
        "The Economist",
        "A public selection from The Economist. Full access may require a subscription.",
        ["https://feeds.acast.com/public/shows/theintelligencepodcast"],
    ),
]


def request(url, method="GET"):
    last_error = None
    for attempt in range(3):
        try:
            return urllib.request.urlopen(
                urllib.request.Request(url, headers=HEADERS, method=method), timeout=45
            )
        except Exception as error:
            last_error = error
            time.sleep(attempt + 1)
    raise RuntimeError(f"Could not load {url}: {last_error}")


def read_show(entry):
    network, feed_url = entry
    with request(feed_url) as response:
        channel = ET.fromstring(response.read(32 * 1024 * 1024)).find("channel")
    if channel is None or channel.find("item") is None:
        raise ValueError(f"No RSS episodes in {feed_url}")
    title = (channel.findtext("title") or "").strip()
    image = channel.find(f"{{{ITUNES}}}image")
    artwork_url = image.get("href") if image is not None else channel.findtext("image/url")
    if not title or not artwork_url:
        raise ValueError(f"Missing title or artwork in {feed_url}")
    artwork_url = artwork_url.replace(
        "http://ichef.bbci.co.uk/", "https://ichef.bbci.co.uk/"
    )
    with request(artwork_url, "HEAD") as response:
        if not response.headers.get("Content-Type", "").startswith("image/"):
            raise ValueError(f"Artwork is not an image: {artwork_url}")
    return {
        "title": title,
        "author": (channel.findtext(f"{{{ITUNES}}}author") or network).strip(),
        "feed_url": feed_url,
        "artwork_urls": [artwork_url],
    }


def main():
    entries = [(name, url) for name, _, urls in NETWORKS for url in urls]
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as executor:
        shows = list(executor.map(read_show, entries))
    by_url = {show["feed_url"]: show for show in shows}
    catalog = [
        {
            "name": name,
            "description": description,
            "shows": [by_url[url] for url in urls],
        }
        for name, description, urls in NETWORKS
    ]
    OUTPUT.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + "\n")
    print(f"Wrote {len(catalog)} networks and {len(shows)} verified shows to {OUTPUT}")


if __name__ == "__main__":
    main()

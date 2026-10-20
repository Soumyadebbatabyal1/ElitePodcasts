use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::discovery_service::{Episode, Show};

pub struct HistoryEntry {
    pub show: Show,
    pub episode: Episode,
    pub completed_at: i64,
}

pub struct StarredEpisode {
    pub show: Show,
    pub episode: Episode,
}

pub struct DownloadedEpisode {
    pub show: Show,
    pub episode: Episode,
    pub path: std::path::PathBuf,
}

pub struct FollowingStore {
    connection: Connection,
    listeners: RefCell<Vec<Box<dyn Fn() -> bool>>>,
}

impl FollowingStore {
    /// Read the episode-list filters on a worker without moving the UI's SQLite connection.
    pub fn episode_filters() -> Result<(HashSet<String>, HashMap<String, HashSet<String>>), String> {
        let path = gtk::glib::user_data_dir().join("elite-podcasts/podcasts.sqlite3");
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| format!("Could not read followed podcasts: {error}"))?;
        let mut statement = connection.prepare(
            "SELECT following.feed_url, played_episodes.episode_id
             FROM following LEFT JOIN played_episodes
             ON following.feed_url = played_episodes.feed_url",
        ).map_err(|error| format!("Could not read episode filters: {error}"))?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        }).map_err(|error| format!("Could not read episode filters: {error}"))?;
        let mut followed = HashSet::new();
        let mut played: HashMap<String, HashSet<String>> = HashMap::new();
        for row in rows {
            let (feed_url, episode_id) = row.map_err(|error| error.to_string())?;
            followed.insert(feed_url.clone());
            if let Some(episode_id) = episode_id {
                played.entry(feed_url).or_default().insert(episode_id);
            }
        }
        Ok((followed, played))
    }

    pub fn open() -> Result<Rc<Self>, String> {
        let directory = gtk::glib::user_data_dir().join("elite-podcasts");
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("Could not create the app data folder: {error}"))?;
        let connection = Connection::open(directory.join("podcasts.sqlite3"))
            .map_err(|error| format!("Could not open the podcast database: {error}"))?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS following (
                feed_url TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                author TEXT NOT NULL,
                artwork_urls TEXT NOT NULL,
                updated TEXT,
                added_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE TABLE IF NOT EXISTS played_episodes (
                feed_url TEXT NOT NULL,
                episode_id TEXT NOT NULL,
                played_at INTEGER NOT NULL DEFAULT (unixepoch()),
                PRIMARY KEY (feed_url, episode_id)
            );
            CREATE TABLE IF NOT EXISTS playback_positions (
                feed_url TEXT NOT NULL,
                episode_id TEXT NOT NULL,
                position_seconds INTEGER NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
                PRIMARY KEY (feed_url, episode_id)
            );
            CREATE TABLE IF NOT EXISTS completed_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                show_json TEXT NOT NULL,
                episode_json TEXT NOT NULL,
                completed_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE TABLE IF NOT EXISTS starred_episodes (
                feed_url TEXT NOT NULL,
                episode_id TEXT NOT NULL,
                show_json TEXT NOT NULL,
                episode_json TEXT NOT NULL,
                starred_at INTEGER NOT NULL DEFAULT (unixepoch()),
                PRIMARY KEY (feed_url, episode_id)
            );
            CREATE TABLE IF NOT EXISTS downloaded_episodes (
                feed_url TEXT NOT NULL,
                episode_id TEXT NOT NULL,
                show_json TEXT NOT NULL,
                episode_json TEXT NOT NULL,
                path TEXT NOT NULL,
                downloaded_at INTEGER NOT NULL DEFAULT (unixepoch()),
                PRIMARY KEY (feed_url, episode_id)
            );
            CREATE TABLE IF NOT EXISTS player_state (
                id INTEGER PRIMARY KEY CHECK(id = 1),
                show_json TEXT,
                episode_json TEXT,
                rate REAL NOT NULL DEFAULT 1.0
            );",
            )
            .map_err(|error| format!("Could not prepare the podcast database: {error}"))?;
        Ok(Rc::new(Self {
            connection,
            listeners: RefCell::new(Vec::new()),
        }))
    }

    pub fn contains(&self, feed_url: &str) -> bool {
        self.connection
            .query_row(
                "SELECT 1 FROM following WHERE feed_url = ?1",
                [feed_url],
                |_| Ok(()),
            )
            .is_ok()
    }

    pub fn list(&self) -> Result<Vec<Show>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT feed_url, title, author, artwork_urls, updated FROM following ORDER BY added_at DESC, rowid DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                let artwork_json: String = row.get(3)?;
                Ok(Show {
                    feed_url: row.get(0)?,
                    title: row.get(1)?,
                    author: row.get(2)?,
                    artwork_urls: serde_json::from_str(&artwork_json).unwrap_or_default(),
                    updated: row.get(4)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn add(&self, show: &Show) -> Result<bool, String> {
        let artwork = serde_json::to_string(&show.artwork_urls).map_err(|error| error.to_string())?;
        let changed = self
            .connection
            .execute(
                "INSERT OR IGNORE INTO following (feed_url, title, author, artwork_urls, updated) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![show.feed_url, show.title, show.author, artwork, show.updated],
            )
            .map_err(|error| format!("Could not follow this podcast: {error}"))?
            > 0;
        if changed {
            self.notify();
        }
        Ok(changed)
    }

    /// Keep a feed's own cover ahead of directory artwork for followed shows.
    pub fn prefer_feed_artworks(&self, covers: &[(String, String)]) -> Result<(), String> {
        if covers.is_empty() { return Ok(()) }
        let transaction = self.connection.unchecked_transaction()
            .map_err(|error| format!("Could not update podcast artwork: {error}"))?;
        let mut changed = false;
        for (feed_url, cover_url) in covers {
            if !(cover_url.starts_with("https://") || cover_url.starts_with("http://")) {
                continue;
            }
            let current: Option<String> = transaction.query_row(
                "SELECT artwork_urls FROM following WHERE feed_url = ?1",
                [feed_url],
                |row| row.get(0),
            ).optional().map_err(|error| format!("Could not read podcast artwork: {error}"))?;
            let Some(current) = current else { continue };
            let mut urls: Vec<String> = serde_json::from_str(&current).unwrap_or_default();
            if urls.first().is_some_and(|url| url == cover_url) {
                continue;
            }
            urls.retain(|url| url != cover_url);
            urls.insert(0, cover_url.clone());
            urls.truncate(4);
            let artwork = serde_json::to_string(&urls).map_err(|error| error.to_string())?;
            transaction.execute(
                "UPDATE following SET artwork_urls = ?2 WHERE feed_url = ?1",
                params![feed_url, artwork],
            ).map_err(|error| format!("Could not save podcast artwork: {error}"))?;
            changed = true;
        }
        transaction.commit().map_err(|error| format!("Could not save podcast artwork: {error}"))?;
        if changed { self.notify(); }
        Ok(())
    }

    /// Add an OPML list in one transaction and refresh the Following grid once.
    pub fn add_many(&self, shows: &[Show]) -> Result<Vec<Show>, String> {
        let transaction = self.connection.unchecked_transaction()
            .map_err(|error| format!("Could not begin importing shows: {error}"))?;
        let mut inserted = Vec::new();
        {
            let mut statement = transaction.prepare(
                "INSERT OR IGNORE INTO following (feed_url, title, author, artwork_urls, updated)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            ).map_err(|error| format!("Could not prepare show import: {error}"))?;
            for show in shows {
                let artwork = serde_json::to_string(&show.artwork_urls)
                    .map_err(|error| error.to_string())?;
                if statement.execute(params![show.feed_url, show.title, show.author, artwork, show.updated])
                    .map_err(|error| format!("Could not import shows: {error}"))? > 0 {
                    inserted.push(show.clone());
                }
            }
        }
        transaction.commit().map_err(|error| format!("Could not save imported shows: {error}"))?;
        if !inserted.is_empty() {
            self.notify();
        }
        Ok(inserted)
    }

    /// Fill the missing artwork and titles of newly imported shows from their RSS feeds.
    pub fn enrich_imported(&self, details: &[(String, String, Option<String>)]) -> Result<(), String> {
        if details.is_empty() { return Ok(()) }
        let transaction = self.connection.unchecked_transaction()
            .map_err(|error| format!("Could not begin updating imported shows: {error}"))?;
        let mut changed = 0;
        {
            let mut statement = transaction.prepare(
                "UPDATE following SET
                   title = CASE WHEN title = feed_url AND ?2 != '' THEN ?2 ELSE title END,
                   artwork_urls = CASE WHEN artwork_urls = '[]' AND ?3 != '' THEN ?3 ELSE artwork_urls END
                 WHERE feed_url = ?1 AND
                   ((title = feed_url AND ?2 != '') OR (artwork_urls = '[]' AND ?3 != ''))",
            ).map_err(|error| format!("Could not prepare imported artwork: {error}"))?;
            for (feed_url, title, artwork_url) in details {
                let artwork = artwork_url.as_ref()
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                    .map(|url| serde_json::to_string(&vec![url]).unwrap_or_default())
                    .unwrap_or_default();
                changed += statement.execute(params![feed_url, title, artwork])
                    .map_err(|error| format!("Could not update imported artwork: {error}"))?;
            }
        }
        transaction.commit().map_err(|error| format!("Could not save imported artwork: {error}"))?;
        if changed > 0 { self.notify(); }
        Ok(())
    }

    pub fn remove(&self, feed_url: &str) -> Result<bool, String> {
        let changed = self
            .connection
            .execute("DELETE FROM following WHERE feed_url = ?1", [feed_url])
            .map_err(|error| format!("Could not unsubscribe: {error}"))?
            > 0;
        if changed {
            self.notify();
        }
        Ok(changed)
    }

    pub fn watch(&self, listener: impl Fn() -> bool + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn mark_played(&self, feed_url: &str, episode_ids: &[String]) -> Result<usize, String> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| format!("Could not begin updating played episodes: {error}"))?;
        let mut added = 0;
        {
            let mut statement = transaction
                .prepare(
                    "INSERT OR IGNORE INTO played_episodes (feed_url, episode_id) VALUES (?1, ?2)",
                )
                .map_err(|error| format!("Could not prepare played episodes: {error}"))?;
            for episode_id in episode_ids {
                added += statement
                    .execute(params![feed_url, episode_id])
                    .map_err(|error| format!("Could not mark episodes as played: {error}"))?;
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("Could not save played episodes: {error}"))?;
        if added > 0 {
            self.notify();
        }
        Ok(added)
    }

    pub fn mark_played_and_reset(&self, feed_url: &str, episode_ids: &[String]) -> Result<usize, String> {
        let transaction = self.connection.unchecked_transaction()
            .map_err(|error| format!("Could not begin updating played episodes: {error}"))?;
        let mut added = 0;
        let mut reset = 0;
        {
            let mut mark = transaction.prepare(
                "INSERT OR IGNORE INTO played_episodes (feed_url, episode_id) VALUES (?1, ?2)",
            ).map_err(|error| format!("Could not prepare played episodes: {error}"))?;
            let mut clear_position = transaction.prepare(
                "DELETE FROM playback_positions WHERE feed_url = ?1 AND episode_id = ?2",
            ).map_err(|error| format!("Could not prepare progress reset: {error}"))?;
            for episode_id in episode_ids {
                added += mark.execute(params![feed_url, episode_id])
                    .map_err(|error| format!("Could not mark episodes as played: {error}"))?;
                reset += clear_position.execute(params![feed_url, episode_id])
                    .map_err(|error| format!("Could not reset listening progress: {error}"))?;
            }
        }
        transaction.commit()
            .map_err(|error| format!("Could not save played episodes: {error}"))?;
        if added > 0 || reset > 0 {
            self.notify();
        }
        Ok(added)
    }

    pub fn mark_all_unplayed(&self, feed_url: &str) -> Result<usize, String> {
        let removed = self.connection
            .execute("DELETE FROM played_episodes WHERE feed_url = ?1", [feed_url])
            .map_err(|error| format!("Could not mark episodes as unplayed: {error}"))?;
        if removed > 0 {
            self.notify();
        }
        Ok(removed)
    }

    pub fn mark_unplayed(&self, feed_url: &str, episode_id: &str) -> Result<bool, String> {
        let removed = self.connection.execute(
            "DELETE FROM played_episodes WHERE feed_url = ?1 AND episode_id = ?2",
            params![feed_url, episode_id],
        ).map_err(|error| format!("Could not mark episode as unplayed: {error}"))? > 0;
        if removed { self.notify(); }
        Ok(removed)
    }

    pub fn is_played(&self, feed_url: &str, episode_id: &str) -> bool {
        self.connection.query_row(
            "SELECT 1 FROM played_episodes WHERE feed_url = ?1 AND episode_id = ?2",
            params![feed_url, episode_id], |_| Ok(()),
        ).is_ok()
    }

    pub fn played_ids(&self, feed_url: &str) -> Result<HashSet<String>, String> {
        let mut statement = self.connection
            .prepare("SELECT episode_id FROM played_episodes WHERE feed_url = ?1")
            .map_err(|error| format!("Could not read played episodes: {error}"))?;
        let rows = statement
            .query_map([feed_url], |row| row.get::<_, String>(0))
            .map_err(|error| format!("Could not read played episodes: {error}"))?;
        rows.collect::<Result<HashSet<_>, _>>()
            .map_err(|error| format!("Could not read played episodes: {error}"))
    }

    pub fn playback_position(&self, feed_url: &str, episode_id: &str) -> u64 {
        self.connection
            .query_row(
                "SELECT position_seconds FROM playback_positions WHERE feed_url = ?1 AND episode_id = ?2",
                params![feed_url, episode_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            .max(0) as u64
    }

    pub fn player_state(&self) -> Result<(Option<(Show, Episode)>, f64), String> {
        let row = self.connection.query_row(
            "SELECT show_json, episode_json, rate FROM player_state WHERE id = 1",
            [],
            |row| Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, f64>(2)?,
            )),
        ).optional().map_err(|error| format!("Could not read player state: {error}"))?;
        let Some((show_json, episode_json, rate)) = row else {
            return Ok((None, 1.0));
        };
        let episode = match (show_json, episode_json) {
            (Some(show), Some(episode)) => Some((
                serde_json::from_str(&show).map_err(|error| format!("Could not read saved show: {error}"))?,
                serde_json::from_str(&episode).map_err(|error| format!("Could not read saved episode: {error}"))?,
            )),
            _ => None,
        };
        Ok((episode, rate))
    }

    pub fn save_player_episode(&self, show: &Show, episode: &Episode, rate: f64) -> Result<(), String> {
        let show_json = serde_json::to_string(show).map_err(|error| error.to_string())?;
        let episode_json = serde_json::to_string(episode).map_err(|error| error.to_string())?;
        self.connection.execute(
            "INSERT INTO player_state (id, show_json, episode_json, rate) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
             show_json = excluded.show_json, episode_json = excluded.episode_json, rate = excluded.rate",
            params![show_json, episode_json, rate],
        ).map_err(|error| format!("Could not save player state: {error}"))?;
        Ok(())
    }

    pub fn save_playback_rate(&self, rate: f64) -> Result<(), String> {
        self.connection.execute(
            "INSERT INTO player_state (id, rate) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET rate = excluded.rate",
            [rate],
        ).map_err(|error| format!("Could not save playback speed: {error}"))?;
        Ok(())
    }

    pub fn clear_player_episode(&self) -> Result<(), String> {
        self.connection.execute(
            "UPDATE player_state SET show_json = NULL, episode_json = NULL WHERE id = 1",
            [],
        ).map_err(|error| format!("Could not clear finished episode: {error}"))?;
        Ok(())
    }

    pub fn save_playback_position(
        &self,
        feed_url: &str,
        episode_id: &str,
        seconds: u64,
    ) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO playback_positions (feed_url, episode_id, position_seconds)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(feed_url, episode_id) DO UPDATE SET
                   position_seconds = excluded.position_seconds,
                   updated_at = unixepoch()",
                params![feed_url, episode_id, seconds.min(i64::MAX as u64) as i64],
            )
            .map_err(|error| format!("Could not save playback position: {error}"))?;
        Ok(())
    }

    pub fn record_completed(&self, show: &Show, episode: &Episode) -> Result<(), String> {
        let show_json = serde_json::to_string(show).map_err(|error| error.to_string())?;
        let episode_json = serde_json::to_string(episode).map_err(|error| error.to_string())?;
        self.connection.execute(
            "INSERT INTO completed_history (show_json, episode_json) VALUES (?1, ?2)",
            params![show_json, episode_json],
        ).map_err(|error| format!("Could not save playback history: {error}"))?;
        Ok(())
    }

    pub fn history(&self) -> Result<Vec<HistoryEntry>, String> {
        let mut statement = self.connection.prepare(
            "SELECT show_json, episode_json, completed_at FROM completed_history ORDER BY id DESC"
        ).map_err(|error| format!("Could not read playback history: {error}"))?;
        let rows = statement.query_map([], |row| {
            let show_json: String = row.get(0)?;
            let episode_json: String = row.get(1)?;
            Ok((show_json, episode_json, row.get::<_, i64>(2)?))
        }).map_err(|error| format!("Could not read playback history: {error}"))?;
        rows.map(|row| {
            let (show_json, episode_json, completed_at) = row.map_err(|error| error.to_string())?;
            Ok(HistoryEntry {
                show: serde_json::from_str(&show_json).map_err(|error| error.to_string())?,
                episode: serde_json::from_str(&episode_json).map_err(|error| error.to_string())?,
                completed_at,
            })
        }).collect()
    }

    pub fn is_starred(&self, feed_url: &str, episode_id: &str) -> bool {
        self.connection.query_row(
            "SELECT 1 FROM starred_episodes WHERE feed_url = ?1 AND episode_id = ?2",
            params![feed_url, episode_id], |_| Ok(()),
        ).is_ok()
    }

    pub fn toggle_starred(&self, show: &Show, episode: &Episode) -> Result<bool, String> {
        let starred = if self.is_starred(&show.feed_url, &episode.id) {
            self.connection.execute(
                "DELETE FROM starred_episodes WHERE feed_url = ?1 AND episode_id = ?2",
                params![show.feed_url, episode.id],
            ).map_err(|error| format!("Could not remove episode star: {error}"))?;
            false
        } else {
            let show_json = serde_json::to_string(show).map_err(|error| error.to_string())?;
            let episode_json = serde_json::to_string(episode).map_err(|error| error.to_string())?;
            self.connection.execute(
                "INSERT INTO starred_episodes (feed_url, episode_id, show_json, episode_json) VALUES (?1, ?2, ?3, ?4)",
                params![show.feed_url, episode.id, show_json, episode_json],
            ).map_err(|error| format!("Could not star episode: {error}"))?;
            true
        };
        self.notify();
        Ok(starred)
    }

    pub fn starred(&self) -> Result<Vec<StarredEpisode>, String> {
        let mut statement = self.connection.prepare(
            "SELECT show_json, episode_json FROM starred_episodes ORDER BY starred_at DESC, rowid DESC",
        ).map_err(|error| format!("Could not read starred episodes: {error}"))?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|error| format!("Could not read starred episodes: {error}"))?;
        rows.map(|row| {
            let (show, episode) = row.map_err(|error| error.to_string())?;
            Ok(StarredEpisode {
                show: serde_json::from_str(&show).map_err(|error| error.to_string())?,
                episode: serde_json::from_str(&episode).map_err(|error| error.to_string())?,
            })
        }).collect()
    }

    pub fn is_downloaded(&self, feed_url: &str, episode_id: &str) -> bool {
        self.connection.query_row(
            "SELECT 1 FROM downloaded_episodes WHERE feed_url = ?1 AND episode_id = ?2",
            params![feed_url, episode_id], |_| Ok(()),
        ).is_ok()
    }

    pub fn save_download(
        &self,
        show: &Show,
        episode: &Episode,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let show_json = serde_json::to_string(show).map_err(|error| error.to_string())?;
        let episode_json = serde_json::to_string(episode).map_err(|error| error.to_string())?;
        self.connection.execute(
            "INSERT INTO downloaded_episodes (feed_url, episode_id, show_json, episode_json, path)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(feed_url, episode_id) DO UPDATE SET
               show_json = excluded.show_json,
               episode_json = excluded.episode_json,
               path = excluded.path,
               downloaded_at = unixepoch()",
            params![show.feed_url, episode.id, show_json, episode_json, path.to_string_lossy()],
        ).map_err(|error| format!("Could not save downloaded episode: {error}"))?;
        self.notify();
        Ok(())
    }

    pub fn downloaded(&self) -> Result<Vec<DownloadedEpisode>, String> {
        let mut statement = self.connection.prepare(
            "SELECT show_json, episode_json, path FROM downloaded_episodes
             ORDER BY downloaded_at DESC, rowid DESC",
        ).map_err(|error| format!("Could not read downloaded episodes: {error}"))?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        }).map_err(|error| format!("Could not read downloaded episodes: {error}"))?;
        rows.map(|row| {
            let (show, episode, path) = row.map_err(|error| error.to_string())?;
            Ok(DownloadedEpisode {
                show: serde_json::from_str(&show).map_err(|error| error.to_string())?,
                episode: serde_json::from_str(&episode).map_err(|error| error.to_string())?,
                path: path.into(),
            })
        }).collect()
    }

    pub fn remove_download(&self, feed_url: &str, episode_id: &str) -> Result<Option<std::path::PathBuf>, String> {
        let path = self.connection.query_row(
            "SELECT path FROM downloaded_episodes WHERE feed_url = ?1 AND episode_id = ?2",
            params![feed_url, episode_id], |row| row.get::<_, String>(0),
        ).optional().map_err(|error| format!("Could not find downloaded episode: {error}"))?;
        if let Some(path) = path {
            self.connection.execute(
                "DELETE FROM downloaded_episodes WHERE feed_url = ?1 AND episode_id = ?2",
                params![feed_url, episode_id],
            ).map_err(|error| format!("Could not remove downloaded episode: {error}"))?;
            self.notify();
            Ok(Some(path.into()))
        } else {
            Ok(None)
        }
    }

    pub fn clear_history(&self) -> Result<(), String> {
        self.connection.execute("DELETE FROM completed_history", [])
            .map_err(|error| format!("Could not clear playback history: {error}"))?;
        Ok(())
    }

    fn notify(&self) {
        // A listener can rebuild a page and register more listeners. Run callbacks
        // without holding the RefCell borrow so watch() remains safe during notify.
        let mut listeners = std::mem::take(&mut *self.listeners.borrow_mut());
        listeners.retain(|listener| listener());
        self.listeners.borrow_mut().splice(0..0, listeners);
    }
}

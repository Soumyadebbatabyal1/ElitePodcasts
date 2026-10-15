use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::{params, Connection};

use crate::discovery_service::Show;

pub struct FollowingStore {
    connection: Connection,
    listeners: RefCell<Vec<Box<dyn Fn() -> bool>>>,
}

impl FollowingStore {
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
        Ok(added)
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

    fn notify(&self) {
        self.listeners.borrow_mut().retain(|listener| listener());
    }
}

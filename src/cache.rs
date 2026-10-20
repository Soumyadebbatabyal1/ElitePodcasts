use serde::de::DeserializeOwned;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

pub fn clear_all() -> Result<(), String> {
    let directory = gtk::glib::user_cache_dir().join("elite-podcasts");
    match std::fs::remove_dir_all(directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not clear cache: {error}")),
    }
}

fn path(kind: &str, key: &str) -> PathBuf {
    let digest = Sha1::digest(key.as_bytes());
    gtk::glib::user_cache_dir()
        .join("elite-podcasts")
        .join(kind)
        .join(format!("{digest:x}"))
}

pub fn read_bytes(kind: &str, key: &str, max_age: Option<Duration>) -> Option<Vec<u8>> {
    let path = path(kind, key);
    if let Some(max_age) = max_age {
        let age = path.metadata().ok()?.modified().ok()?.elapsed().ok()?;
        if age > max_age {
            return None;
        }
    }
    std::fs::read(path).ok()
}

pub fn write_bytes(kind: &str, key: &str, bytes: &[u8]) {
    let path = path(kind, key);
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let temp = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    if std::fs::write(&temp, bytes).is_ok() {
        let _ = std::fs::rename(temp, path);
    }
}

pub fn read_json<T: DeserializeOwned>(kind: &str, key: &str, max_age: Option<Duration>) -> Option<T> {
    serde_json::from_slice(&read_bytes(kind, key, max_age)?).ok()
}

pub fn write_json<T: Serialize>(kind: &str, key: &str, value: &T) {
    if let Ok(bytes) = serde_json::to_vec(value) {
        write_bytes(kind, key, &bytes);
    }
}

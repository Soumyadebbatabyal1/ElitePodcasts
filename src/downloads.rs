use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use sha1::{Digest, Sha1};

use crate::discovery_service::{Episode, Show, USER_AGENT};

pub fn podcast_download_dir() -> PathBuf {
    let music = gtk::glib::user_special_dir(gtk::glib::UserDirectory::Music)
        .unwrap_or_else(|| gtk::glib::home_dir().join("Music"));
    music.join("Podcasts")
}

pub fn download_episode(
    show: &Show,
    episode: &Episode,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, String> {
    let url = episode.audio_url.as_deref()
        .ok_or_else(|| "This episode has no downloadable audio source".to_owned())?;
    let directory = podcast_download_dir();
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create {}: {error}", directory.display()))?;

    let digest = format!("{:x}", Sha1::digest(format!("{}\n{}", show.feed_url, episode.id).as_bytes()));
    let extension = audio_extension(url);
    let stem = safe_file_stem(&episode.title);
    let filename = format!("{stem}-{}.{}", &digest[..10], extension);
    let destination = directory.join(filename);
    let partial = destination.with_extension(format!("{extension}.part"));

    let response = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(15))
        .redirects(20)
        .user_agent(USER_AGENT)
        .build()
        .get(url)
        .call()
        .map_err(|error| format!("Could not download episode: {error}"))?;
    if cancelled.load(Ordering::Relaxed) {
        return Err("Download canceled".to_owned());
    }
    let total = response.header("Content-Length").and_then(|value| value.parse::<u64>().ok())
        .filter(|size| *size > 0);
    progress(0, total);
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&partial)
        .map_err(|error| format!("Could not create download file: {error}"))?;
    let result = copy_audio(&mut reader, &mut file, cancelled, total, &mut progress)
        .and_then(|bytes| {
            if bytes == 0 { return Err("The episode audio file is empty".to_owned()); }
            file.flush().map_err(|error| format!("Could not finish episode download: {error}"))
        });
    drop(file);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    if cancelled.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&partial);
        return Err("Download canceled".to_owned());
    }
    if destination.exists() {
        if let Err(error) = std::fs::remove_file(&destination) {
            let _ = std::fs::remove_file(&partial);
            return Err(format!("Could not replace existing download: {error}"));
        }
    }
    if let Err(error) = std::fs::rename(&partial, &destination) {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("Could not save downloaded episode: {error}"));
    }
    Ok(destination)
}

fn copy_audio(
    reader: &mut impl Read,
    writer: &mut impl Write,
    cancelled: &AtomicBool,
    total: Option<u64>,
    progress: &mut impl FnMut(u64, Option<u64>),
) -> Result<u64, String> {
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    let mut reported = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) { return Err("Download canceled".to_owned()); }
        let count = reader.read(&mut buffer)
            .map_err(|error| format!("Download was interrupted: {error}"))?;
        if count == 0 { break; }
        writer.write_all(&buffer[..count])
            .map_err(|error| format!("Could not write episode audio: {error}"))?;
        bytes += count as u64;
        if reported.elapsed() >= Duration::from_millis(200) {
            progress(bytes, total);
            reported = Instant::now();
        }
    }
    progress(bytes, total);
    Ok(bytes)
}

fn audio_extension(url: &str) -> &'static str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    match Path::new(path).extension().and_then(|extension| extension.to_str())
        .unwrap_or("").to_ascii_lowercase().as_str()
    {
        "mp3" => "mp3",
        "m4a" => "m4a",
        "mp4" => "mp4",
        "aac" => "aac",
        "ogg" => "ogg",
        "opus" => "opus",
        "wav" => "wav",
        "flac" => "flac",
        "oga" => "oga",
        _ => "audio",
    }
}

fn safe_file_stem(title: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in title.chars() {
        if character.is_alphanumeric() {
            if separator && !result.is_empty() { result.push('-'); }
            result.push(character);
            separator = false;
        } else {
            separator = true;
        }
        if result.len() >= 100 { break; }
    }
    if result.is_empty() { "episode".to_owned() } else { result }
}

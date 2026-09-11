//! Lightweight metadata probe. Runs before a download so the UI can show what
//! it is about to fetch (single track vs playlist/album, title, item count).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Result of a probe: enough to preview the job.
#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub title: String,
    pub uploader: Option<String>,
    pub is_playlist: bool,
    pub entry_count: Option<usize>,
    pub webpage_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawInfo {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    uploader: Option<String>,
    #[serde(default, rename = "_type")]
    kind: Option<String>,
    #[serde(default)]
    playlist_count: Option<usize>,
    #[serde(default)]
    entries: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    webpage_url: Option<String>,
}

/// Windows: suppress the console window yt-dlp would otherwise flash.
#[cfg(windows)]
fn base_command(ytdlp: &Path) -> Command {
    let mut cmd = Command::new(ytdlp);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Unix: nothing to suppress. Split into its own function rather than a
/// `#[cfg(windows)]` statement inside a shared body, because the binding would
/// then be `mut` with no mutation here: an `unused_mut` warning, which CI
/// treats as an error (`-D warnings`). Same split as `kill_tree` in `runner.rs`.
#[cfg(not(windows))]
fn base_command(ytdlp: &Path) -> Command {
    Command::new(ytdlp)
}

/// Query yt-dlp for a flat description of `url`. Uses `--flat-playlist` so a
/// large album/playlist resolves quickly without touching every entry.
pub fn probe(ytdlp: &Path, url: &str) -> Result<MediaInfo> {
    let bin_dir = ytdlp.parent().unwrap_or_else(|| Path::new("."));
    let deno = bin_dir.join(if cfg!(windows) { "deno.exe" } else { "deno" });
    let output = base_command(ytdlp)
        .args([
            "--ignore-config",
            // Bound every network wait so a stalled probe errors out instead
            // of leaving the UI's "reading link" state stuck forever.
            "--socket-timeout",
            "15",
            "--encoding",
            "utf-8",
            "--no-js-runtimes",
            "--js-runtimes",
        ])
        .arg(format!("deno:{}", deno.to_string_lossy()))
        .args([
            "--no-remote-components",
            "--no-warnings",
            "--flat-playlist",
            "--dump-single-json",
            url,
        ])
        .output()
        .context("spawning yt-dlp for probe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("yt-dlp probe failed: {}", stderr.trim());
    }

    let raw: RawInfo =
        serde_json::from_slice(&output.stdout).context("parsing yt-dlp JSON output")?;

    let is_playlist = raw.kind.as_deref() == Some("playlist") || raw.entries.is_some();
    let entry_count = raw
        .playlist_count
        .or_else(|| raw.entries.as_ref().map(|e| e.len()));

    Ok(MediaInfo {
        title: raw.title.unwrap_or_else(|| "(untitled)".to_string()),
        uploader: raw.uploader,
        is_playlist,
        entry_count,
        webpage_url: raw.webpage_url,
    })
}

/// Strict check that a string is an HTTPS URL whose host is exactly YouTube,
/// YouTube Music, or youtu.be. The UI refuses to probe or download anything
/// else, so substring tricks (`https://youtube.com.evil.com/…`,
/// `https://evil.com/?youtube.com`, userinfo `@`, explicit ports) must fail.
pub fn looks_like_supported_url(s: &str) -> bool {
    const ALLOWED_HOSTS: [&str; 6] = [
        "youtube.com",
        "www.youtube.com",
        "m.youtube.com",
        "music.youtube.com",
        "youtu.be",
        "www.youtu.be",
    ];

    let lower = s.trim().to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // YouTube URLs never carry userinfo or an explicit port; both are
    // classic host-spoofing vectors, so their mere presence disqualifies.
    if authority.contains('@') || authority.contains(':') {
        return false;
    }
    ALLOWED_HOSTS.contains(&authority)
}

#[cfg(test)]
mod tests {
    use super::looks_like_supported_url;

    #[test]
    fn accepts_common_youtube_urls() {
        assert!(looks_like_supported_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(looks_like_supported_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(looks_like_supported_url(
            "https://music.youtube.com/watch?v=abc&list=OLAK5uy_x"
        ));
        assert!(looks_like_supported_url(
            "https://www.youtube.com/playlist?list=PLxyz"
        ));
    }

    #[test]
    fn rejects_junk() {
        assert!(!looks_like_supported_url("not a url"));
        assert!(!looks_like_supported_url("https://vimeo.com/12345"));
        assert!(!looks_like_supported_url("ftp://youtube.com/x"));
        assert!(!looks_like_supported_url(
            "http://www.youtube.com/watch?v=x"
        ));
    }

    #[test]
    fn rejects_host_spoofing() {
        assert!(!looks_like_supported_url(
            "https://youtube.com.evil.com/watch?v=x"
        ));
        assert!(!looks_like_supported_url("https://evilyoutube.com/watch"));
        assert!(!looks_like_supported_url("https://evil.com/?youtube.com"));
        assert!(!looks_like_supported_url("https://evil.com/youtube.com"));
        assert!(!looks_like_supported_url("https://evil.com#youtube.com"));
        assert!(!looks_like_supported_url("https://youtube.com@evil.com/x"));
        assert!(!looks_like_supported_url("https://youtube.com:8080/watch"));
    }

    #[test]
    fn accepts_mobile_and_bare_hosts() {
        assert!(looks_like_supported_url("https://m.youtube.com/watch?v=x"));
        assert!(looks_like_supported_url("https://youtube.com/watch?v=x"));
        assert!(looks_like_supported_url(
            "  https://MUSIC.youtube.com/playlist?list=OLAK5uy_x  "
        ));
    }
}

//! Pure, side-effect-free construction of the yt-dlp argument vector.
//!
//! This module is deliberately free of I/O so it can be unit-tested in
//! isolation. `DownloadOptions::build_args` is the single source of truth for
//! how UI choices translate into yt-dlp command-line flags.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What the user wants out of the download.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadKind {
    /// Extract audio only (the primary use case).
    #[default]
    AudioOnly,
    /// Keep video (muxed with best audio).
    Video,
}

/// Target audio container/codec. `Best` keeps the source stream without
/// re-encoding (fastest, no quality loss).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioFormat {
    Best,
    #[default]
    Mp3,
    Flac,
    Opus,
    M4a,
    Wav,
    Vorbis,
    Aac,
}

impl AudioFormat {
    /// The token yt-dlp expects after `--audio-format`.
    pub fn yt_dlp_token(self) -> Option<&'static str> {
        match self {
            AudioFormat::Best => None, // no recode
            AudioFormat::Mp3 => Some("mp3"),
            AudioFormat::Flac => Some("flac"),
            AudioFormat::Opus => Some("opus"),
            AudioFormat::M4a => Some("m4a"),
            AudioFormat::Wav => Some("wav"),
            AudioFormat::Vorbis => Some("vorbis"),
            AudioFormat::Aac => Some("aac"),
        }
    }

    /// Lossless targets ignore the quality/bitrate setting.
    pub fn is_lossless(self) -> bool {
        matches!(self, AudioFormat::Flac | AudioFormat::Wav)
    }

    pub fn label(self) -> &'static str {
        match self {
            AudioFormat::Best => "Best (source, no re-encode)",
            AudioFormat::Mp3 => "MP3",
            AudioFormat::Flac => "FLAC (lossless container)",
            AudioFormat::Opus => "Opus",
            AudioFormat::M4a => "M4A (AAC)",
            AudioFormat::Wav => "WAV",
            AudioFormat::Vorbis => "Ogg Vorbis",
            AudioFormat::Aac => "AAC",
        }
    }

    pub const ALL: [AudioFormat; 8] = [
        AudioFormat::Best,
        AudioFormat::Mp3,
        AudioFormat::Flac,
        AudioFormat::Opus,
        AudioFormat::M4a,
        AudioFormat::Wav,
        AudioFormat::Vorbis,
        AudioFormat::Aac,
    ];
}

/// Audio quality for lossy formats. yt-dlp accepts either a VBR level (0 best,
/// 10 worst) or an explicit bitrate like `320K`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioQuality {
    /// Best available (`--audio-quality 0`).
    #[default]
    Best,
    /// Fixed bitrate in kbps (e.g. 320, 256, 192, 128).
    Kbps(u32),
}

impl AudioQuality {
    pub fn yt_dlp_token(self) -> String {
        match self {
            AudioQuality::Best => "0".to_string(),
            AudioQuality::Kbps(k) => format!("{k}K"),
        }
    }

    pub fn label(self) -> String {
        match self {
            AudioQuality::Best => "Best".to_string(),
            AudioQuality::Kbps(k) => format!("{k} kbps"),
        }
    }

    pub const PRESETS: [AudioQuality; 5] = [
        AudioQuality::Best,
        AudioQuality::Kbps(320),
        AudioQuality::Kbps(256),
        AudioQuality::Kbps(192),
        AudioQuality::Kbps(128),
    ];
}

/// Maximum video height. Used to build the yt-dlp format selector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoQuality {
    Best,
    P2160,
    P1440,
    #[default]
    P1080,
    P720,
    P480,
    P360,
}

impl VideoQuality {
    pub fn max_height(self) -> Option<u32> {
        match self {
            VideoQuality::Best => None,
            VideoQuality::P2160 => Some(2160),
            VideoQuality::P1440 => Some(1440),
            VideoQuality::P1080 => Some(1080),
            VideoQuality::P720 => Some(720),
            VideoQuality::P480 => Some(480),
            VideoQuality::P360 => Some(360),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VideoQuality::Best => "Best",
            VideoQuality::P2160 => "2160p (4K)",
            VideoQuality::P1440 => "1440p",
            VideoQuality::P1080 => "1080p",
            VideoQuality::P720 => "720p",
            VideoQuality::P480 => "480p",
            VideoQuality::P360 => "360p",
        }
    }

    pub const ALL: [VideoQuality; 7] = [
        VideoQuality::Best,
        VideoQuality::P2160,
        VideoQuality::P1440,
        VideoQuality::P1080,
        VideoQuality::P720,
        VideoQuality::P480,
        VideoQuality::P360,
    ];
}

/// Video container when muxing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoContainer {
    #[default]
    Mp4,
    Mkv,
    Webm,
}

impl VideoContainer {
    pub fn token(self) -> &'static str {
        match self {
            VideoContainer::Mp4 => "mp4",
            VideoContainer::Mkv => "mkv",
            VideoContainer::Webm => "webm",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            VideoContainer::Mp4 => "MP4",
            VideoContainer::Mkv => "MKV",
            VideoContainer::Webm => "WebM",
        }
    }
    pub const ALL: [VideoContainer; 3] = [
        VideoContainer::Mp4,
        VideoContainer::Mkv,
        VideoContainer::Webm,
    ];
}

/// Filename templates. These map to yt-dlp `-o` output templates. `Custom`
/// carries a raw template string the user typed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamingPreset {
    /// `%(title)s.%(ext)s`
    TitleOnly,
    /// `%(artist)s - %(title)s.%(ext)s`
    ArtistTitle,
    /// Prefix native track number or playlist order when one is available.
    #[default]
    PlaylistIndexTitle,
    /// Album/playlist folder with native track number or playlist-order fallback.
    AlbumTrack,
    /// Raw user-supplied template.
    Custom(String),
}

impl NamingPreset {
    pub fn template(&self) -> String {
        match self {
            NamingPreset::TitleOnly => "%(title)s.%(ext)s".to_string(),
            NamingPreset::ArtistTitle => "%(artist)s - %(title)s.%(ext)s".to_string(),
            NamingPreset::PlaylistIndexTitle => {
                "%(catchyt_track_prefix&{} - |)s%(track,title)s.%(ext)s".to_string()
            }
            NamingPreset::AlbumTrack => {
                "%(album,playlist|Album)s/%(catchyt_track_prefix&{} - |)s%(track,title)s.%(ext)s"
                    .to_string()
            }
            NamingPreset::Custom(s) => {
                // An emptied custom field would reach yt-dlp as `-o ""`;
                // fall back to the plain-title template instead.
                if s.trim().is_empty() {
                    "%(title)s.%(ext)s".to_string()
                } else {
                    s.clone()
                }
            }
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            NamingPreset::TitleOnly => "Title",
            NamingPreset::ArtistTitle => "Artist - Title",
            NamingPreset::PlaylistIndexTitle => "01 - Title (when available)",
            NamingPreset::AlbumTrack => "Album / 01 - Track (albums)",
            NamingPreset::Custom(_) => "Custom template",
        }
    }
}

/// Full set of user choices for one download job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadOptions {
    pub kind: DownloadKind,
    pub audio_format: AudioFormat,
    pub audio_quality: AudioQuality,
    pub video_quality: VideoQuality,
    pub video_container: VideoContainer,

    pub embed_metadata: bool,
    pub embed_thumbnail: bool,
    pub embed_chapters: bool,

    /// When true, treat a playlist/album URL as a whole; when false, grab only
    /// the single video (`--no-playlist`).
    pub download_playlist: bool,
    /// Optional `--playlist-items` selector, e.g. "1-5,8".
    pub playlist_items: Option<String>,

    pub naming: NamingPreset,
    pub output_dir: PathBuf,

    /// Sanitise filenames to ASCII / Windows-safe. Helps portability.
    pub restrict_filenames: bool,
    /// Number of concurrent fragment downloads (`-N`).
    pub concurrent_fragments: u32,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        DownloadOptions {
            kind: DownloadKind::AudioOnly,
            audio_format: AudioFormat::Mp3,
            audio_quality: AudioQuality::Best,
            video_quality: VideoQuality::P1080,
            video_container: VideoContainer::Mp4,
            embed_metadata: true,
            embed_thumbnail: true,
            embed_chapters: false,
            download_playlist: true,
            playlist_items: None,
            naming: NamingPreset::PlaylistIndexTitle,
            output_dir: default_music_dir(),
            restrict_filenames: false,
            concurrent_fragments: 4,
        }
    }
}

/// Marker prefix for machine-parseable progress lines emitted by yt-dlp.
pub const PROGRESS_PREFIX: &str = "CATCHYT_PROG";

/// The `--progress-template` value. Fields are pipe-separated after the prefix:
/// percent | speed | eta | downloaded | total | queue index | item count | title
pub fn progress_template() -> String {
    format!(
        "download:{PROGRESS_PREFIX}|%(progress._percent_str)s|%(progress._speed_str)s|\
%(progress._eta_str)s|%(progress._downloaded_bytes_str)s|%(progress._total_bytes_str)s|\
%(info.playlist_autonumber)s|%(info.n_entries)s|%(info.title)s"
    )
}

impl DownloadOptions {
    /// Build the full yt-dlp argument vector (excluding the URL, which the
    /// runner appends last). `ffmpeg_dir` points at the directory containing the
    /// bundled ffmpeg/ffprobe binaries.
    pub fn build_args(&self, ffmpeg_dir: &Path) -> Vec<String> {
        // Ignore any user/global config so behaviour is deterministic.
        let deno = ffmpeg_dir.join(if cfg!(windows) { "deno.exe" } else { "deno" });
        let mut a: Vec<String> = vec![
            "--ignore-config".into(),
            "--no-abort-on-error".into(),
            // Keep pipe output deterministic on Windows. Without this, yt-dlp
            // may emit CP1252 and break a strict UTF-8 reader.
            "--encoding".into(),
            "utf-8".into(),
            // Use only the Deno executable managed by CatchYT. The official
            // yt-dlp.exe already bundles the matching EJS solver scripts.
            "--no-js-runtimes".into(),
            "--js-runtimes".into(),
            format!("deno:{}", deno.to_string_lossy()),
            "--no-remote-components".into(),
        ];

        // Point yt-dlp at our bundled ffmpeg so no system install is required.
        a.push("--ffmpeg-location".into());
        a.push(ffmpeg_dir.to_string_lossy().into_owned());

        // Concurrency.
        a.push("-N".into());
        a.push(self.concurrent_fragments.max(1).to_string());

        // Playlist handling.
        if self.download_playlist {
            a.push("--yes-playlist".into());
        } else {
            a.push("--no-playlist".into());
        }
        if let Some(items) = self
            .playlist_items
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            a.push("--playlist-items".into());
            a.push(items.to_string());
        }

        // Format selection.
        match self.kind {
            DownloadKind::AudioOnly => {
                a.push("-x".into()); // --extract-audio
                if let Some(fmt) = self.audio_format.yt_dlp_token() {
                    a.push("--audio-format".into());
                    a.push(fmt.into());
                }
                // Quality only matters for lossy re-encodes.
                if !self.audio_format.is_lossless() && self.audio_format != AudioFormat::Best {
                    a.push("--audio-quality".into());
                    a.push(self.audio_quality.yt_dlp_token());
                }
            }
            DownloadKind::Video => {
                a.push("-f".into());
                a.push(self.video_format_selector());
                a.push("--merge-output-format".into());
                a.push(self.video_container.token().into());
            }
        }

        // Precompute the zero-padded track prefix used by the naming presets,
        // from trustworthy numeric fields (native track number, else playlist
        // position). This MUST run before the track-tag mapping below: that
        // mapping rewrites track_number as a string, and applying an integer
        // format to a string makes the whole template render "NA".
        a.push("--parse-metadata".into());
        a.push("%(track_number,playlist_index|)02d:%(catchyt_track_prefix)s".into());
        // A non-integer source still renders the literal "NA"; strip it so
        // the filename template falls back to the plain title instead.
        a.push("--replace-in-metadata".into());
        a.push("catchyt_track_prefix".into());
        a.push("^NA$".into());
        a.push("".into());

        // Metadata / embedding.
        if self.embed_metadata {
            // Preserve a native album track number; otherwise use playlist
            // order so music libraries still receive a useful track tag.
            a.push("--parse-metadata".into());
            a.push("%(track_number,playlist_index|)s:%(track_number)s".into());
            a.push("--embed-metadata".into());
        }
        if self.embed_thumbnail {
            a.push("--embed-thumbnail".into());
        }
        if self.embed_chapters {
            a.push("--embed-chapters".into());
        }

        // Filenames.
        if self.restrict_filenames {
            a.push("--restrict-filenames".into());
        }
        a.push("--trim-filenames".into());
        a.push("180".into());

        // Output location + template.
        a.push("-P".into());
        a.push(self.output_dir.to_string_lossy().into_owned());
        a.push("-o".into());
        a.push(self.naming.template());
        if self.embed_thumbnail {
            // Intermediate thumbnail names never need user-controlled titles.
            a.push("-o".into());
            a.push("thumbnail:%(id)s.%(ext)s".into());
        }

        // Machine-readable progress.
        a.push("--newline".into());
        a.push("--progress-template".into());
        a.push(progress_template());
        // Emit one JSON line per completed item so the UI can log final paths.
        a.push("--no-simulate".into());

        a
    }

    /// yt-dlp format selector string for the chosen max height.
    pub fn video_format_selector(&self) -> String {
        match self.video_quality.max_height() {
            None => "bv*+ba/b".to_string(),
            Some(h) => format!("bv*[height<={h}]+ba/b[height<={h}]/b[height<={h}]/bv*+ba/b"),
        }
    }
}

/// True when a custom output template can write outside the chosen output
/// directory: absolute path, drive letter, or `..` traversal. yt-dlp joins
/// relative templates under `-P`, but absolute ones replace it entirely. The
/// user only affects their own machine, so the UI warns rather than blocks.
pub fn template_escapes_output_dir(template: &str) -> bool {
    let t = template.trim();
    t.starts_with('/') || t.starts_with('\\') || t.contains("..") || t.chars().nth(1) == Some(':')
}

/// Best-effort default output directory (~/Music, falling back to home).
pub fn default_music_dir() -> PathBuf {
    if let Some(dirs) = directories::UserDirs::new() {
        if let Some(audio) = dirs.audio_dir() {
            return audio.to_path_buf();
        }
        return dirs.home_dir().to_path_buf();
    }
    PathBuf::from(".")
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ff() -> PathBuf {
        PathBuf::from("/opt/ffmpeg")
    }

    fn window(args: &[String], flag: &str) -> Option<String> {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    }

    #[test]
    fn audio_mp3_has_extract_and_format() {
        let mut o = DownloadOptions::default();
        o.kind = DownloadKind::AudioOnly;
        o.audio_format = AudioFormat::Mp3;
        o.audio_quality = AudioQuality::Kbps(320);
        let args = o.build_args(&ff());
        assert!(args.contains(&"-x".to_string()));
        assert_eq!(window(&args, "--audio-format").as_deref(), Some("mp3"));
        assert_eq!(window(&args, "--audio-quality").as_deref(), Some("320K"));
    }

    #[test]
    fn flac_ignores_quality() {
        let mut o = DownloadOptions::default();
        o.audio_format = AudioFormat::Flac;
        o.audio_quality = AudioQuality::Kbps(320);
        let args = o.build_args(&ff());
        assert_eq!(window(&args, "--audio-format").as_deref(), Some("flac"));
        assert!(
            !args.contains(&"--audio-quality".to_string()),
            "lossless formats must not pass a bitrate"
        );
    }

    #[test]
    fn best_audio_does_not_recode() {
        let mut o = DownloadOptions::default();
        o.audio_format = AudioFormat::Best;
        let args = o.build_args(&ff());
        assert!(args.contains(&"-x".to_string()));
        assert!(!args.contains(&"--audio-format".to_string()));
        assert!(!args.contains(&"--audio-quality".to_string()));
    }

    #[test]
    fn video_selector_respects_height() {
        let mut o = DownloadOptions::default();
        o.kind = DownloadKind::Video;
        o.video_quality = VideoQuality::P720;
        o.video_container = VideoContainer::Mkv;
        let args = o.build_args(&ff());
        let sel = window(&args, "-f").unwrap();
        assert!(sel.contains("height<=720"), "selector was {sel}");
        assert_eq!(
            window(&args, "--merge-output-format").as_deref(),
            Some("mkv")
        );
    }

    #[test]
    fn playlist_toggle() {
        let mut o = DownloadOptions::default();
        o.download_playlist = false;
        let args = o.build_args(&ff());
        assert!(args.contains(&"--no-playlist".to_string()));

        o.download_playlist = true;
        o.playlist_items = Some(" 1-3,7 ".to_string());
        let args = o.build_args(&ff());
        assert!(args.contains(&"--yes-playlist".to_string()));
        assert_eq!(window(&args, "--playlist-items").as_deref(), Some("1-3,7"));
    }

    #[test]
    fn empty_playlist_items_omitted() {
        let mut o = DownloadOptions::default();
        o.playlist_items = Some("   ".to_string());
        let args = o.build_args(&ff());
        assert!(!args.contains(&"--playlist-items".to_string()));
    }

    #[test]
    fn metadata_flags_present() {
        let mut o = DownloadOptions::default();
        o.embed_metadata = true;
        o.embed_thumbnail = true;
        o.embed_chapters = true;
        let args = o.build_args(&ff());
        assert!(args.contains(&"--embed-metadata".to_string()));
        assert!(args.contains(&"--embed-thumbnail".to_string()));
        assert!(args.contains(&"--embed-chapters".to_string()));
        assert!(
            args.contains(&"%(track_number,playlist_index|)s:%(track_number)s".to_string()),
            "track tag mapping must be present when metadata embedding is on"
        );
        assert!(args
            .windows(2)
            .any(|pair| { pair == ["-o".to_string(), "thumbnail:%(id)s.%(ext)s".to_string()] }));
    }

    #[test]
    fn disabling_metadata_omits_track_number_mapping() {
        let mut o = DownloadOptions::default();
        o.embed_metadata = false;
        let args = o.build_args(&ff());
        assert!(!args.contains(&"--embed-metadata".to_string()));
        assert!(!args.contains(&"%(track_number,playlist_index|)s:%(track_number)s".to_string()));
    }

    #[test]
    fn track_prefix_is_computed_before_the_track_tag_mapping() {
        // The tag mapping rewrites track_number as a string; the prefix must
        // therefore be derived from the still-numeric fields first, otherwise
        // integer formatting fails and filenames start with "NA".
        let args = DownloadOptions::default().build_args(&ff());
        let prefix_pos = args
            .iter()
            .position(|a| a == "%(track_number,playlist_index|)02d:%(catchyt_track_prefix)s")
            .expect("prefix parse-metadata present");
        let tag_pos = args
            .iter()
            .position(|a| a == "%(track_number,playlist_index|)s:%(track_number)s")
            .expect("track tag mapping present");
        assert!(prefix_pos < tag_pos);

        // The "NA" guard follows the prefix computation.
        let replace_pos = args
            .iter()
            .position(|a| a == "--replace-in-metadata")
            .expect("replace-in-metadata present");
        assert_eq!(args[replace_pos + 1], "catchyt_track_prefix");
        assert_eq!(args[replace_pos + 2], "^NA$");
        assert_eq!(args[replace_pos + 3], "");
    }

    #[test]
    fn track_prefix_survives_disabled_metadata_embedding() {
        // Naming presets rely on the prefix even when tags are not embedded.
        let mut o = DownloadOptions::default();
        o.embed_metadata = false;
        let args = o.build_args(&ff());
        assert!(args
            .contains(&"%(track_number,playlist_index|)02d:%(catchyt_track_prefix)s".to_string()));
    }

    #[test]
    fn empty_custom_template_falls_back_to_title() {
        assert_eq!(
            NamingPreset::Custom("   ".to_string()).template(),
            "%(title)s.%(ext)s"
        );
        assert_eq!(
            NamingPreset::Custom(String::new()).template(),
            "%(title)s.%(ext)s"
        );
    }

    #[test]
    fn escape_detection_flags_absolute_and_traversal_templates() {
        assert!(template_escapes_output_dir("C:/dump/%(title)s.%(ext)s"));
        assert!(template_escapes_output_dir("/tmp/%(title)s.%(ext)s"));
        assert!(template_escapes_output_dir(
            r"\\server\share\%(title)s.%(ext)s"
        ));
        assert!(template_escapes_output_dir("../%(title)s.%(ext)s"));
        assert!(template_escapes_output_dir("  ../up/%(title)s.%(ext)s  "));

        assert!(!template_escapes_output_dir("%(title)s.%(ext)s"));
        assert!(!template_escapes_output_dir("%(album)s/%(title)s.%(ext)s"));
        assert!(!template_escapes_output_dir(""));
    }

    #[test]
    fn naming_album_template() {
        let mut o = DownloadOptions::default();
        o.naming = NamingPreset::AlbumTrack;
        let args = o.build_args(&ff());
        assert_eq!(
            window(&args, "-o").as_deref(),
            Some("%(album,playlist|Album)s/%(catchyt_track_prefix&{} - |)s%(track,title)s.%(ext)s")
        );
    }

    #[test]
    fn default_naming_adds_a_number_only_when_available() {
        let options = DownloadOptions::default();
        assert_eq!(options.naming, NamingPreset::PlaylistIndexTitle);
        assert_eq!(
            options.naming.template(),
            "%(catchyt_track_prefix&{} - |)s%(track,title)s.%(ext)s"
        );
    }

    #[test]
    fn deterministic_encoding_runtime_and_filename_guards_are_present() {
        let args = DownloadOptions::default().build_args(&ff());
        assert_eq!(window(&args, "--encoding").as_deref(), Some("utf-8"));
        let expected_deno = format!(
            "deno:{}",
            ff().join(if cfg!(windows) { "deno.exe" } else { "deno" })
                .to_string_lossy()
        );
        assert_eq!(
            window(&args, "--js-runtimes").as_deref(),
            Some(expected_deno.as_str())
        );
        assert!(args.contains(&"--no-js-runtimes".to_string()));
        assert!(args.contains(&"--no-remote-components".to_string()));
        assert_eq!(window(&args, "--trim-filenames").as_deref(), Some("180"));
    }

    #[test]
    fn progress_template_carries_global_playlist_position() {
        let template = progress_template();
        assert!(template.contains("%(info.playlist_autonumber)s"));
        assert!(template.contains("%(info.n_entries)s"));
        assert!(template.ends_with("%(info.title)s"));
    }

    #[test]
    fn ffmpeg_location_always_passed() {
        let o = DownloadOptions::default();
        let args = o.build_args(&ff());
        assert_eq!(
            window(&args, "--ffmpeg-location").as_deref(),
            Some("/opt/ffmpeg")
        );
    }
}

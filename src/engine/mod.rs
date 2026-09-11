//! The download engine: everything that drives yt-dlp/ffmpeg, kept free of any
//! UI concerns so it can be tested on its own.

pub mod deps;
pub mod options;
pub mod probe;
pub mod runner;

pub use deps::{ensure_dependencies, resolve_existing, BootstrapProgress, BootstrapStage, Deps};
pub use options::{
    AudioFormat, AudioQuality, DownloadKind, DownloadOptions, NamingPreset, VideoContainer,
    VideoQuality,
};
pub use probe::{looks_like_supported_url, probe, MediaInfo};
pub use runner::{parse_progress_line, spawn, Event, JobHandle, Progress};

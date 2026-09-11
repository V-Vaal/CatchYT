//! Integration tests against the public engine API.
//!
//! The pure tests always run. The end-to-end tests are marked `#[ignore]` and
//! only run when explicitly requested (`cargo test -- --ignored`) AND network
//! access is available — CI runs them on the Windows job.
//!
//! NOTE: this directory doubles as a Cargo test root; do not point CatchYT's
//! output folder at it when testing the app by hand.

#![allow(clippy::field_reassign_with_default)]

use catchyt::engine::options::{
    AudioFormat, AudioQuality, DownloadKind, DownloadOptions, NamingPreset,
};
use catchyt::engine::runner::parse_progress_line;
use std::path::PathBuf;

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[test]
fn public_api_builds_expected_mp3_args() {
    let mut o = DownloadOptions::default();
    o.kind = DownloadKind::AudioOnly;
    o.audio_format = AudioFormat::Mp3;
    o.audio_quality = AudioQuality::Kbps(320);
    o.naming = NamingPreset::AlbumTrack;
    let args = o.build_args(&PathBuf::from("/tmp/ff"));

    assert!(args.contains(&"-x".to_string()));
    assert_eq!(arg_after(&args, "--audio-format").as_deref(), Some("mp3"));
    assert_eq!(arg_after(&args, "--audio-quality").as_deref(), Some("320K"));
    assert_eq!(
        arg_after(&args, "-o").as_deref(),
        Some("%(album,playlist|Album)s/%(catchyt_track_prefix&{} - |)s%(track,title)s.%(ext)s")
    );
    assert_eq!(
        arg_after(&args, "--ffmpeg-location").as_deref(),
        Some("/tmp/ff")
    );
    // The naming presets depend on the precomputed, integer-safe track prefix.
    assert!(
        args.contains(&"%(track_number,playlist_index|)02d:%(catchyt_track_prefix)s".to_string())
    );
}

#[test]
fn progress_round_trips_through_parser() {
    let line = "CATCHYT_PROG| 73.0%|2.5MiB/s|00:03|7.3MiB|10.0MiB|3|13|Track Name";
    let p = parse_progress_line(line).expect("parse");
    assert!((p.percent - 73.0).abs() < 0.01);
    assert_eq!(p.item_index, Some(3));
    assert_eq!(p.item_count, Some(13));
    assert_eq!(p.title, "Track Name");

    let overall = p.overall_fraction().expect("playlist context");
    assert!((overall - (2.0 + 0.73) / 13.0).abs() < 0.001);
}

#[test]
fn single_item_progress_has_no_overall_fraction() {
    let line = "CATCHYT_PROG| 50.0%|1.0MiB/s|00:10|1.0MiB|2.0MiB|NA|NA|Solo Track";
    let p = parse_progress_line(line).expect("parse");
    assert_eq!(p.item_index, None);
    assert_eq!(p.item_count, None);
    assert!(p.overall_fraction().is_none());
}

/// Real first-run bootstrap without contacting YouTube. Enable with:
///   CATCHYT_BOOTSTRAP_E2E=1 cargo test -- --ignored e2e_bootstraps_dependencies
#[test]
#[ignore]
fn e2e_bootstraps_dependencies() {
    if std::env::var("CATCHYT_BOOTSTRAP_E2E").is_err() {
        eprintln!("skipping bootstrap e2e (set CATCHYT_BOOTSTRAP_E2E=1 to run)");
        return;
    }

    use catchyt::engine::{self, BootstrapProgress};
    let mut progress = |_event: BootstrapProgress| {};
    let deps = engine::ensure_dependencies(&mut progress).expect("bootstrap dependencies");

    assert!(deps.all_present());
    assert!(deps.ytdlp.is_file());
    assert!(deps.ffmpeg.is_file());
    assert!(deps.ffprobe.is_file());
    assert!(deps.deno.is_file());
}

/// Real download smoke test. Requires network access. Enable with:
///   CATCHYT_E2E=1 cargo test -- --ignored e2e_downloads_audio
#[test]
#[ignore]
fn e2e_downloads_audio() {
    if std::env::var("CATCHYT_E2E").is_err() {
        eprintln!("skipping e2e (set CATCHYT_E2E=1 to run)");
        return;
    }
    use catchyt::engine::{self, Event};
    use crossbeam_channel::unbounded;

    // Resolve/bootstrap dependencies.
    let mut noop = |_p: engine::deps::BootstrapProgress| {};
    let deps = engine::ensure_dependencies(&mut noop).expect("bootstrap deps");

    let tmp = std::env::temp_dir().join("catchyt_e2e");
    let _ = std::fs::create_dir_all(&tmp);

    let mut o = DownloadOptions::default();
    o.kind = DownloadKind::AudioOnly;
    o.audio_format = AudioFormat::Mp3;
    o.audio_quality = AudioQuality::Kbps(128);
    o.download_playlist = false;
    o.output_dir = tmp.clone();

    // "me at the zoo" — the first, short, stable YouTube video.
    let url = "https://www.youtube.com/watch?v=jNQXAC9IVRw";
    let (tx, rx) = unbounded::<Event>();
    let mut job = engine::spawn(&deps.ytdlp, &deps.bin_dir, &o, url, tx);

    let mut success = false;
    while let Ok(ev) = rx.recv() {
        if let Event::Finished { success: s, .. } = ev {
            success = s;
            break;
        }
    }
    job.join();

    assert!(success, "download job did not finish successfully");
    let produced = std::fs::read_dir(&tmp)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().map(|x| x == "mp3").unwrap_or(false));
    assert!(produced, "no .mp3 produced in {}", tmp.display());
}

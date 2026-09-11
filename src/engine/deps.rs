//! First-run dependency bootstrap.
//!
//! Rather than bundling `yt-dlp.exe`, `ffmpeg.exe`, and `deno.exe` inside our binary (which
//! materially raises the odds of antivirus heuristic false positives, because
//! yt-dlp.exe is itself a PyInstaller build), CatchYT downloads them on first
//! launch from their official release hosts into a per-user directory:
//!
//!   %LOCALAPPDATA%\CatchYT\bin\
//!
//! Subsequent launches reuse the cached binaries.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Official download endpoints. All are served from GitHub releases.
const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
/// Checksums published by the same yt-dlp release (sha256sum format).
const YTDLP_SUMS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";
const YTDLP_SUMS_NAME: &str = "yt-dlp.exe";
const FFMPEG_ZIP_URL: &str =
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
const FFMPEG_ZIP_NAME: &str = "ffmpeg-master-latest-win64-gpl.zip";
/// Checksum sidecar published next to the BtbN archive.
const FFMPEG_SHA256_URL: &str =
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip.sha256";
pub const DENO_VERSION: &str = "2.9.2";
const DENO_ZIP_URL: &str =
    "https://github.com/denoland/deno/releases/download/v2.9.2/deno-x86_64-pc-windows-msvc.zip";
const DENO_ZIP_SHA256: &str = "5fe194d26ac5ef77fcc5288c2c438c7a0465f3b6180440ebf04092714bf2dcdf";
const FFMPEG_INSTALL_MARKER: &str = ".ffmpeg-install-complete";
const DENO_INSTALL_MARKER: &str = ".deno-2.9.2-install-complete";

/// Resolved paths to the tools CatchYT drives.
#[derive(Debug, Clone)]
pub struct Deps {
    /// Directory holding the binaries (also passed to yt-dlp as --ffmpeg-location).
    pub bin_dir: PathBuf,
    pub ytdlp: PathBuf,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub deno: PathBuf,
}

/// A phase of the first-run bootstrap. The UI owns the human-facing wording
/// (and its translation); the engine only reports which step is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStage {
    DownloadingYtdlp,
    DownloadingFfmpeg,
    ExtractingFfmpeg,
    DownloadingDeno,
    ExtractingDeno,
}

/// A coarse-grained progress signal for the bootstrap UI.
#[derive(Debug, Clone)]
pub enum BootstrapProgress {
    Stage(BootstrapStage),
    /// Byte-level progress for the file currently downloading.
    Fraction {
        label: String,
        /// 0.0..=1.0, or None when indeterminate (unknown content length).
        fraction: Option<f32>,
        /// Instantaneous download speed in bytes/sec, once measurable.
        speed_bps: Option<f64>,
        /// Estimated time remaining, once speed and total size are known.
        eta_secs: Option<f64>,
    },
    Done,
}

/// Returns the per-user binary directory, creating it if needed.
pub fn bin_dir() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow!("cannot resolve local app data directory"))?;
    let dir = base.data_local_dir().join("CatchYT").join("bin");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    migrate_legacy_ytdlp(&dir)?;
    Ok(dir)
}

/// Older builds used ProjectDirs with duplicated organization/application
/// names, which produced `%LOCALAPPDATA%\CatchYT\CatchYT\data\bin`. Preserve a
/// cached yt-dlp when moving to the documented `%LOCALAPPDATA%\CatchYT\bin`.
fn migrate_legacy_ytdlp(dir: &Path) -> Result<()> {
    let Some(project_dirs) = directories::ProjectDirs::from("dev", "CatchYT", "CatchYT") else {
        return Ok(());
    };
    let legacy = project_dirs.data_local_dir().join("bin");
    if legacy == dir {
        return Ok(());
    }

    let name = exe_name("yt-dlp");
    let source = legacy.join(&name);
    let dest = dir.join(&name);
    if source.is_file() && !dest.exists() {
        let staging = dir.join(format!("{name}.migration.part"));
        let result = (|| -> Result<()> {
            fs::copy(&source, &staging)
                .with_context(|| format!("migrating {} to {}", source.display(), dest.display()))?;
            publish_file(&staging, &dest)?;
            let _ = fs::remove_file(&source);
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staging);
        }
        result?;
    }
    Ok(())
}

fn exe_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

/// Inspect the cache and report which tools are already present.
pub fn resolve_existing() -> Result<Deps> {
    let bin = bin_dir()?;
    Ok(Deps {
        ytdlp: bin.join(exe_name("yt-dlp")),
        ffmpeg: bin.join(exe_name("ffmpeg")),
        ffprobe: bin.join(exe_name("ffprobe")),
        deno: bin.join(exe_name("deno")),
        bin_dir: bin,
    })
}

impl Deps {
    fn ffmpeg_present(&self) -> bool {
        self.ffmpeg.is_file()
            && self.ffprobe.is_file()
            && self.bin_dir.join(FFMPEG_INSTALL_MARKER).is_file()
    }

    fn deno_present(&self) -> bool {
        self.deno.is_file() && self.bin_dir.join(DENO_INSTALL_MARKER).is_file()
    }

    pub fn all_present(&self) -> bool {
        self.ytdlp.is_file() && self.ffmpeg_present() && self.deno_present()
    }
}

/// Ensure all dependencies exist, downloading whatever is missing. `progress`
/// is invoked with human-facing status updates; it must be cheap.
pub fn ensure_dependencies(progress: &mut dyn FnMut(BootstrapProgress)) -> Result<Deps> {
    let deps = resolve_existing()?;

    if !deps.ytdlp.is_file() {
        progress(BootstrapProgress::Stage(BootstrapStage::DownloadingYtdlp));
        let expected = fetch_expected_sha256(YTDLP_SUMS_URL, YTDLP_SUMS_NAME)
            .context("fetching yt-dlp checksums")?;
        download_to_file(YTDLP_URL, &deps.ytdlp, "yt-dlp", Some(&expected), progress)
            .context("downloading yt-dlp")?;
        mark_executable(&deps.ytdlp)?;
    }

    if !deps.ffmpeg_present() {
        progress(BootstrapProgress::Stage(BootstrapStage::DownloadingFfmpeg));
        let tmp_zip = deps.bin_dir.join("ffmpeg-download.zip");
        let marker = deps.bin_dir.join(FFMPEG_INSTALL_MARKER);
        let _ = fs::remove_file(&marker);

        let install_result = (|| -> Result<()> {
            let expected = fetch_expected_sha256(FFMPEG_SHA256_URL, FFMPEG_ZIP_NAME)
                .context("fetching ffmpeg checksum")?;
            download_to_file(
                FFMPEG_ZIP_URL,
                &tmp_zip,
                "ffmpeg",
                Some(&expected),
                progress,
            )
            .context("downloading ffmpeg archive")?;
            progress(BootstrapProgress::Stage(BootstrapStage::ExtractingFfmpeg));
            extract_ffmpeg(&tmp_zip, &deps.bin_dir).context("extracting ffmpeg")?;
            mark_executable(&deps.ffmpeg)?;
            mark_executable(&deps.ffprobe)?;
            write_install_marker(&deps.bin_dir, FFMPEG_INSTALL_MARKER)?;
            Ok(())
        })();
        let _ = fs::remove_file(&tmp_zip);
        install_result?;
    }

    if !deps.deno_present() {
        progress(BootstrapProgress::Stage(BootstrapStage::DownloadingDeno));
        let tmp_zip = deps.bin_dir.join("deno-download.zip");
        let marker = deps.bin_dir.join(DENO_INSTALL_MARKER);
        let _ = fs::remove_file(&marker);

        let install_result = (|| -> Result<()> {
            download_to_file(
                DENO_ZIP_URL,
                &tmp_zip,
                "deno",
                Some(DENO_ZIP_SHA256),
                progress,
            )
            .context("downloading Deno archive")?;
            progress(BootstrapProgress::Stage(BootstrapStage::ExtractingDeno));
            extract_deno(&tmp_zip, &deps.bin_dir).context("extracting Deno")?;
            mark_executable(&deps.deno)?;
            write_install_marker(&deps.bin_dir, DENO_INSTALL_MARKER)?;
            Ok(())
        })();
        let _ = fs::remove_file(&tmp_zip);
        install_result?;
    }

    if !deps.all_present() {
        return Err(anyhow!(
            "dependency bootstrap finished but some binaries are still missing"
        ));
    }

    progress(BootstrapProgress::Done);
    Ok(deps)
}

fn http_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("CatchYT/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(30))
        // For reqwest's blocking client this bounds each I/O wait, not the
        // complete transfer, so slow but progressing downloads can finish.
        .timeout(std::time::Duration::from_secs(600))
        .build()?)
}

/// Fetch the checksum file published next to a release artifact and return
/// the SHA-256 expected for `filename`. yt-dlp and BtbN both serve moving
/// `latest` tags: fetching the sums first means a release landing mid-download
/// yields a mismatch that fails closed — retrying the bootstrap resolves it.
fn fetch_expected_sha256(sums_url: &str, filename: &str) -> Result<String> {
    let listing = http_client()?
        .get(sums_url)
        .send()?
        .error_for_status()
        .with_context(|| format!("HTTP request for {sums_url}"))?
        .text()?;
    parse_sha256_for(&listing, filename)
        .ok_or_else(|| anyhow!("no SHA-256 entry for {filename} in {sums_url}"))
}

/// Parse `sha256sum`-style listings: one `<hex64> <name>` pair per line, with
/// binary-mode `*` prefixes tolerated. A file made of a single bare hash — the
/// BtbN `.sha256` sidecar format — is also accepted.
fn parse_sha256_for(listing: &str, filename: &str) -> Option<String> {
    let mut bare_hash: Option<String> = None;
    let mut line_count = 0usize;
    for line in listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        line_count += 1;
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else { continue };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        match parts.next() {
            Some(name) if name.trim_start_matches('*').eq_ignore_ascii_case(filename) => {
                return Some(hash.to_ascii_lowercase());
            }
            None => bare_hash = Some(hash.to_ascii_lowercase()),
            Some(_) => {}
        }
    }
    // A bare hash is only trustworthy when it is the whole file.
    if line_count == 1 {
        bare_hash
    } else {
        None
    }
}

/// Stream an HTTP GET to a file, reporting byte progress.
fn download_to_file(
    url: &str,
    dest: &Path,
    label: &str,
    expected_sha256: Option<&str>,
    progress: &mut dyn FnMut(BootstrapProgress),
) -> Result<()> {
    let tmp = dest.with_extension("part");
    let _ = fs::remove_file(&tmp);

    let result = (|| -> Result<()> {
        let client = http_client()?;

        let mut resp = client
            .get(url)
            .send()?
            .error_for_status()
            .with_context(|| format!("HTTP request for {url}"))?;

        let total = resp.content_length();
        let mut out = fs::File::create(&tmp)?;
        let mut buf = [0u8; 64 * 1024];
        let mut downloaded: u64 = 0;
        let start = std::time::Instant::now();

        loop {
            let n = resp.read(&mut buf)?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut out, &buf[..n])?;
            downloaded += n as u64;
            let frac = total.map(|t| (downloaded as f32 / t as f32).clamp(0.0, 1.0));

            let elapsed = start.elapsed().as_secs_f64();
            let speed_bps = (elapsed > 0.2).then(|| downloaded as f64 / elapsed);
            let eta_secs = match (total, speed_bps) {
                (Some(t), Some(s)) if s > 0.0 => Some(t.saturating_sub(downloaded) as f64 / s),
                _ => None,
            };

            progress(BootstrapProgress::Fraction {
                label: label.to_string(),
                fraction: frac,
                speed_bps,
                eta_secs,
            });
        }
        out.sync_all()?;
        drop(out);
        if let Some(expected) = expected_sha256 {
            verify_sha256(&tmp, expected)?;
        }
        publish_file(&tmp, dest)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Publish a completely written staging file. Windows does not let
/// `fs::rename` replace an existing destination, so remove a stale destination
/// first. Final executable readiness is guarded by the install marker below.
fn publish_file(staging: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        fs::remove_file(dest).with_context(|| format!("removing stale {}", dest.display()))?;
    }
    fs::rename(staging, dest).with_context(|| format!("publishing {}", dest.display()))?;
    Ok(())
}

fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let mut file = fs::File::open(path)?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }
    let actual = context
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(anyhow!(
            "SHA-256 mismatch for {} (expected {expected}, got {actual})",
            path.display()
        ));
    }
    Ok(())
}

fn write_install_marker(bin_dir: &Path, marker_name: &str) -> Result<()> {
    let marker = bin_dir.join(marker_name);
    let staging = bin_dir.join(format!("{marker_name}.part"));
    fs::write(&staging, b"complete\n")?;
    publish_file(&staging, &marker)
}

/// Pull ffmpeg.exe / ffprobe.exe out of the BtbN archive (they live under a
/// top-level versioned folder in `bin/`).
fn extract_ffmpeg(zip_path: &Path, bin_dir: &Path) -> Result<()> {
    extract_expected_files(
        zip_path,
        bin_dir,
        &[exe_name("ffmpeg"), exe_name("ffprobe")],
        "ffmpeg",
    )
}

/// Pull the single self-contained Deno executable from its official archive.
fn extract_deno(zip_path: &Path, bin_dir: &Path) -> Result<()> {
    extract_expected_files(zip_path, bin_dir, &[exe_name("deno")], "Deno")
}

/// Extract only explicitly expected basenames. Staging files are published
/// only after every distinct expected member has been found and fully written.
fn extract_expected_files(
    zip_path: &Path,
    bin_dir: &Path,
    wanted: &[String],
    archive_label: &str,
) -> Result<()> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let staging = wanted
        .iter()
        .map(|name| bin_dir.join(format!("{name}.part")))
        .collect::<Vec<_>>();
    for path in &staging {
        let _ = fs::remove_file(path);
    }

    let result = (|| -> Result<()> {
        let mut found = vec![false; wanted.len()];

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let Some(path) = entry.enclosed_name() else {
                continue;
            };
            let Some(base) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(index) = wanted.iter().position(|name| name == base) else {
                continue;
            };
            if found[index] {
                return Err(anyhow!(
                    "{archive_label} archive contained duplicate {base}"
                ));
            }

            let mut out = fs::File::create(&staging[index])?;
            std::io::copy(&mut entry, &mut out)?;
            out.sync_all()?;
            found[index] = true;
        }

        if found.iter().any(|present| !present) {
            let count = found.iter().filter(|present| **present).count();
            return Err(anyhow!(
                "{archive_label} archive did not contain expected binaries (found {count}/{})",
                wanted.len()
            ));
        }

        for (index, name) in wanted.iter().enumerate() {
            publish_file(&staging[index], &bin_dir.join(name))?;
        }
        Ok(())
    })();

    if result.is_err() {
        for path in &staging {
            let _ = fs::remove_file(path);
        }
    }
    result
}

#[cfg(unix)]
fn mark_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn mark_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        exe_name, extract_deno, extract_ffmpeg, parse_sha256_for, publish_file, verify_sha256,
        Deps, DENO_INSTALL_MARKER, DENO_VERSION, DENO_ZIP_SHA256, DENO_ZIP_URL,
        FFMPEG_INSTALL_MARKER, FFMPEG_SHA256_URL, FFMPEG_ZIP_NAME, FFMPEG_ZIP_URL, YTDLP_SUMS_NAME,
        YTDLP_SUMS_URL, YTDLP_URL,
    };
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let unique = format!(
                "catchyt-deps-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, contents) in entries {
            archive.start_file(*name, options).unwrap();
            archive.write_all(contents).unwrap();
        }
        archive.finish().unwrap();
    }

    #[test]
    fn ffmpeg_uses_btbn_stable_latest_tag() {
        assert!(FFMPEG_ZIP_URL.contains("/releases/download/latest/"));
        assert!(!FFMPEG_ZIP_URL.contains("/releases/latest/download/"));
    }

    #[test]
    fn deno_release_is_pinned_with_a_sha256() {
        assert!(DENO_ZIP_URL.contains(&format!("/releases/download/v{DENO_VERSION}/")));
        assert_eq!(DENO_ZIP_SHA256.len(), 64);
        assert!(DENO_ZIP_SHA256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ytdlp_and_ffmpeg_checksums_come_from_the_same_release() {
        // The sums file must live in the same release directory as the binary
        // it vouches for, and name exactly the artifact we download.
        let release_dir = |url: &str| url.rsplit_once('/').map(|(base, _)| base.to_string());
        assert_eq!(release_dir(YTDLP_SUMS_URL), release_dir(YTDLP_URL));
        assert!(YTDLP_URL.ends_with(YTDLP_SUMS_NAME));
        assert_eq!(FFMPEG_SHA256_URL, format!("{FFMPEG_ZIP_URL}.sha256"));
        assert!(FFMPEG_ZIP_URL.ends_with(FFMPEG_ZIP_NAME));
    }

    #[test]
    fn parses_multi_file_sha256_listings() {
        let wanted = "A".repeat(64);
        let other = "b".repeat(64);
        let listing = format!("{other}  yt-dlp\n{wanted} *yt-dlp.exe\n{other}  yt-dlp.tar.gz\n");

        assert_eq!(
            parse_sha256_for(&listing, "yt-dlp.exe"),
            Some(wanted.to_ascii_lowercase())
        );
        assert_eq!(parse_sha256_for(&listing, "missing.exe"), None);
    }

    #[test]
    fn parses_bare_single_hash_sidecar() {
        // BtbN publishes `<archive>.sha256` files holding just the hash.
        let hash = "c".repeat(64);
        assert_eq!(
            parse_sha256_for(&format!("{hash}\n"), FFMPEG_ZIP_NAME),
            Some(hash.clone())
        );
        // A bare hash in a multi-line file is ambiguous — reject it.
        assert_eq!(
            parse_sha256_for(&format!("{hash}\n{hash}\n"), FFMPEG_ZIP_NAME),
            None
        );
    }

    #[test]
    fn rejects_malformed_sha256_entries() {
        assert_eq!(parse_sha256_for("", "yt-dlp.exe"), None);
        assert_eq!(parse_sha256_for("nothex  yt-dlp.exe", "yt-dlp.exe"), None);
        let short = "d".repeat(63);
        assert_eq!(
            parse_sha256_for(&format!("{short}  yt-dlp.exe"), "yt-dlp.exe"),
            None
        );
    }

    #[test]
    fn sha256_verification_accepts_known_content_and_rejects_mismatch() {
        let dir = TestDir::new();
        let file = dir.0.join("digest.bin");
        fs::write(&file, b"abc").unwrap();

        verify_sha256(
            &file,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .unwrap();
        assert!(verify_sha256(&file, &"0".repeat(64)).is_err());
    }

    #[test]
    fn deno_extraction_publishes_only_expected_executable() {
        let dir = TestDir::new();
        let zip = dir.0.join("deno.zip");
        let deno = exe_name("deno");
        write_zip(
            &zip,
            &[(&deno, b"deno contents"), ("README.md", b"ignored")],
        );

        extract_deno(&zip, &dir.0).unwrap();

        assert_eq!(fs::read(dir.0.join(deno)).unwrap(), b"deno contents");
        assert!(!dir.0.join("README.md").exists());
    }

    #[test]
    fn extraction_publishes_both_expected_binaries() {
        let dir = TestDir::new();
        let zip = dir.0.join("ffmpeg.zip");
        let ffmpeg_entry = format!("build/bin/{}", exe_name("ffmpeg"));
        let ffprobe_entry = format!("build/bin/{}", exe_name("ffprobe"));
        write_zip(
            &zip,
            &[
                (&ffmpeg_entry, b"ffmpeg contents"),
                (&ffprobe_entry, b"ffprobe contents"),
            ],
        );

        extract_ffmpeg(&zip, &dir.0).unwrap();

        assert_eq!(
            fs::read(dir.0.join(exe_name("ffmpeg"))).unwrap(),
            b"ffmpeg contents"
        );
        assert_eq!(
            fs::read(dir.0.join(exe_name("ffprobe"))).unwrap(),
            b"ffprobe contents"
        );
    }

    #[test]
    fn incomplete_archive_does_not_publish_partial_install() {
        let dir = TestDir::new();
        let zip = dir.0.join("ffmpeg.zip");
        let ffmpeg_entry = format!("build/bin/{}", exe_name("ffmpeg"));
        write_zip(&zip, &[(&ffmpeg_entry, b"ffmpeg contents")]);

        let error = extract_ffmpeg(&zip, &dir.0).unwrap_err().to_string();

        assert!(error.contains("found 1/2"));
        assert!(!dir.0.join(exe_name("ffmpeg")).exists());
        assert!(!dir.0.join(exe_name("ffprobe")).exists());
        assert!(!dir.0.join(format!("{}.part", exe_name("ffmpeg"))).exists());
    }

    #[test]
    fn duplicate_ffmpeg_cannot_stand_in_for_ffprobe() {
        let dir = TestDir::new();
        let zip = dir.0.join("ffmpeg.zip");
        let first = format!("first/bin/{}", exe_name("ffmpeg"));
        let second = format!("second/bin/{}", exe_name("ffmpeg"));
        write_zip(&zip, &[(&first, b"first"), (&second, b"second")]);

        let error = extract_ffmpeg(&zip, &dir.0).unwrap_err().to_string();

        assert!(error.contains("duplicate"));
        assert!(!dir.0.join(exe_name("ffmpeg")).exists());
        assert!(!dir.0.join(exe_name("ffprobe")).exists());
    }

    #[test]
    fn publish_replaces_a_stale_download_on_windows() {
        let dir = TestDir::new();
        let staging = dir.0.join("download.part");
        let dest = dir.0.join("download.zip");
        fs::write(&staging, b"fresh").unwrap();
        fs::write(&dest, b"stale").unwrap();

        publish_file(&staging, &dest).unwrap();

        assert_eq!(fs::read(dest).unwrap(), b"fresh");
        assert!(!staging.exists());
    }

    #[test]
    fn ffmpeg_requires_transaction_marker() {
        let dir = TestDir::new();
        let deps = Deps {
            bin_dir: dir.0.clone(),
            ytdlp: dir.0.join(exe_name("yt-dlp")),
            ffmpeg: dir.0.join(exe_name("ffmpeg")),
            ffprobe: dir.0.join(exe_name("ffprobe")),
            deno: dir.0.join(exe_name("deno")),
        };
        fs::write(&deps.ytdlp, b"yt-dlp").unwrap();
        fs::write(&deps.ffmpeg, b"ffmpeg").unwrap();
        fs::write(&deps.ffprobe, b"ffprobe").unwrap();
        fs::write(&deps.deno, b"deno").unwrap();
        fs::write(dir.0.join(DENO_INSTALL_MARKER), b"complete").unwrap();

        assert!(!deps.all_present());
        fs::write(dir.0.join(FFMPEG_INSTALL_MARKER), b"complete").unwrap();
        assert!(deps.all_present());
    }
}

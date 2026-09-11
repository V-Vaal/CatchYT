# CatchYT agent guide

## Project scope

CatchYT is a native Rust desktop application built with `eframe`/`egui`. It drives
`yt-dlp` and `ffmpeg`; it does not implement media extraction itself. The product
targets Windows first, while pure engine logic should remain testable on Linux CI.

Read `README.md` for product behavior and `ARCHITECTURE.md` before making structural
changes. Consult `SIGNING.md` for release or code-signing work.

## Repository map

- `src/app.rs`: egui state and UI orchestration.
- `src/theme.rs`: visual constants and styling.
- `src/engine/options.rs`: validated options and `yt-dlp` argument construction.
- `src/engine/deps.rs`: first-run download and extraction of `yt-dlp`/`ffmpeg`.
- `src/engine/probe.rs`: metadata probing.
- `src/engine/runner.rs`: child-process execution and progress parsing.
- `tests/engine_integration.rs`: integration tests; the real download test is opt-in.
- `build.rs`, `build.ps1`, `.github/workflows/build.yml`: Windows build and release.

Keep UI concerns in `app.rs` and testable command/process logic in `engine/`. Never
block the egui render thread with HTTP, filesystem-heavy, or child-process work.

## Commands and RTK

Use RTK for supported high-volume commands to keep output compact:

```powershell
rtk cargo test --all
rtk cargo build
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk git status
rtk git diff
rtk grep "pattern" src tests
rtk read src/engine/options.rs
rtk ls .
```

RTK is an output proxy, not a shell. For PowerShell cmdlets, scripts, pipelines, or
unsupported commands, invoke PowerShell directly rather than forcing an RTK prefix.
Use `rtk proxy <command>` only when passthrough tracking is useful.

## Required validation

Run the smallest relevant tests while iterating. Before handing off code changes,
run all of the following unless the task is documentation-only:

```powershell
cargo fmt --all -- --check
rtk cargo test --all
rtk cargo clippy --all-targets --all-features -- -D warnings
```

For release/build-system changes, also run `rtk cargo build --release` on Windows.
The ignored E2E test performs a real network download and is not part of routine
validation. Run it only when explicitly relevant:

```powershell
$env:CATCHYT_E2E = "1"
rtk cargo test -- --ignored e2e_downloads_audio
```

## Behavioral and safety constraints

- Preserve argument boundaries: pass `yt-dlp` arguments as discrete `Command`
  arguments; do not construct a shell command string from user input.
- Treat URLs, output templates, playlist selectors, filenames, archive members, and
  subprocess output as untrusted data.
- Keep downloads on HTTPS and restricted to documented official upstream release
  endpoints. Changes to download URLs, archive extraction, update behavior, or
  executable launching require targeted tests and explicit security consideration.
- Write downloads to temporary/partial files and publish them atomically only after
  successful completion. Do not leave a partial executable at its final path.
- Prevent archive path traversal; extract only explicitly expected basenames.
- Preserve the no-console behavior of Windows release builds and avoid bundling
  third-party executables into `catchyt.exe` unless the product decision changes.
- Do not weaken authoritative tests. Formatting, Clippy (`-D warnings`), and tests
  are all blocking in CI; run them locally before handing off.
- All user-facing copy lives in `src/i18n.rs`, which holds one complete table per
  language (French default, English). Never hard-code UI strings in `app.rs`; add
  every new string to both tables. Keep identifiers, code comments, and technical
  docs clear and consistent with the existing English codebase.

## Change discipline

Do not edit generated files under `target/` or commit local binaries such as
`catchyt.exe`. Preserve unrelated user changes. Update tests and relevant docs when
behavior, supported formats, dependencies, architecture, or release steps change.

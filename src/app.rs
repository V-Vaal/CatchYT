//! The CatchYT desktop application (egui/eframe front-end).

use crate::engine::{
    self, deps::BootstrapProgress, AudioFormat, AudioQuality, BootstrapStage, Deps, DownloadKind,
    DownloadOptions, Event, JobHandle, MediaInfo, NamingPreset, Progress, VideoContainer,
    VideoQuality,
};
use crate::i18n::{Lang, Tr};
use crate::settings::Settings;
use crate::theme;
use crossbeam_channel::Receiver;
use egui::{Align, Layout, RichText};

/// Messages from the first-run bootstrap thread.
enum BootMsg {
    Progress(BootstrapProgress),
    Finished(Result<Deps, String>),
}

/// Messages from the probe (metadata preview) thread.
enum ProbeMsg {
    Ok(MediaInfo),
    Err(String),
}

#[derive(PartialEq)]
enum Phase {
    Bootstrapping,
    Ready,
    BootstrapFailed(String),
}

enum JobState {
    Idle,
    Running,
    Done { success: bool, code: Option<i32> },
    Cancelled,
}

/// Log severity, classified once when the line is ingested (the engine marks
/// errors, warnings are detected here). The UI only maps levels to colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogLevel {
    Info,
    Warn,
    Error,
}

struct LogLine {
    level: LogLevel,
    text: String,
}

/// Watchdog for a probe whose yt-dlp process hangs without producing output.
/// yt-dlp itself runs with `--socket-timeout`, so this only fires on a
/// pathological non-network hang.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

pub struct CatchYtApp {
    phase: Phase,
    deps: Option<Deps>,
    lang: Lang,

    // bootstrap
    boot_rx: Receiver<BootMsg>,
    boot_stage: Option<BootstrapStage>,
    boot_fraction: Option<f32>,
    boot_detail: String,

    // user input / options
    url: String,
    opts: DownloadOptions,
    naming_choice: NamingChoice,
    custom_template: String,

    // probe preview
    probe_rx: Option<Receiver<ProbeMsg>>,
    preview: Option<MediaInfo>,
    probe_error: Option<String>,
    probing: bool,
    probe_started: Option<std::time::Instant>,

    // active job
    event_rx: Option<Receiver<Event>>,
    job: Option<JobHandle>,
    job_state: JobState,
    progress: Option<Progress>,
    global_fraction: f32,
    logs: Vec<LogLine>,
    show_logs: bool,
    failure_message: Option<String>,
    error_count: usize,
    warning_count: usize,
    cancel_requested: bool,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum NamingChoice {
    TitleOnly,
    ArtistTitle,
    PlaylistIndexTitle,
    AlbumTrack,
    Custom,
}

impl NamingChoice {
    fn all() -> [NamingChoice; 5] {
        [
            NamingChoice::TitleOnly,
            NamingChoice::ArtistTitle,
            NamingChoice::PlaylistIndexTitle,
            NamingChoice::AlbumTrack,
            NamingChoice::Custom,
        ]
    }
    fn label(self, tr: &'static Tr) -> &'static str {
        match self {
            NamingChoice::TitleOnly => tr.naming_title,
            NamingChoice::ArtistTitle => tr.naming_artist_title,
            NamingChoice::PlaylistIndexTitle => tr.naming_index_title,
            NamingChoice::AlbumTrack => tr.naming_album_track,
            NamingChoice::Custom => tr.naming_custom,
        }
    }

    /// Recover the UI selection (and the custom template text) from a
    /// persisted naming preset.
    fn from_preset(preset: &NamingPreset) -> (NamingChoice, String) {
        let default_template = "%(title)s.%(ext)s".to_string();
        match preset {
            NamingPreset::TitleOnly => (NamingChoice::TitleOnly, default_template),
            NamingPreset::ArtistTitle => (NamingChoice::ArtistTitle, default_template),
            NamingPreset::PlaylistIndexTitle => {
                (NamingChoice::PlaylistIndexTitle, default_template)
            }
            NamingPreset::AlbumTrack => (NamingChoice::AlbumTrack, default_template),
            NamingPreset::Custom(template) => (NamingChoice::Custom, template.clone()),
        }
    }
}

impl CatchYtApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let settings = Settings::load();
        let (naming_choice, custom_template) = NamingChoice::from_preset(&settings.opts.naming);

        CatchYtApp {
            phase: Phase::Bootstrapping,
            deps: None,
            lang: settings.lang,
            boot_rx: spawn_bootstrap(&cc.egui_ctx, false),
            boot_stage: None,
            boot_fraction: None,
            boot_detail: String::new(),
            url: String::new(),
            opts: settings.opts,
            naming_choice,
            custom_template,
            probe_rx: None,
            preview: None,
            probe_error: None,
            probing: false,
            probe_started: None,
            event_rx: None,
            job: None,
            job_state: JobState::Idle,
            progress: None,
            global_fraction: 0.0,
            logs: Vec::new(),
            show_logs: settings.show_logs,
            failure_message: None,
            error_count: 0,
            warning_count: 0,
            cancel_requested: false,
        }
    }

    /// Reset the bootstrap UI state and (re)run the dependency bootstrap.
    /// `refresh_ytdlp` deletes the cached yt-dlp first so the latest release
    /// is fetched again (ffmpeg/Deno installs are kept).
    fn restart_bootstrap(&mut self, ctx: &egui::Context, refresh_ytdlp: bool) {
        self.boot_rx = spawn_bootstrap(ctx, refresh_ytdlp);
        self.boot_stage = None;
        self.boot_fraction = None;
        self.boot_detail.clear();
        self.phase = Phase::Bootstrapping;
    }

    /// Snapshot the current choices and write them to disk. Failures are
    /// ignored: persistence must never get in the way of a download.
    fn persist_settings(&mut self) {
        self.sync_naming();
        let settings = Settings {
            lang: self.lang,
            opts: self.opts.clone(),
            show_logs: self.show_logs,
        };
        let _ = settings.save();
    }

    fn tr(&self) -> &'static Tr {
        self.lang.tr()
    }

    fn sync_naming(&mut self) {
        self.opts.naming = match self.naming_choice {
            NamingChoice::TitleOnly => NamingPreset::TitleOnly,
            NamingChoice::ArtistTitle => NamingPreset::ArtistTitle,
            NamingChoice::PlaylistIndexTitle => NamingPreset::PlaylistIndexTitle,
            NamingChoice::AlbumTrack => NamingPreset::AlbumTrack,
            NamingChoice::Custom => NamingPreset::Custom(self.custom_template.clone()),
        };
    }

    fn drain_bootstrap(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.boot_rx.try_recv() {
            match msg {
                BootMsg::Progress(p) => match p {
                    BootstrapProgress::Stage(stage) => {
                        self.boot_stage = Some(stage);
                        self.boot_fraction = None;
                        self.boot_detail.clear();
                    }
                    BootstrapProgress::Fraction {
                        label,
                        fraction,
                        speed_bps,
                        eta_secs,
                    } => {
                        self.boot_fraction = fraction;
                        self.boot_detail =
                            format_bootstrap_detail(&label, fraction, speed_bps, eta_secs);
                    }
                    BootstrapProgress::Done => {}
                },
                BootMsg::Finished(res) => match res {
                    Ok(deps) => {
                        self.deps = Some(deps);
                        self.phase = Phase::Ready;
                    }
                    Err(e) => self.phase = Phase::BootstrapFailed(e),
                },
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }

    fn drain_probe(&mut self) {
        // Clone the receiver handle so we don't hold a borrow of `self` while
        // mutating other fields inside the loop.
        let rx = self.probe_rx.clone();
        if let Some(rx) = rx {
            if let Ok(msg) = rx.try_recv() {
                self.probing = false;
                self.probe_started = None;
                match msg {
                    ProbeMsg::Ok(info) => {
                        self.preview = Some(info);
                        self.probe_error = None;
                    }
                    ProbeMsg::Err(e) => {
                        self.preview = None;
                        self.probe_error = Some(e);
                    }
                }
                self.probe_rx = None;
            } else if self
                .probe_started
                .is_some_and(|started| started.elapsed() > PROBE_TIMEOUT)
            {
                // Give the button back instead of leaving the spinner stuck
                // until restart; a late reply lands in a dropped channel.
                self.probing = false;
                self.probe_started = None;
                self.probe_rx = None;
                self.preview = None;
                self.probe_error = Some(self.tr().probe_timeout.to_string());
            }
        }
    }

    fn drain_events(&mut self) {
        let mut finished = None;
        let rx = self.event_rx.clone();
        if let Some(rx) = rx {
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    Event::Progress(p) => {
                        self.global_fraction = advance_global_fraction(self.global_fraction, &p);
                        self.progress = Some(p);
                    }
                    Event::Log(line) => {
                        let level = if line.contains("WARNING:") {
                            self.warning_count += 1;
                            LogLevel::Warn
                        } else {
                            LogLevel::Info
                        };
                        self.push_log(level, line);
                    }
                    Event::Error(line) => {
                        self.error_count += 1;
                        if self.failure_message.is_none() {
                            self.failure_message = Some(line.clone());
                        }
                        self.push_log(LogLevel::Error, line);
                    }
                    Event::SpawnError(e) => {
                        self.error_count += 1;
                        self.failure_message = Some(e.clone());
                        self.push_log(LogLevel::Error, format!("ERROR: {e}"));
                    }
                    Event::Finished { success, code } => finished = Some((success, code)),
                }
            }
        }
        if let Some((success, code)) = finished {
            if success {
                self.global_fraction = 1.0;
            }
            self.job_state = if self.cancel_requested {
                JobState::Cancelled
            } else {
                JobState::Done { success, code }
            };
            self.event_rx = None;
            if let Some(mut j) = self.job.take() {
                j.join();
            }
        }
    }

    fn start_probe(&mut self, ctx: &egui::Context) {
        let Some(deps) = &self.deps else { return };
        let url = self.url.trim().to_string();
        if !engine::looks_like_supported_url(&url) {
            return;
        }
        self.probing = true;
        self.probe_started = Some(std::time::Instant::now());
        self.probe_error = None;
        self.preview = None;
        let ytdlp = deps.ytdlp.clone();
        let (tx, rx) = crossbeam_channel::bounded::<ProbeMsg>(1);
        self.probe_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let msg = match engine::probe(&ytdlp, &url) {
                Ok(info) => ProbeMsg::Ok(info),
                Err(e) => ProbeMsg::Err(e.to_string()),
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    fn start_download(&mut self) {
        let Some(deps) = self.deps.clone() else {
            return;
        };
        // Also syncs the naming preset; starting a download is the natural
        // "the user is happy with these settings" checkpoint.
        self.persist_settings();
        let url = self.url.trim().to_string();
        // The button is disabled for unsupported URLs; this guard backs it up
        // so nothing that is not plain-HTTPS YouTube ever reaches yt-dlp.
        if !engine::looks_like_supported_url(&url) {
            return;
        }
        self.logs.clear();
        self.progress = None;
        self.global_fraction = 0.0;
        self.failure_message = None;
        self.error_count = 0;
        self.warning_count = 0;
        self.cancel_requested = false;
        self.job_state = JobState::Running;

        let (tx, rx) = crossbeam_channel::unbounded::<Event>();
        self.event_rx = Some(rx);
        let opts = self.opts.clone();
        let handle = engine::spawn(&deps.ytdlp, &deps.bin_dir, &opts, &url, tx);
        self.job = Some(handle);
    }

    fn cancel_download(&mut self) {
        if let Some(j) = &self.job {
            j.cancel();
        }
        self.cancel_requested = true;
        let note = self.tr().log_cancel_requested.to_string();
        self.push_log(LogLevel::Info, note);
    }

    fn push_log(&mut self, level: LogLevel, text: String) {
        self.logs.push(LogLine { level, text });
        if self.logs.len() > 1000 {
            self.logs.drain(0..self.logs.len() - 1000);
        }
    }
}

impl eframe::App for CatchYtApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Closing the window must not leave yt-dlp/ffmpeg running invisibly
        // in the background: kill the whole process tree, then persist.
        if let Some(job) = &self.job {
            job.cancel();
        }
        if let Some(mut job) = self.job.take() {
            job.join();
        }
        self.persist_settings();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // `matches!` borrows `self.phase` only for the check, releasing it before
        // the `&mut self` UI methods run (avoids a borrow conflict).
        if matches!(self.phase, Phase::Bootstrapping) {
            self.drain_bootstrap(ctx);
            self.bootstrap_ui(ctx);
        } else if matches!(self.phase, Phase::BootstrapFailed(_)) {
            self.bootstrap_failed_ui(ctx);
        } else {
            self.drain_probe();
            self.drain_events();
            // egui only repaints on interaction; while background work runs,
            // keep scheduling frames so progress stays live without input.
            // (Replaces the former fixed-lifetime "repaint pump" thread,
            // which stopped after 10 minutes and outlived short jobs.)
            if matches!(self.job_state, JobState::Running) || self.probing {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            self.main_ui(ctx);
        }
    }
}

// ── UI rendering ────────────────────────────────────────────────────────────
impl CatchYtApp {
    fn bootstrap_ui(&mut self, ctx: &egui::Context) {
        let tr = self.tr();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(
                    RichText::new("CatchYT")
                        .size(38.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.label(RichText::new(tr.tagline).size(15.0).color(theme::MUTED));
                ui.add_space(40.0);
                ui.label(RichText::new(boot_stage_text(self.boot_stage, tr)).size(14.0));
                ui.add_space(12.0);
                let bar = match self.boot_fraction {
                    Some(f) => egui::ProgressBar::new(f).desired_width(360.0),
                    None => egui::ProgressBar::new(0.0)
                        .desired_width(360.0)
                        .animate(true),
                };
                ui.add(bar);
                ui.add_space(6.0);
                if !self.boot_detail.is_empty() {
                    ui.label(
                        RichText::new(self.boot_detail.as_str())
                            .size(12.0)
                            .color(theme::MUTED),
                    );
                    ui.add_space(6.0);
                }
                ui.label(RichText::new(tr.boot_note).size(12.0).color(theme::MUTED));
            });
        });
    }

    fn bootstrap_failed_ui(&mut self, ctx: &egui::Context) {
        let tr = self.tr();
        let msg = if let Phase::BootstrapFailed(m) = &self.phase {
            m.clone()
        } else {
            String::new()
        };
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(
                    RichText::new(tr.setup_failed)
                        .size(24.0)
                        .strong()
                        .color(theme::ERR),
                );
                ui.add_space(12.0);
                ui.label(RichText::new(msg.as_str()).color(theme::MUTED));
                ui.add_space(20.0);
                if ui.button(tr.retry).clicked() {
                    let ctx = ui.ctx().clone();
                    self.restart_bootstrap(&ctx, false);
                }
            });
        });
    }

    fn main_ui(&mut self, ctx: &egui::Context) {
        let tr = self.tr();
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("CatchYT")
                        .size(22.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.label(RichText::new(tr.topbar_tagline).color(theme::MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for lang in [Lang::En, Lang::Fr] {
                        if ui
                            .selectable_label(self.lang == lang, lang.label())
                            .clicked()
                        {
                            self.lang = lang;
                        }
                    }
                    ui.separator();
                    let log_label = if self.error_count > 0 {
                        tr.logs_badge_errors
                            .replace("{n}", &self.error_count.to_string())
                    } else if self.warning_count > 0 {
                        tr.logs_badge_warnings
                            .replace("{n}", &self.warning_count.to_string())
                    } else {
                        tr.logs.to_string()
                    };
                    if ui.selectable_label(self.show_logs, log_label).clicked() {
                        self.show_logs = !self.show_logs;
                    }
                    ui.separator();
                    // Re-fetch the latest yt-dlp on demand: extractors rot as
                    // YouTube changes, and the bootstrap otherwise only runs
                    // once per machine.
                    let idle = !matches!(self.job_state, JobState::Running) && !self.probing;
                    let update_btn = ui
                        .add_enabled(idle, egui::Button::new(tr.update_ytdlp).small())
                        .on_hover_text(tr.update_ytdlp_tip);
                    if update_btn.clicked() {
                        self.restart_bootstrap(ctx, true);
                    }
                });
            });
            ui.add_space(6.0);
        });

        if self.show_logs {
            egui::TopBottomPanel::bottom("logs")
                .resizable(true)
                .default_height(150.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr.logs).strong().color(theme::MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button(tr.hide).clicked() {
                                self.show_logs = false;
                            }
                        });
                    });
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.logs {
                                let col = match line.level {
                                    LogLevel::Error => theme::ERR,
                                    LogLevel::Warn => theme::WARN,
                                    LogLevel::Info => theme::MUTED,
                                };
                                ui.label(
                                    RichText::new(line.text.as_str())
                                        .monospace()
                                        .size(11.5)
                                        .color(col),
                                );
                            }
                        });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.url_row(ui, ctx);
                ui.add_space(6.0);
                self.preview_row(ui);
                ui.add_space(6.0);
                self.options_ui(ui);
                ui.add_space(10.0);
                self.action_row(ui);
                ui.add_space(10.0);
                self.progress_ui(ui);
                self.state_banner_ui(ui);
                self.job_status_ui(ui);
            });
        });
    }

    fn url_row(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let tr = self.tr();
        ui.group(|ui| {
            ui.label(RichText::new(tr.link_section).strong());
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text(tr.link_hint)
                        .desired_width(f32::INFINITY),
                );
                resp.context_menu(|ui| {
                    if ui.button(tr.paste).clicked() {
                        if let Some(text) = clipboard_text() {
                            self.url = text.trim().to_string();
                        }
                        ui.close_menu();
                    }
                    if ui.button(tr.clear).clicked() {
                        self.url.clear();
                        ui.close_menu();
                    }
                });
                let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if submit {
                    self.start_probe(ctx);
                }
            });
            ui.horizontal(|ui| {
                let valid = engine::looks_like_supported_url(&self.url);
                let can_probe = !self.probing && valid;
                if ui
                    .add_enabled(can_probe, egui::Button::new(tr.fetch_info))
                    .clicked()
                {
                    self.start_probe(ctx);
                }
                if self.probing {
                    ui.spinner();
                    ui.label(RichText::new(tr.reading_link).color(theme::MUTED));
                }
                if !self.url.trim().is_empty() && !valid {
                    ui.label(
                        RichText::new(tr.unsupported_url)
                            .color(theme::WARN)
                            .size(12.0),
                    );
                }
            });
        });
    }

    fn preview_row(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        if let Some(info) = &self.preview {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("▶").color(theme::ACCENT));
                    ui.label(RichText::new(info.title.as_str()).strong());
                });
                ui.horizontal(|ui| {
                    if let Some(u) = &info.uploader {
                        ui.label(RichText::new(u.as_str()).color(theme::MUTED).size(12.0));
                    }
                    if info.is_playlist {
                        let n = info
                            .entry_count
                            .map(|c| tr.playlist_items_count.replace("{n}", &c.to_string()))
                            .unwrap_or_else(|| tr.playlist_word.to_string());
                        ui.label(
                            RichText::new(format!("· {n}"))
                                .color(theme::ACCENT)
                                .size(12.0),
                        );
                    }
                });
            });
        } else if let Some(err) = &self.probe_error {
            ui.group(|ui| {
                ui.label(RichText::new(tr.probe_failed.replace("{e}", err)).color(theme::ERR));
            });
        }
    }

    fn options_ui(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        ui.columns(2, |cols| {
            // ── Left column: format & quality ───────────────────────────────
            cols[0].group(|ui| {
                ui.label(RichText::new(tr.format_section).strong());
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.opts.kind,
                        DownloadKind::AudioOnly,
                        tr.audio_toggle,
                    );
                    ui.selectable_value(&mut self.opts.kind, DownloadKind::Video, tr.video_toggle);
                });
                ui.add_space(6.0);

                match self.opts.kind {
                    DownloadKind::AudioOnly => {
                        egui::ComboBox::from_label(tr.audio_format_label)
                            .selected_text(audio_format_label(self.opts.audio_format, tr))
                            .show_ui(ui, |ui| {
                                for f in AudioFormat::ALL {
                                    ui.selectable_value(
                                        &mut self.opts.audio_format,
                                        f,
                                        audio_format_label(f, tr),
                                    );
                                }
                            });

                        let lossless = self.opts.audio_format.is_lossless();
                        let is_best = self.opts.audio_format == AudioFormat::Best;
                        ui.add_enabled_ui(!lossless && !is_best, |ui| {
                            egui::ComboBox::from_label(tr.quality_label)
                                .selected_text(audio_quality_label(self.opts.audio_quality, tr))
                                .show_ui(ui, |ui| {
                                    for q in AudioQuality::PRESETS {
                                        ui.selectable_value(
                                            &mut self.opts.audio_quality,
                                            q,
                                            audio_quality_label(q, tr),
                                        );
                                    }
                                });
                        });
                        if lossless {
                            ui.label(
                                RichText::new(tr.lossless_note)
                                    .size(11.0)
                                    .color(theme::MUTED),
                            );
                        }
                    }
                    DownloadKind::Video => {
                        egui::ComboBox::from_label(tr.resolution_label)
                            .selected_text(video_quality_label(self.opts.video_quality, tr))
                            .show_ui(ui, |ui| {
                                for q in VideoQuality::ALL {
                                    ui.selectable_value(
                                        &mut self.opts.video_quality,
                                        q,
                                        video_quality_label(q, tr),
                                    );
                                }
                            });
                        egui::ComboBox::from_label(tr.container_label)
                            .selected_text(self.opts.video_container.label())
                            .show_ui(ui, |ui| {
                                for c in VideoContainer::ALL {
                                    ui.selectable_value(
                                        &mut self.opts.video_container,
                                        c,
                                        c.label(),
                                    );
                                }
                            });
                    }
                }
            });

            // ── Right column: metadata, naming, destination ─────────────────
            cols[1].group(|ui| {
                ui.label(RichText::new(tr.meta_section).strong());
                ui.add_space(4.0);
                ui.checkbox(&mut self.opts.embed_metadata, tr.cb_metadata);
                ui.checkbox(&mut self.opts.embed_thumbnail, tr.cb_thumbnail);
                ui.checkbox(&mut self.opts.embed_chapters, tr.cb_chapters);
                ui.checkbox(&mut self.opts.restrict_filenames, tr.cb_restrict);

                ui.add_space(6.0);
                egui::ComboBox::from_label(tr.naming_label)
                    .selected_text(self.naming_choice.label(tr))
                    .show_ui(ui, |ui| {
                        for c in NamingChoice::all() {
                            ui.selectable_value(&mut self.naming_choice, c, c.label(tr));
                        }
                    });
                if self.naming_choice == NamingChoice::Custom {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.custom_template)
                            .hint_text("%(title)s.%(ext)s")
                            .desired_width(f32::INFINITY),
                    );
                    ui.label(
                        RichText::new(tr.custom_template_note)
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                    if crate::engine::options::template_escapes_output_dir(&self.custom_template) {
                        ui.label(
                            RichText::new(tr.custom_template_warning)
                                .size(11.0)
                                .color(theme::WARN),
                        );
                    }
                }
            });
        });

        // ── Playlist + destination (full width) ─────────────────────────────
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.opts.download_playlist, tr.playlist_checkbox);
                ui.add_space(12.0);
                ui.label(tr.items_label);
                let mut items = self.opts.playlist_items.clone().unwrap_or_default();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut items)
                        .hint_text(tr.items_hint)
                        .desired_width(120.0),
                );
                if resp.changed() {
                    self.opts.playlist_items = if items.trim().is_empty() {
                        None
                    } else {
                        Some(items)
                    };
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(tr.save_to);
                let path = self.opts.output_dir.display().to_string();
                ui.label(
                    RichText::new(path)
                        .monospace()
                        .size(12.0)
                        .color(theme::MUTED),
                );
                if ui.button(tr.choose).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.opts.output_dir = dir;
                    }
                }
            });
        });
    }

    fn action_row(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        ui.horizontal(|ui| {
            let running = matches!(self.job_state, JobState::Running);
            let can_start = !running && engine::looks_like_supported_url(&self.url);
            let btn = egui::Button::new(RichText::new(tr.download_btn).size(15.0).strong())
                .fill(theme::ACCENT)
                .min_size(egui::vec2(160.0, 38.0));
            if ui.add_enabled(can_start, btn).clicked() {
                self.start_download();
            }
            if running && ui.button(RichText::new(tr.cancel).size(14.0)).clicked() {
                self.cancel_download();
            }
        });
    }

    /// The always-visible, at-a-glance job state, front and center under the
    /// progress bars.
    fn state_banner_ui(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        let (text, color) = match &self.job_state {
            JobState::Idle => return,
            JobState::Running => (tr.state_running, theme::WARN),
            JobState::Done { success: true, .. } => (tr.state_done, theme::OK),
            JobState::Done { success: false, .. } => (tr.state_failed, theme::ERR),
            JobState::Cancelled => (tr.state_cancelled, theme::MUTED),
        };
        ui.vertical_centered(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new(text).size(24.0).strong().color(color));
            ui.add_space(6.0);
        });
    }

    fn job_status_ui(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        let failure_code = match &self.job_state {
            JobState::Done {
                success: false,
                code,
            } => Some(*code),
            _ => None,
        };

        if let Some(code) = failure_code {
            let message = self
                .failure_message
                .clone()
                .unwrap_or_else(|| tr.fail_no_detail.to_string());
            egui::Frame::group(ui.style())
                .fill(theme::ERR.linear_multiply(0.12))
                .stroke(egui::Stroke::new(1.0_f32, theme::ERR))
                .show(ui, |ui| {
                    ui.label(RichText::new(message).color(theme::TEXT));
                    ui.horizontal(|ui| {
                        if let Some(code) = code {
                            ui.label(
                                RichText::new(tr.exit_code.replace("{n}", &code.to_string()))
                                    .size(12.0)
                                    .color(theme::MUTED),
                            );
                        }
                        if self.error_count > 1 {
                            ui.label(
                                RichText::new(
                                    tr.errors_detected
                                        .replace("{n}", &self.error_count.to_string()),
                                )
                                .size(12.0)
                                .color(theme::MUTED),
                            );
                        }
                        if ui.button(tr.show_logs).clicked() {
                            self.show_logs = true;
                        }
                    });
                });
        } else if matches!(self.job_state, JobState::Running) {
            if let Some(message) = self.failure_message.clone() {
                egui::Frame::group(ui.style())
                    .fill(theme::WARN.linear_multiply(0.10))
                    .stroke(egui::Stroke::new(1.0_f32, theme::WARN))
                    .show(ui, |ui| {
                        ui.label(RichText::new(tr.error_detected).strong().color(theme::WARN));
                        ui.label(RichText::new(message).color(theme::TEXT));
                        ui.label(
                            RichText::new(tr.continuing_after_error)
                                .size(12.0)
                                .color(theme::MUTED),
                        );
                    });
            }
        }
    }

    fn progress_ui(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr();
        if let Some(p) = &self.progress {
            ui.group(|ui| {
                if let (Some(index), Some(count)) = (p.item_index, p.item_count) {
                    if count > 1 {
                        ui.label(
                            RichText::new(
                                tr.global_progress
                                    .replace("{i}", &index.to_string())
                                    .replace("{n}", &count.to_string()),
                            )
                            .strong(),
                        );
                        ui.add(
                            egui::ProgressBar::new(self.global_fraction.clamp(0.0, 1.0))
                                .text(format!("{:.0}%", self.global_fraction * 100.0))
                                .fill(theme::OK),
                        );
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(tr.current_track)
                                .size(12.0)
                                .color(theme::MUTED),
                        );
                    }
                }
                if !p.title.is_empty() {
                    ui.label(RichText::new(p.title.as_str()).strong());
                }
                let frac = (p.percent / 100.0).clamp(0.0, 1.0);
                ui.add(
                    egui::ProgressBar::new(frac)
                        .text(RichText::new(p.percent_str.as_str()).size(12.0))
                        .fill(theme::ACCENT),
                );
                ui.horizontal(|ui| {
                    let info = tr
                        .progress_info
                        .replace("{d}", &p.downloaded)
                        .replace("{t}", &p.total)
                        .replace("{s}", &p.speed)
                        .replace("{e}", &p.eta);
                    ui.label(RichText::new(info).size(12.0).color(theme::MUTED));
                });
            });
        }
    }
}

/// Standard eframe entry helper used by `main`.
pub fn run() -> eframe::Result<()> {
    let icon = load_icon();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([720.0, 720.0])
        .with_min_inner_size([560.0, 560.0])
        .with_title("CatchYT");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "CatchYT",
        native_options,
        Box::new(|cc| Ok(Box::new(CatchYtApp::new(cc)))),
    )
}

fn load_icon() -> Option<egui::IconData> {
    // Icon embedded at compile time (a missing asset fails the build);
    // returns None only if the bytes fail to decode.
    let bytes: &[u8] = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width: w,
        height: h,
    })
}

/// Launch the dependency bootstrap on a background thread and return the
/// channel its progress arrives on. When `refresh_ytdlp` is set, the cached
/// yt-dlp is deleted first so `ensure_dependencies` fetches the latest
/// release again (ffmpeg/Deno installs are kept).
fn spawn_bootstrap(ctx: &egui::Context, refresh_ytdlp: bool) -> Receiver<BootMsg> {
    let (tx, rx) = crossbeam_channel::unbounded::<BootMsg>();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        if refresh_ytdlp {
            if let Ok(deps) = engine::resolve_existing() {
                let _ = std::fs::remove_file(&deps.ytdlp);
            }
        }
        let tx2 = tx.clone();
        let ctx2 = ctx.clone();
        let mut cb = move |p: BootstrapProgress| {
            let _ = tx2.send(BootMsg::Progress(p));
            ctx2.request_repaint();
        };
        let result = engine::ensure_dependencies(&mut cb).map_err(|e| format!("{e:#}"));
        let _ = tx.send(BootMsg::Finished(result));
        ctx.request_repaint();
    });
    rx
}

/// Read the system clipboard as text, if possible.
fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get_text().ok())
}

/// Localized text for the current bootstrap stage.
fn boot_stage_text(stage: Option<BootstrapStage>, tr: &Tr) -> String {
    match stage {
        None => tr.preparing.to_string(),
        Some(BootstrapStage::DownloadingYtdlp) => tr.boot_ytdlp.to_string(),
        Some(BootstrapStage::DownloadingFfmpeg) => tr.boot_ffmpeg_download.to_string(),
        Some(BootstrapStage::ExtractingFfmpeg) => tr.boot_ffmpeg_extract.to_string(),
        Some(BootstrapStage::DownloadingDeno) => tr
            .boot_deno_download
            .replace("{version}", engine::deps::DENO_VERSION),
        Some(BootstrapStage::ExtractingDeno) => tr.boot_deno_extract.to_string(),
    }
}

fn audio_format_label(format: AudioFormat, tr: &'static Tr) -> &'static str {
    match format {
        AudioFormat::Best => tr.audio_best_format,
        other => other.label(),
    }
}

fn audio_quality_label(quality: AudioQuality, tr: &'static Tr) -> String {
    match quality {
        AudioQuality::Best => tr.best_quality.to_string(),
        other => other.label(),
    }
}

fn video_quality_label(quality: VideoQuality, tr: &'static Tr) -> &'static str {
    match quality {
        VideoQuality::Best => tr.best_resolution,
        other => other.label(),
    }
}

/// Build the small "yt-dlp · 84% · 1.2 MB/s · ETA 3s" line shown under the
/// bootstrap progress bar.
fn format_bootstrap_detail(
    label: &str,
    fraction: Option<f32>,
    speed_bps: Option<f64>,
    eta_secs: Option<f64>,
) -> String {
    let mut parts = vec![label.to_string()];
    if let Some(f) = fraction {
        parts.push(format!("{:.0}%", f * 100.0));
    }
    if let Some(s) = speed_bps {
        parts.push(format_speed(s));
    }
    if let Some(e) = eta_secs {
        parts.push(format!("ETA {}", format_duration(e)));
    }
    parts.join(" · ")
}

fn format_speed(bytes_per_sec: f64) -> String {
    // Binary units, labelled as such — consistent with yt-dlp's own output.
    if bytes_per_sec >= 1024.0 * 1024.0 {
        format!("{:.1} MiB/s", bytes_per_sec / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KiB/s", bytes_per_sec / 1024.0)
    }
}

fn format_duration(secs: f64) -> String {
    let secs = secs.round().max(0.0) as u64;
    if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn advance_global_fraction(previous: f32, progress: &Progress) -> f32 {
    progress
        .overall_fraction()
        .map(|next| previous.max(next))
        .unwrap_or(previous)
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod bootstrap_format_tests {
    use super::*;

    #[test]
    fn detail_includes_all_known_parts() {
        let s = format_bootstrap_detail("ffmpeg", Some(0.5), Some(1_500_000.0), Some(75.0));
        assert_eq!(s, "ffmpeg · 50% · 1.4 MiB/s · ETA 1m15s");
    }

    #[test]
    fn detail_degrades_gracefully_when_unknown() {
        let s = format_bootstrap_detail("yt-dlp", None, None, None);
        assert_eq!(s, "yt-dlp");
    }

    #[test]
    fn small_speeds_use_kib() {
        assert_eq!(format_speed(2048.0), "2 KiB/s");
    }

    #[test]
    fn large_speeds_use_mib() {
        assert_eq!(format_speed(2.0 * 1024.0 * 1024.0), "2.0 MiB/s");
    }

    #[test]
    fn global_progress_never_moves_backwards_between_formats() {
        let completed_first_format = Progress {
            percent: 100.0,
            item_index: Some(2),
            item_count: Some(4),
            ..Progress::default()
        };
        let reset_for_second_format = Progress {
            percent: 0.0,
            item_index: Some(2),
            item_count: Some(4),
            ..Progress::default()
        };
        let first = advance_global_fraction(0.0, &completed_first_format);
        let second = advance_global_fraction(first, &reset_for_second_format);
        assert_eq!(first, second);
    }

    #[test]
    fn naming_choice_round_trips_through_persisted_presets() {
        let (choice, template) =
            NamingChoice::from_preset(&NamingPreset::Custom("%(id)s.%(ext)s".to_string()));
        assert_eq!(choice, NamingChoice::Custom);
        assert_eq!(template, "%(id)s.%(ext)s");

        let (choice, template) = NamingChoice::from_preset(&NamingPreset::AlbumTrack);
        assert_eq!(choice, NamingChoice::AlbumTrack);
        assert_eq!(template, "%(title)s.%(ext)s");
    }

    #[test]
    fn deno_boot_stage_text_carries_the_pinned_version() {
        use crate::i18n::Lang;
        for lang in Lang::ALL {
            let text = boot_stage_text(Some(BootstrapStage::DownloadingDeno), lang.tr());
            assert!(text.contains(engine::deps::DENO_VERSION), "text was {text}");
            assert!(!text.contains("{version}"));
        }
    }
}

//! UI language support. Every user-facing string lives here, in one table per
//! language, so the interface is fully French or fully English — never a mix.
//!
//! Strings containing `{...}` placeholders are filled with `str::replace` at
//! the call site (egui redraws every frame, so allocation there is fine).

/// The interface language. French is the default; the top bar has a toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    pub fn tr(self) -> &'static Tr {
        match self {
            Lang::Fr => &FR,
            Lang::En => &EN,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Fr => "FR",
            Lang::En => "EN",
        }
    }

    pub const ALL: [Lang; 2] = [Lang::Fr, Lang::En];
}

/// One full set of interface strings.
pub struct Tr {
    // Branding / bootstrap
    pub tagline: &'static str,
    pub topbar_tagline: &'static str,
    pub preparing: &'static str,
    pub boot_ytdlp: &'static str,
    pub boot_ffmpeg_download: &'static str,
    pub boot_ffmpeg_extract: &'static str,
    /// `{version}` is replaced by the pinned Deno version.
    pub boot_deno_download: &'static str,
    pub boot_deno_extract: &'static str,
    pub boot_note: &'static str,
    pub setup_failed: &'static str,
    pub retry: &'static str,

    // Top bar / logs
    pub logs: &'static str,
    /// `{n}` is replaced by the error count.
    pub logs_badge_errors: &'static str,
    /// `{n}` is replaced by the warning count.
    pub logs_badge_warnings: &'static str,
    pub hide: &'static str,
    pub update_ytdlp: &'static str,
    pub update_ytdlp_tip: &'static str,

    // Link row
    pub link_section: &'static str,
    pub link_hint: &'static str,
    pub fetch_info: &'static str,
    pub reading_link: &'static str,
    pub unsupported_url: &'static str,
    pub paste: &'static str,
    pub clear: &'static str,

    // Preview
    /// `{n}` is replaced by the entry count.
    pub playlist_items_count: &'static str,
    pub playlist_word: &'static str,
    /// `{e}` is replaced by the probe error.
    pub probe_failed: &'static str,
    pub probe_timeout: &'static str,

    // Options
    pub format_section: &'static str,
    pub audio_toggle: &'static str,
    pub video_toggle: &'static str,
    pub audio_format_label: &'static str,
    pub quality_label: &'static str,
    pub best_quality: &'static str,
    pub audio_best_format: &'static str,
    pub lossless_note: &'static str,
    pub resolution_label: &'static str,
    pub container_label: &'static str,
    pub best_resolution: &'static str,

    pub meta_section: &'static str,
    pub cb_metadata: &'static str,
    pub cb_thumbnail: &'static str,
    pub cb_chapters: &'static str,
    pub cb_restrict: &'static str,

    pub naming_label: &'static str,
    pub naming_title: &'static str,
    pub naming_artist_title: &'static str,
    pub naming_index_title: &'static str,
    pub naming_album_track: &'static str,
    pub naming_custom: &'static str,
    pub custom_template_note: &'static str,
    pub custom_template_warning: &'static str,

    pub playlist_checkbox: &'static str,
    pub items_label: &'static str,
    pub items_hint: &'static str,
    pub save_to: &'static str,
    pub choose: &'static str,

    // Actions & job state
    pub download_btn: &'static str,
    pub cancel: &'static str,
    pub state_running: &'static str,
    pub state_done: &'static str,
    pub state_failed: &'static str,
    pub state_cancelled: &'static str,
    pub log_cancel_requested: &'static str,

    // Failure / warning banners
    pub fail_no_detail: &'static str,
    /// `{n}` is replaced by the exit code.
    pub exit_code: &'static str,
    /// `{n}` is replaced by the error count.
    pub errors_detected: &'static str,
    pub show_logs: &'static str,
    pub error_detected: &'static str,
    pub continuing_after_error: &'static str,

    // Progress
    /// `{i}` and `{n}` are replaced by the item position and count.
    pub global_progress: &'static str,
    pub current_track: &'static str,
    /// `{d}` downloaded, `{t}` total, `{s}` speed, `{e}` eta.
    pub progress_info: &'static str,
}

pub static FR: Tr = Tr {
    tagline: "Téléchargeur YouTube & YouTube Music",
    topbar_tagline: "· audio & vidéo depuis YouTube",
    preparing: "Préparation…",
    boot_ytdlp: "Installation des composants — téléchargement de yt-dlp…",
    boot_ffmpeg_download: "Installation des composants — téléchargement de ffmpeg…",
    boot_ffmpeg_extract: "Installation des composants — extraction de ffmpeg…",
    boot_deno_download: "Prise en charge JavaScript — téléchargement de Deno {version}…",
    boot_deno_extract: "Prise en charge JavaScript — extraction de Deno…",
    boot_note: "Premier lancement : yt-dlp, ffmpeg et Deno (~230 Mo à télécharger).",
    setup_failed: "Échec de l'installation",
    retry: "Réessayer",

    logs: "Logs",
    logs_badge_errors: "Logs · {n} erreur(s)",
    logs_badge_warnings: "Logs · {n} avertissement(s)",
    hide: "Masquer",
    update_ytdlp: "↻ yt-dlp",
    update_ytdlp_tip: "Retélécharge la dernière version de yt-dlp (utile si les téléchargements \
                       se mettent à échouer). ffmpeg et Deno sont conservés.",

    link_section: "Lien",
    link_hint: "Collez une URL de vidéo, playlist ou album YouTube / YouTube Music",
    fetch_info: "Analyser le lien",
    reading_link: "Lecture du lien…",
    unsupported_url:
        "⚠ Lien non pris en charge — seuls les liens YouTube et YouTube Music (https) sont acceptés",
    paste: "Coller",
    clear: "Effacer",

    playlist_items_count: "{n} éléments",
    playlist_word: "playlist",
    probe_failed: "Impossible de lire le lien : {e}",
    probe_timeout: "L'analyse du lien a expiré — réessaie.",

    format_section: "Format",
    audio_toggle: "🎵 Audio",
    video_toggle: "🎬 Vidéo",
    audio_format_label: "Format audio",
    quality_label: "Qualité",
    best_quality: "Meilleure",
    audio_best_format: "Meilleur (source, sans réencodage)",
    lossless_note: "FLAC/WAV sont sans perte — le débit ne s'applique pas. Remarque : la \
                    source YouTube est compressée avec perte, ce n'est donc pas du vrai \
                    lossless.",
    resolution_label: "Résolution",
    container_label: "Conteneur",
    best_resolution: "Meilleure",

    meta_section: "Métadonnées & fichiers",
    cb_metadata: "Inclure les métadonnées (titre, artiste, album)",
    cb_thumbnail: "Inclure la pochette / miniature",
    cb_chapters: "Inclure les chapitres",
    cb_restrict: "Limiter les noms de fichiers à l'ASCII sûr",

    naming_label: "Nommage",
    naming_title: "Titre",
    naming_artist_title: "Artiste - Titre",
    naming_index_title: "01 - Titre (si disponible)",
    naming_album_track: "Album / 01 - Piste (albums)",
    naming_custom: "Modèle personnalisé…",
    custom_template_note: "Syntaxe des modèles de sortie yt-dlp",
    custom_template_warning:
        "⚠ Ce modèle peut écrire hors du dossier de destination (chemin absolu ou « .. »).",

    playlist_checkbox: "Télécharger toute la playlist / l'album",
    items_label: "Éléments :",
    items_hint: "tous (ex. 1-5,8)",
    save_to: "Enregistrer dans :",
    choose: "Choisir…",

    download_btn: "⬇  Télécharger",
    cancel: "Annuler",
    state_running: "Téléchargement en cours…",
    state_done: "✓ Téléchargement terminé",
    state_failed: "✖ Téléchargement interrompu",
    state_cancelled: "Téléchargement annulé",
    log_cancel_requested: "— annulation demandée —",

    fail_no_detail: "yt-dlp s'est arrêté sans fournir de détail supplémentaire.",
    exit_code: "Code de sortie : {n}",
    errors_detected: "· {n} erreurs détectées",
    show_logs: "Afficher les logs",
    error_detected: "Une erreur a été détectée",
    continuing_after_error: "yt-dlp tente de poursuivre les autres pistes de la sélection.",

    global_progress: "Progression globale · piste {i} sur {n}",
    current_track: "Piste en cours",
    progress_info: "{d} sur {t}   ·   {s}   ·   restant {e}",
};

pub static EN: Tr = Tr {
    tagline: "YouTube & YouTube Music downloader",
    topbar_tagline: "· audio & video from YouTube",
    preparing: "Preparing…",
    boot_ytdlp: "Setting up required components — downloading yt-dlp…",
    boot_ffmpeg_download: "Setting up required components — downloading ffmpeg…",
    boot_ffmpeg_extract: "Setting up required components — extracting ffmpeg…",
    boot_deno_download: "Setting up JavaScript support — downloading Deno {version}…",
    boot_deno_extract: "Setting up JavaScript support — extracting Deno…",
    boot_note: "First launch downloads yt-dlp, ffmpeg and Deno (~230 MB).",
    setup_failed: "Setup failed",
    retry: "Retry",

    logs: "Logs",
    logs_badge_errors: "Logs · {n} error(s)",
    logs_badge_warnings: "Logs · {n} warning(s)",
    hide: "Hide",
    update_ytdlp: "↻ yt-dlp",
    update_ytdlp_tip: "Re-downloads the latest yt-dlp (useful when downloads start failing). \
                       ffmpeg and Deno are kept.",

    link_section: "Link",
    link_hint: "Paste a YouTube / YouTube Music video, playlist or album URL",
    fetch_info: "Fetch info",
    reading_link: "Reading link…",
    unsupported_url:
        "⚠ Unsupported link — only YouTube and YouTube Music (https) URLs are accepted",
    paste: "Paste",
    clear: "Clear",

    playlist_items_count: "{n} items",
    playlist_word: "playlist",
    probe_failed: "Could not read link: {e}",
    probe_timeout: "Reading the link timed out — try again.",

    format_section: "Format",
    audio_toggle: "🎵 Audio",
    video_toggle: "🎬 Video",
    audio_format_label: "Audio format",
    quality_label: "Quality",
    best_quality: "Best",
    audio_best_format: "Best (source, no re-encode)",
    lossless_note: "FLAC/WAV are lossless — bitrate does not apply. Note: the YouTube \
                    source is lossy, so this is not true lossless audio.",
    resolution_label: "Resolution",
    container_label: "Container",
    best_resolution: "Best",

    meta_section: "Metadata & files",
    cb_metadata: "Embed metadata (title, artist, album)",
    cb_thumbnail: "Embed cover art / thumbnail",
    cb_chapters: "Embed chapters",
    cb_restrict: "Restrict filenames to safe ASCII",

    naming_label: "Naming",
    naming_title: "Title",
    naming_artist_title: "Artist - Title",
    naming_index_title: "01 - Title (when available)",
    naming_album_track: "Album / 01 - Track (albums)",
    naming_custom: "Custom template…",
    custom_template_note: "yt-dlp output template syntax",
    custom_template_warning:
        "⚠ This template can write outside the destination folder (absolute path or “..”).",

    playlist_checkbox: "Download whole playlist / album",
    items_label: "Items:",
    items_hint: "all (e.g. 1-5,8)",
    save_to: "Save to:",
    choose: "Choose…",

    download_btn: "⬇  Download",
    cancel: "Cancel",
    state_running: "Downloading…",
    state_done: "✓ Download complete",
    state_failed: "✖ Download failed",
    state_cancelled: "Download cancelled",
    log_cancel_requested: "— cancel requested —",

    fail_no_detail: "yt-dlp stopped without further detail.",
    exit_code: "Exit code: {n}",
    errors_detected: "· {n} errors detected",
    show_logs: "Show logs",
    error_detected: "An error was detected",
    continuing_after_error: "yt-dlp is continuing with the remaining tracks.",

    global_progress: "Overall progress · track {i} of {n}",
    current_track: "Current track",
    progress_info: "{d} of {t}   ·   {s}   ·   ETA {e}",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_languages_fill_placeholders() {
        for lang in Lang::ALL {
            let tr = lang.tr();
            assert!(tr.global_progress.contains("{i}") && tr.global_progress.contains("{n}"));
            assert!(tr.boot_deno_download.contains("{version}"));
            assert!(tr.exit_code.contains("{n}"));
            assert!(tr.progress_info.contains("{d}"));
        }
    }

    #[test]
    fn default_language_is_french() {
        assert_eq!(Lang::default(), Lang::Fr);
    }
}

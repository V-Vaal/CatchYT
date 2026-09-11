# Architecture de CatchYT

Ce document explique **comment le code est organisé et pourquoi**, pour permettre à quiconque (y compris toi dans six mois) de se repérer rapidement. Il complète le [README](README.md) (orienté utilisateur/build) et [SIGNING.md](SIGNING.md) (signature de l'exe). Ici on parle structure interne, flux de données, threading et décisions techniques.

> **TL;DR** — CatchYT est un front-end de bureau (Rust + egui) au-dessus de `yt-dlp` et `ffmpeg`. Il ne télécharge rien lui-même depuis YouTube : il **construit une ligne de commande yt-dlp**, lance le process, **parse sa sortie** en temps réel et l'affiche. Toute l'intelligence "YouTube" est déléguée à yt-dlp ; tout ce qui casse quand YouTube change, c'est yt-dlp qui le répare (d'où le bootstrap qui télécharge toujours la dernière version).

---

## 1. Vue d'ensemble

```
┌─────────────────────────────────────────────────────────────┐
│                        catchyt.exe                          │
│                                                             │
│  ┌──────────────┐         ┌────────────────────────────┐    │
│  │  src/app.rs  │ appelle │       src/engine/          │    │
│  │  (UI egui)   │ ──────► │  (aucune dépendance UI)    │    │
│  │  src/theme.rs│ ◄────── │  options / deps /          │    │
│  └──────────────┘ events  │  probe / runner            │    │
│                  (channels)└────────────┬───────────────┘   │
└───────────────────────────────────────── │ ────────────────┘
                                           │ spawn (process fils)
                                           ▼
                     %LOCALAPPDATA%\CatchYT\bin\
                     ├── yt-dlp.exe   ◄── téléchargé au 1er lancement
                     ├── ffmpeg.exe   ◄── (deps.rs)
                     ├── ffprobe.exe
                     └── deno.exe     ◄── runtime JavaScript yt-dlp/EJS
```

Deux couches, séparées volontairement :

| Couche | Fichiers | Rôle | Dépendances |
|---|---|---|---|
| **Engine** | `src/engine/*` | Construction d'arguments, bootstrap des binaires, probe de métadonnées, exécution + parsing | `std`, `reqwest`, `zip`, `serde`, `crossbeam-channel` — **zéro egui** |
| **App** | `src/app.rs`, `src/theme.rs` | État de l'interface, rendu egui, orchestration des threads | `eframe`/`egui` + la couche engine |

Cette séparation est matérialisée par le split **lib/bin** dans `Cargo.toml` : la crate expose une lib (`src/lib.rs`) que les tests d'intégration (`tests/engine_integration.rs`) consomment via `use catchyt::engine::...`, sans jamais instancier d'UI. Si tu ajoutes de la logique métier, elle va dans `engine/` ; si tu ajoutes un widget, dans `app.rs`.

---

## 2. Cartographie des fichiers

```
src/
  main.rs                 Point d'entrée. 8 lignes : cache la console Windows
                          en release (windows_subsystem) et appelle app::run().
  lib.rs                  Racine de la lib — expose app, engine, theme.
  app.rs                  TOUTE l'UI : struct CatchYtApp (état), machine à
                          états (phases), drain des channels, rendu egui,
                          helpers de formatage (vitesse/ETA du bootstrap).
  theme.rs                Palette de couleurs + application du style egui.
  engine/
    mod.rs                Ré-exports publics de la couche engine.
    options.rs            ★ Cœur métier. DownloadOptions + build_args() :
                          la traduction unique choix-UI → flags yt-dlp.
                          Pur, sans I/O, très testé.
    deps.rs               Bootstrap au 1er lancement : télécharge yt-dlp.exe,
                          ffmpeg/ffprobe (zip BtbN) et Deno, vérifie/extrait,
                          cache dans %LOCALAPPDATA%\CatchYT\bin\.
                          Émet BootstrapProgress (fraction, vitesse, ETA).
    probe.rs              Aperçu avant téléchargement : yt-dlp --dump-single-json
                          --flat-playlist → MediaInfo (titre, uploader,
                          playlist?, nb d'items). + validation d'URL stricte
                          (liste blanche d'hôtes, HTTPS uniquement).
    runner.rs             Exécution du job : spawn yt-dlp, threads lecteurs
                          stdout/stderr, parsing des lignes CATCHYT_PROG,
                          Event::{Progress, Log, Error, Finished, SpawnError}.
tests/
  engine_integration.rs   Tests via l'API publique + e2e réel opt-in
                          (variable d'env CATCHYT_E2E=1).
build.rs                  Ressources Windows : icône, manifeste DPI-aware,
                          métadonnées de version (réduit les faux positifs AV).
build.ps1                 Build local : tests + cargo build --release [+ -Run].
.github/workflows/
  build.yml               CI : lint/tests Linux + build/tests Windows
                          + artefact exe + release sur tag v*.
assets/                   icon.png (embarquée dans l'UI), icon.ico (winresource).
```

---

## 3. Le flux d'un téléchargement, de bout en bout

1. **Saisie d'URL** — `looks_like_supported_url()` (`probe.rs`) est une **barrière stricte** : HTTPS obligatoire, hôte sur liste blanche exacte (YouTube / YouTube Music / youtu.be), rejet de l'userinfo (`@`) et des ports explicites. Les boutons Analyser/Télécharger restent désactivés tant qu'elle ne passe pas, et `start_probe()`/`start_download()` re-vérifient avant de lancer yt-dlp. Le tout est testé contre le spoofing d'hôte (`rejects_host_spoofing`).
2. **Fetch info (optionnel)** — `start_probe()` lance un thread qui exécute `yt-dlp --flat-playlist --dump-single-json <url>`, parse le JSON en `MediaInfo` et l'envoie via un channel `bounded(1)`. `--flat-playlist` évite de résoudre chaque entrée d'un album de 200 titres.
3. **Download** — `start_download()` :
   - `DownloadOptions::build_args()` (`options.rs`) produit le vecteur d'arguments complet. **C'est le seul endroit du code qui connaisse les flags yt-dlp de téléchargement.** L'URL est ajoutée en dernier par le runner.
   - `runner::spawn()` lance le process (`CREATE_NO_WINDOW` sur Windows pour ne pas faire clignoter une console) et retourne immédiatement un `JobHandle`.
4. **Progression** — yt-dlp est lancé avec `--progress-template` réglé sur un format maison (voir §4). Le thread lecteur parse chaque ligne : `Event::Progress` contient la piste courante et la position globale dans la file album/playlist.
5. **Fin** — à la mort du process, `Event::Finished { success, code }`. Une erreur produit une bannière rouge indépendante du panneau de logs, masqué par défaut.
6. **Annulation** — `JobHandle::cancel()` positionne un `AtomicBool` **et tue immédiatement tout l'arbre de processus** (`taskkill /T /F` sous Windows) : yt-dlp *et* ses enfants ffmpeg/deno s'arrêtent, même pendant un post-traitement silencieux. Le check du flag dans le thread lecteur reste en filet de sécurité. La fermeture de la fenêtre (`on_exit`) fait la même chose — aucun process ne survit à l'app. Les fichiers partiels ne sont pas supprimés.

---

## 4. Le protocole de progression `CATCHYT_PROG`

yt-dlp permet de personnaliser sa ligne de progression. On lui passe (voir `options.rs::progress_template()`) :

```
download:CATCHYT_PROG|%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s|%(progress._downloaded_bytes_str)s|%(progress._total_bytes_str)s|%(info.playlist_autonumber)s|%(info.n_entries)s|%(info.title)s
```

Côté réception, `runner::parse_progress_line()` reconnaît le préfixe `CATCHYT_PROG|` et découpe en 8 champs : `percent | speed | eta | downloaded | total | queue_index | item_count | title`. La progression globale vaut `(index - 1 + progression_piste) / total` et ne recule jamais entre deux formats d'une même piste.

- **L'ordre et le nombre de champs sont couplés** entre `options.rs` (émission) et `runner.rs` (parsing). Les deux fichiers ont des tests ; le test d'intégration `progress_round_trips_through_parser` vérifie la cohérence bout-à-bout.
- Le titre est **volontairement en dernière position** et le parseur utilise `splitn` : les éventuels `|` du titre sont maintenant conservés.
- yt-dlp émet `NA` / `Unknown` sur certains flux (lives) : le parseur retombe sur `0.0`% sans paniquer (`tolerates_unknown_percent`).

---

## 5. Modèle de threading

**Aucun runtime async.** L'engine utilise des I/O bloquantes (`reqwest::blocking`, `std::process`) sur des threads dédiés, et des channels `crossbeam` pour remonter vers l'UI. C'est un choix délibéré : le volume d'événements est faible (quelques lignes/seconde), tokio n'apporterait que de la complexité ici.

```
thread UI (egui)                    threads de fond
────────────────                    ───────────────────────────────────────────
CatchYtApp::update()
  ├─ drain_bootstrap() ◄── channel ── thread bootstrap (ensure_dependencies)
  ├─ drain_probe()     ◄── channel ── thread probe (yt-dlp --dump-single-json)
  └─ drain_events()    ◄── channel ── thread lecteur stdout ─┬─ process yt-dlp
                                      thread lecteur stderr ─┘
```

Règles du modèle :

- **L'UI ne bloque jamais** : elle fait uniquement des `try_recv()` dans `update()`. Tout ce qui peut durer (réseau, process) vit sur un thread.
- **Réveil de l'UI** : egui ne redessine que sur événement. Chaque callback de fond appelle `ctx.request_repaint()` ; en plus, tant qu'un job tourne ou qu'un probe est en cours, `update()` programme le rafraîchissement suivant via `request_repaint_after(100 ms)` — auto-entretenu, sans thread dédié, et sans limite de durée. (L'ancien thread « repaint pump » s'arrêtait au bout de 10 minutes, ce qui figeait la progression des gros téléchargements, et survivait aux jobs courts.)
- **stderr est drainé sur son propre thread** (`runner.rs::read_child`) : si on lisait stdout et stderr séquentiellement, un yt-dlp très bavard sur stderr remplirait le buffer du pipe et *deadlockerait* le process fils. C'est le piège classique de `std::process` — ne pas « simplifier » ça.
- Le log UI est plafonné à **1000 lignes** (drain des plus anciennes) pour borner la mémoire sur les grosses playlists.

---

## 6. Bootstrap des dépendances (`deps.rs`)

Au premier lancement, l'app télécharge dans `%LOCALAPPDATA%\CatchYT\bin\` :

| Outil | Source | Format | Intégrité |
|---|---|---|---|
| `yt-dlp.exe` | `github.com/yt-dlp/yt-dlp/releases/latest` | binaire direct | SHA-256 vérifié contre le `SHA2-256SUMS` publié par la même release |
| `ffmpeg.exe` + `ffprobe.exe` | `github.com/BtbN/FFmpeg-Builds/releases/latest` (build win64-gpl) | zip, on n'extrait que les 2 exe | SHA-256 vérifié contre le sidecar `.zip.sha256` de la release |
| `deno.exe` | release officielle `denoland/deno` épinglée | zip, extraction du seul exe attendu | version **et** SHA-256 épinglés dans le code |

Détails d'implémentation qui comptent :

- **Pourquoi ne pas embarquer les binaires dans l'exe ?** `yt-dlp.exe` est un build PyInstaller — l'embarquer ferait exploser le taux de faux positifs antivirus sur notre propre exe (voir README §Antivirus). Le bootstrap au premier lancement garde `catchyt.exe` petit (~6 Mo) et « propre ».
- **yt-dlp suit `latest`**, car YouTube casse régulièrement les extracteurs. Deno est au contraire épinglé avec le SHA-256 officiel : son minimum évolue moins souvent et le bootstrap reste reproductible. Suivre `latest` n'empêche pas de vérifier : le hash attendu est lu dans le fichier de checksums **de la même release**, et un mismatch échoue fermement (`latest` étant un tag mouvant, une release qui tombe entre les deux requêtes produit un mismatch — relancer suffit).
- **Mise à jour à la demande** : le bouton « ↻ yt-dlp » de la barre du haut supprime le yt-dlp en cache et relance le bootstrap (ffmpeg/Deno, protégés par leurs marqueurs, ne sont pas retéléchargés). Sans ce geste, seul le premier lancement profite de `latest`.
- **Téléchargement atomique** : on écrit dans `<dest>.part` puis `fs::rename` — un crash en plein download ne laisse jamais un exe tronqué que `all_present()` prendrait pour valide.
- **Progression riche** : la boucle de download mesure débit instantané et ETA (`BootstrapProgress::Fraction { label, fraction, speed_bps, eta_secs }`), formatés côté UI par `format_bootstrap_detail()` (`app.rs`, testé). Si le serveur n'annonce pas de `Content-Length`, fraction et ETA sont `None` et l'UI affiche une barre indéterminée.
- **TLS via rustls** (pas OpenSSL) : pas de DLL système à trouver, cross-compilation simple, surface antivirus réduite.

---

## 7. L'UI comme machine à états (`app.rs`)

```
                 ┌──────────────────┐   Ok(deps)    ┌────────┐
 lancement ────► │  Bootstrapping   │ ────────────► │ Ready  │
                 └────────┬─────────┘               └────────┘
                          │ Err(e)                     ▲
                          ▼                            │ Retry
                 ┌──────────────────┐                  │
                 │ BootstrapFailed  │ ─────────────────┘
                 └──────────────────┘
```

- `Phase` gouverne quel écran est rendu (`bootstrap_ui` / `bootstrap_failed_ui` / `main_ui`).
- À l'intérieur de `Ready`, l'état du job est un second petit automate : `JobState::{Idle, Running, Done{success, code}, Cancelled}`. Les logs restent collectés mais leur panneau est masqué par défaut.
- `DownloadOptions` (engine) est la **source de vérité** des choix utilisateur ; l'UI y lit/écrit directement. Seule exception : le preset de nommage passe par un enum UI intermédiaire (`NamingChoice`) parce que la variante `Custom(String)` porte du texte éditable — `sync_naming()` réconcilie les deux juste avant le lancement d'un job.
- `theme.rs` centralise **toutes** les couleurs (constantes `ACCENT`, `BG`, `OK`, `ERR`…). Pas de couleur en dur dans `app.rs`.

---

## 8. Construction des arguments yt-dlp (`options.rs`)

Quelques invariants à connaître avant de toucher `build_args()` :

- `--ignore-config` est toujours passé : un `yt-dlp.conf` global de l'utilisateur ne doit pas altérer le comportement de l'app (déterminisme).
- `--ffmpeg-location <bin_dir>` est toujours passé : on utilise *notre* ffmpeg bootstrappé, jamais un éventuel ffmpeg du PATH.
- **Audio** : `-x` (+ `--audio-format` sauf pour `Best` qui ne ré-encode pas). `--audio-quality` n'est passé **que** pour les formats avec perte — un bitrate sur du FLAC/WAV serait absurde (`flac_ignores_quality` le verrouille).
- **Vidéo** : sélecteur de format à étages `bv*[height<=H]+ba/b[height<=H]/b[height<=H]/bv*+ba/b` — on tente le meilleur flux ≤ H, avec dégradations successives plutôt qu'un échec sec.
- **FLAC ≠ lossless réel** : la source YouTube est déjà compressée avec perte (Opus/AAC). L'UI l'explique à l'utilisateur ; le README aussi. Ne pas « vendre » le FLAC comme une amélioration de qualité.
- Chaque nouveau flag doit venir avec son test unitaire dans le module `tests` du fichier — c'est le contrat qui permet de refactorer l'UI sans peur.

---

## 9. Stratégie de test

| Niveau | Où | Quoi | Quand |
|---|---|---|---|
| Unitaires (engine) | `options.rs`, `runner.rs`, `probe.rs` (modules `#[cfg(test)]`) | Chaque mapping option→flag, parsing de progression, validation d'URL | `cargo test`, toujours |
| Unitaires (UI) | `app.rs::bootstrap_format_tests` | Formatage vitesse/ETA du bootstrap | `cargo test`, toujours |
| Intégration | `tests/engine_integration.rs` | Mêmes garanties via l'API **publique** de la lib | `cargo test`, toujours |
| End-to-end | `e2e_downloads_audio` (ignored) | Vrai téléchargement d'une vidéo courte (« Me at the zoo ») → vérifie qu'un `.mp3` sort | `$env:CATCHYT_E2E="1"; cargo test -- --ignored e2e_downloads_audio` |

Le e2e est `#[ignore]` + gardé par une variable d'env parce qu'il dépend du réseau **et** de YouTube (rate-limiting possible sur les IP de CI). La CI Windows l'exécute en `continue-on-error` : informatif, jamais bloquant.

---

## 10. CI/CD (`.github/workflows/build.yml`)

```
push / PR ──► job "check" (Linux)          job "windows-build" (Windows)
              ├─ fmt + clippy -D warnings  ├─ cargo test --all      (bloquant)
              │  (bloquants)               ├─ cargo build --release (bloquant)
              └─ cargo test --all          ├─ e2e réel              (non bloquant)
                 (bloquant)                └─ upload artefact catchyt.exe

              job "audit" (Linux)
              └─ cargo audit (RUSTSEC)     (informatif, ne bloque jamais)

tag v*.*.* ──► job "release" : télécharge l'artefact TESTÉ de windows-build
               et l'attache à la GitHub Release (jamais de rebuild : le binaire
               publié est exactement celui qui a passé les tests)
```

`fmt`, `clippy -D warnings` et les tests sont bloquants partout ; seul `cargo audit` (veille RUSTSEC sur les dépendances) et le e2e réseau sont informatifs.

---

## 11. Spécificités Windows

- **Pas de console** : `#![windows_subsystem = "windows"]` en release (`main.rs`) + `CREATE_NO_WINDOW` sur chaque process fils (`probe.rs`, `runner.rs`). Oublier ce flag sur un nouveau `Command` fera clignoter une fenêtre noire à chaque appel yt-dlp.
- **`build.rs`** embarque icône, manifeste DPI-aware (PerMonitorV2) et métadonnées de version via `winresource`. Des métadonnées complètes et cohérentes réduisent le score de suspicion des heuristiques antivirus sur un exe non signé.
- **Profil release** (`Cargo.toml`) : `opt-level="z"`, LTO, `codegen-units=1`, `panic="abort"`, `strip` → exe ~6 Mo, sans symboles. Motivé autant par la taille que par l'antivirus (moins de patterns « louches »).
- **Runtime MSVC statique** (`.cargo/config.toml`) : le build Windows x64 ne dépend pas de `VCRUNTIME140.dll`, ce qui permet de copier `catchyt.exe` sur une machine Windows sans installer le redistribuable Visual C++.
- La seule vraie réponse aux alertes SmartScreen reste la **signature de code** → [SIGNING.md](SIGNING.md).

---

## 12. Comment étendre le projet (recettes)

**Ajouter un format audio** : nouvelle variante dans `AudioFormat` (`options.rs`) + l'ajouter à `ALL`, `yt_dlp_token()`, `label()`, et `is_lossless()` si pertinent + un test. L'UI (combo box) itère sur `ALL`, rien d'autre à faire.

**Ajouter un preset de nommage** : variante dans `NamingPreset` (template + label) **et** dans l'enum UI `NamingChoice` + `sync_naming()` (`app.rs`).

**Ajouter une option yt-dlp** : champ dans `DownloadOptions` + valeur par défaut + émission dans `build_args()` + test unitaire + widget dans `options_ui()`.

**Supporter un autre site que YouTube** : yt-dlp le gère déjà probablement ; côté app il suffirait d'élargir `looks_like_supported_url()` (et l'étiquette UI). Le reste du pipeline est agnostique.

**Ajouter une plateforme (macOS/Linux)** : l'engine est déjà quasi portable (`exe_name()`, `mark_executable()` conditionnels). Les points à traiter : URLs de download ffmpeg par OS dans `deps.rs`, et la CI.

---

## 13. Limitations connues / dette assumée

- **Un seul job à la fois** — pas de file d'attente de téléchargements (l'UI désactive le bouton pendant un job).
- **Annulation** — tue immédiatement l'arbre de processus, mais ne supprime pas les fichiers partiels déjà écrits dans le dossier de sortie.
- **`latest` non épinglé** pour yt-dlp/ffmpeg — trade-off assumé (§6), atténué par la vérification des checksums publiés.
- **Mise à jour des outils manuelle** — le bouton « ↻ yt-dlp » retélécharge à la demande ; pas de vérification automatique de fraîcheur au lancement.
- **egui/eframe 0.28** — plusieurs versions majeures de retard ; la migration vers les 0.3x reste à planifier.
- **Template personnalisé** — validé a minima (vide → fallback, avertissement si chemin absolu/`..`), mais la syntaxe yt-dlp elle-même n'est pas vérifiée avant le lancement.

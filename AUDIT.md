# Audit CatchYT — revue sécurité, architecture et code

*Audit réalisé le 2026-07-12, sur l'intégralité du code source (`src/`, `tests/`, `build.rs`, `build.ps1`, CI) et de la documentation (`README.md`, `ARCHITECTURE.md`, `AGENTS.md`).*

## Verdict global

Le projet est **sain et nettement au-dessus de la moyenne** pour un outil personnel : séparation lib/UI propre, moteur pur et très testé, hygiène de sécurité réfléchie (allowlist d'URL avec tests anti-spoofing, arguments passés en vecteur sans shell, protection zip-slip, écritures atomiques, `--ignore-config`/`--no-remote-components`, Deno épinglé avec SHA-256). Aucune faille critique.

Les points à corriger sont : **un bug UI réel** (gel de la progression après 10 minutes), **des angles morts sur l'annulation et le probe**, **une incohérence d'intégrité dans le bootstrap** (yt-dlp/ffmpeg non vérifiés alors que Deno l'est), et **un problème d'environnement sérieux** (projet dans OneDrive avec `.git` détruit).

| ID | Sévérité | Sujet |
|----|----------|-------|
| A1 | 🔴 Élevée | Projet sous OneDrive, `.git` vide → aucun historique, risque de corruption |
| B1 | 🔴 Élevée | La progression UI gèle après 10 min de téléchargement (repaint pump borné) |
| S1 | 🟠 Moyenne | yt-dlp et ffmpeg téléchargés sans vérification d'intégrité (Deno, si) |
| B2 | 🟠 Moyenne | Fuite de threads « repaint pump » (un par téléchargement, jamais arrêtés) |
| B3 | 🟠 Moyenne | Annulation : ffmpeg orphelin non tué, kill inopérant si yt-dlp est silencieux |
| B4 | 🟠 Moyenne | Probe sans timeout → bouton « Analyser » bloqué définitivement si yt-dlp pend |
| A2 | 🟠 Moyenne | ARCHITECTURE.md §13 contredit le code (settings + i18n existent) |
| A3 | 🟠 Moyenne | CI : le binaire publié en release n'est pas celui qui a passé les tests |
| S2 | 🟡 Faible | Binaires en cache jamais mis à jour ni revérifiés |
| S3 | 🟡 Faible | Template de nommage custom non validé (vide, chemin absolu, `..`) |
| S4 | 🟡 Faible | Fermer l'app ne stoppe pas un téléchargement en cours |
| B5–B8, A4–A6 | ⚪ Mineur | Détails listés plus bas |

---

## 🔴 Élevée

### A1. Environnement : OneDrive + dépôt git détruit

**Constat.** Le dossier vit dans `C:\Users\Val\OneDrive\Bureau\youtube-dl`. Le répertoire `.git` existe mais est vide : `git status` répond `fatal: not a git repository`. Tout l'historique local est perdu. De plus, `target/` (~17 000 fichiers, régénérables) est synchronisé par OneDrive, ce qui est lent, inutile, et le mécanisme le plus probable de la corruption de `.git` (OneDrive gère mal les milliers de petits fichiers modifiés en rafale).

**Risque.** Perte de travail non versionné, corruption récurrente, builds ralentis. C'est le point le plus urgent de l'audit — pas du code, mais il conditionne tout le reste.

**Correctif recommandé.**
1. Déplacer le projet hors OneDrive (ex. `C:\dev\catchyt`).
2. `git init` + commit initial + push vers GitHub (le README suppose déjà un dépôt GitHub pour la CI).
3. Si le maintien sous OneDrive est voulu : au minimum exclure `target/` de la synchro (clic droit → « Libérer de l'espace » ne suffit pas ; utiliser les exclusions de dossiers OneDrive) et re-créer le dépôt git. Le binaire `catchyt.exe` (6 Mo) à la racine est déjà correctement ignoré par `.gitignore`.

### B1. La progression gèle après 10 minutes de téléchargement

**Constat.** [app.rs:357](src/app.rs:357) : au lancement d'un job, un thread « repaint pump » rafraîchit l'UI 10×/s pendant `6000 × 100 ms = 10 minutes`, puis s'arrête. Or c'est **la seule source de repaint** pendant un téléchargement : le thread lecteur de `runner.rs` envoie les événements dans le channel mais n'appelle jamais `ctx.request_repaint()` (contrairement aux threads bootstrap et probe qui le font). egui ne redessine que sur événement — au-delà de 10 minutes (gros album, vidéo 4K, connexion lente), la barre de progression **se fige** tant que l'utilisateur ne bouge pas la souris, et l'état « Terminé » n'apparaît qu'à la prochaine interaction.

**Correctif recommandé.** Passer un `egui::Context` cloné à `engine::spawn` (ou envelopper le `Sender`) pour appeler `request_repaint()` à chaque événement, comme le fait déjà le bootstrap ([app.rs:136-139](src/app.rs:136)) — puis supprimer le pump. Alternative minimale sans toucher au moteur : dans `update()`, appeler `ctx.request_repaint_after(Duration::from_millis(100))` tant que `job_state == Running` (même approche que [app.rs:231](src/app.rs:231) pour le bootstrap).

---

## 🟠 Moyenne

### S1. Intégrité du bootstrap : yt-dlp et ffmpeg non vérifiés

**Constat.** [deps.rs:18-24](src/engine/deps.rs:18) : Deno est épinglé (version + SHA-256 vérifié), mais `yt-dlp.exe` (tag `latest`) et l'archive ffmpeg BtbN (tag roulant `latest`) sont téléchargés **sans aucune vérification d'intégrité**, puis exécutés. La seule protection est TLS + la confiance dans les comptes GitHub amont.

**Analyse.** Le choix de suivre `latest` pour yt-dlp est légitime et documenté (YouTube casse régulièrement les extracteurs ; épingler yt-dlp rendrait l'app inutilisable en quelques semaines). Mais suivre `latest` n'empêche pas de vérifier l'intégrité :

- Les releases yt-dlp publient un fichier `SHA2-256SUMS` à côté des binaires. Télécharger ce fichier puis vérifier `yt-dlp.exe` protège contre les téléchargements tronqués/corrompus et les incohérences de CDN — pas contre une compromission complète du compte amont, mais c'est déjà le modèle de menace couvert pour Deno.
- Les builds BtbN publient de même des `.sha256` par artefact sur le tag `latest`.

**Correctif recommandé.** Étendre le mécanisme `verify_sha256` existant : télécharger le fichier de checksums de la même release et vérifier avant `publish_file`. Coût faible, cohérence retrouvée avec le traitement de Deno. (À noter en positif : la structure actuelle — staging `.part`, marqueurs d'installation transactionnels, extraction limitée aux basenames attendus avec `enclosed_name()` — est exemplaire.)

### B2. Threads « repaint pump » jamais arrêtés

**Constat.** Le pump de [app.rs:357-363](src/app.rs:357) tourne 10 minutes quoi qu'il arrive : il ne s'arrête pas quand le job se termine, et chaque nouveau téléchargement en lance un de plus. Trois téléchargements courts d'affilée = trois threads qui forcent chacun 10 repaints/s pendant 10 minutes sur une app au repos (CPU/GPU/batterie gaspillés, surtout sur portable).

**Correctif.** Résolu automatiquement par le correctif B1 (suppression du pump). Sinon : partager un `Arc<AtomicBool>` mis à `true` dans `drain_events` à la réception de `Finished`, et sortir de la boucle du pump dès qu'il est levé.

### B3. Annulation incomplète : processus enfants survivants et kill différé

**Constat.** Deux problèmes dans [runner.rs](src/engine/runner.rs:193) :
1. Le flag `kill` n'est testé **qu'à la réception d'une ligne stdout**. Pendant un post-traitement silencieux (conversion ffmpeg d'une longue piste, embedding), l'annulation ne prend effet qu'à la prochaine ligne — potentiellement des minutes. (Limitation partiellement documentée dans ARCHITECTURE §13.)
2. `child.kill()` ne tue que `yt-dlp.exe`. Sous Windows, les processus enfants (`ffmpeg.exe`, `deno.exe`) **ne sont pas tués avec leur parent** : un ffmpeg orphelin peut continuer à écrire dans le dossier de sortie après « Annuler ».

**Correctif recommandé.** Le fix propre sous Windows est un **Job Object** (`CreateJobObject` + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, via la crate `win32job` ou `windows-rs`) : la fermeture du job tue tout l'arbre. Alternative sans dépendance : `taskkill /PID <id> /T /F`. Pour le kill différé : surveiller le flag depuis un thread dédié (ou un `recv_timeout` sur un channel de contrôle) plutôt que depuis le callback stdout.

### B4. Probe sans timeout

**Constat.** [probe.rs:52-69](src/engine/probe.rs:52) : `Command::output()` bloque sans limite. Si yt-dlp pend (réseau, DNS, YouTube qui tarpit), le thread du probe ne rend jamais la main : `self.probing` reste `true` pour toujours, le spinner tourne indéfiniment et le bouton « Analyser » reste désactivé jusqu'au redémarrage (le téléchargement, lui, reste possible).

**Correctif recommandé.** Ajouter `--socket-timeout 15` aux arguments du probe (yt-dlp gère lui-même le timeout, solution la plus simple), et/ou armer un timeout côté app : si aucun `ProbeMsg` après N secondes, réinitialiser `probing` et afficher une erreur. Idéalement, permettre aussi d'annuler un probe en cours en relançant.

### A2. ARCHITECTURE.md contredit le code

**Constat.** La doc est de très bonne qualité mais le §13 « Limitations connues » est périmé sur trois points :
- « Pas de persistance des préférences » → faux, [settings.rs](src/settings.rs) persiste tout dans `settings.json` (et c'est testé).
- « Localisation — UI en anglais, pas d'i18n » → faux, [i18n.rs](src/i18n.rs) fournit FR/EN complet, FR par défaut.
- §3 décrit `looks_like_supported_url()` comme une validation « *cosmétique* » qui « n'est **pas** une barrière de sécurité », alors que le code la traite explicitement comme une barrière stricte ([probe.rs:93-96](src/engine/probe.rs:93), [app.rs:335-339](src/app.rs:335)) avec des tests anti-spoofing dédiés. C'est la doc qui est en retard sur le durcissement.

**Risque.** Une doc d'architecture fausse est pire qu'absente : elle oriente les mauvaises décisions (la vôtre dans six mois, ou celle d'un agent IA qui la lit — AGENTS.md impose de la lire avant tout changement structurel).

**Correctif.** Mettre à jour §13 (retirer les deux limitations résolues, garder « un seul job », « annulation différée », « latest non épinglé ») et reformuler §3.1 pour refléter le statut réel de la validation d'URL.

### A3. CI : le binaire publié n'est pas le binaire testé

**Constat.** [build.yml:66-85](.github/workflows/build.yml:66) : le job `release` **recompile** au lieu de récupérer l'artefact du job `windows-build` (qui, lui, a passé les tests). Deux builds séparés peuvent différer (toolchain mise à jour entre-temps, dépendance `latest`…) ; l'exe attaché à la release n'a formellement jamais été testé, et il n'a pas non plus tourné `cargo test` dans son propre job.

**Correctif.** Dans `release`, remplacer les étapes d'installation Rust + build par `actions/download-artifact@v4` (artefact `catchyt-windows-x86_64` du job `windows-build`, déjà en `needs:`). Bonus : garantit que le SHA-256 de la release correspond à l'artefact de CI.

---

## 🟡 Faible

### S2. Binaires en cache : jamais mis à jour, jamais revérifiés

Une fois bootstrappés, `yt-dlp`/`ffmpeg`/`deno` ne sont plus jamais rafraîchis : les correctifs (sécurité et extracteurs YouTube) n'arrivent jamais, et la seule « mise à jour » est de supprimer `%LOCALAPPDATA%\CatchYT\bin\` à la main — ce qui n'est documenté nulle part dans l'UI. Le paradoxe : l'architecture choisit `latest` « pour suivre les cassages YouTube », mais seul le **premier** lancement en profite. Suggestion : un bouton « Mettre à jour les outils » (re-télécharge yt-dlp, qui est petit), ou un re-téléchargement automatique si le fichier a plus de N jours, ou simplement lancer `yt-dlp -U` en tâche de fond. (La revérification d'intégrité du cache à chaque lancement serait du théâtre : un malware local capable de remplacer `yt-dlp.exe` peut aussi patcher CatchYT — même frontière de confiance.)

### S3. Template de nommage custom non validé

[options.rs:224](src/engine/options.rs:224) + [app.rs:761](src/app.rs:761) : le template `Custom` est passé tel quel à `-o`. Deux cas non gérés : (1) champ **vide** → `-o ""` transmis à yt-dlp, comportement indéfini ; (2) un template contenant un chemin absolu ou `..` écrit **hors du dossier de sortie** choisi. C'est de l'auto-sabotage uniquement (l'utilisateur tape lui-même le template), donc sévérité faible, mais un fallback sur `%(title)s.%(ext)s` si le champ trimé est vide, et un avertissement UI si le template contient `..` ou commence par `/`/`X:\`, seraient bon marché.

### S4. Fermer la fenêtre ne stoppe pas le téléchargement

`on_exit` ([app.rs:383](src/app.rs:383)) persiste les réglages mais n'annule pas le job : `yt-dlp` + `ffmpeg` continuent en arrière-plan, invisibles (pas de console), après la fermeture de l'app. Comportement défendable (le téléchargement finit) mais non documenté et surprenant — combiné à B3, un utilisateur qui « ferme pour arrêter » n'arrête rien. Recommandation : appeler `cancel()` + tuer l'arbre de processus dans `on_exit`, ou a minima le documenter.

---

## ⚪ Mineur / qualité de code

- **B5 — Duplication du spawn bootstrap.** [app.rs:130-143](src/app.rs:130) et [app.rs:461-480](src/app.rs:461) (retry) dupliquent 15 lignes identiques. Extraire un `fn spawn_bootstrap(ctx: &egui::Context) -> Receiver<BootMsg>`.
- **B6 — Classification d'erreur dupliquée et divergente.** Le runner classe sur `trim_start().starts_with("ERROR:")` ([runner.rs:243](src/engine/runner.rs:243)) ; la coloration des logs UI re-teste `line.contains("ERROR")` ([app.rs:544](src/app.rs:544)) avec une heuristique différente. Faire porter l'information par l'`Event` (conserver la variante dans le log) plutôt que re-parser.
- **B7 — Commentaire mensonger.** [app.rs:983](src/app.rs:983) : « ignored if the asset is absent at build time » — faux, `include_bytes!` échoue à la compilation si le fichier manque.
- **B8 — Unités.** `format_speed` ([app.rs:1057](src/app.rs:1057)) divise par 1024 mais affiche « MB/s » (ce sont des MiB/s). Cosmétique ; yt-dlp affiche « MiB/s » dans les mêmes écrans, d'où une légère incohérence visuelle.
- **A4 — Clippy/fmt advisory en CI.** Choix assumé et documenté, mais le code les passe déjà : les rendre bloquants ne coûte rien et évite la dérive.
- **A5 — Fraîcheur des dépendances.** `eframe`/`egui` 0.28 (mi-2024) a plusieurs versions majeures de retard ; pas de faille connue exploitée ici, mais la migration sera d'autant plus coûteuse qu'elle attend. Ajouter un job `cargo audit` (advisory) en CI pour être prévenu des RUSTSEC sur `reqwest`/`rustls`/`ring`/`zip`/`image`.
- **A6 — Métadonnées.** `Cargo.toml` : `repository = ""` (à renseigner quand le dépôt GitHub existera). `deps.rs:225` : le User-Agent du bootstrap pointe vers `github.com/yt-dlp/yt-dlp` au lieu du projet lui-même.

---

## Points forts à préserver

À contre-balancer des points ci-dessus, car ils sont rares dans un projet de cette taille :

- **Frontière de sécurité URL exemplaire** : allowlist d'hôtes exacts, HTTPS obligatoire, rejet userinfo/port, et surtout **testée contre le spoofing** ([probe.rs:149-159](src/engine/probe.rs:149)).
- **Aucune construction de commande shell** : arguments en `Vec<String>` de bout en bout ; `--ignore-config`, `--no-js-runtimes` + Deno épinglé, `--no-remote-components` verrouillent le comportement de yt-dlp.
- **Extraction d'archives durcie** : basenames en allowlist, `enclosed_name()` (anti zip-slip), rejet des doublons, publication transactionnelle avec marqueurs — le tout testé, y compris les cas d'échec partiel.
- **Écritures atomiques systématiques** (`.part` + rename) pour les binaires, les marqueurs et les settings.
- **Drainage stderr sur thread dédié** pour éviter le deadlock de pipe classique — avec le commentaire qui explique pourquoi ne pas « simplifier ».
- **Chargement des settings tolérant** (corruption, champs inconnus, dossier disparu) — chaque cas testé.
- **Tests unitaires ciblant les invariants** (ordre des `--parse-metadata`, FLAC sans bitrate, progression jamais rétrograde) et non l'implémentation.

## Ordre d'attaque suggéré

1. **A1** (OneDrive/git) — avant toute modification de code, pour ne plus rien perdre.
2. **B1 + B2** (repaint) — un seul petit correctif pour les deux, gain utilisateur immédiat.
3. **B4** (`--socket-timeout` sur le probe) — une ligne.
4. **S1** (checksums yt-dlp/ffmpeg) — réutilise `verify_sha256` existant.
5. **B3 + S4** (job object Windows) — même chantier « cycle de vie des processus ».
6. **A2, A3** (doc + CI) — rapides, à faire au fil de l'eau.

---

## Suivi des correctifs — 2026-07-13

Tous les points ont été traités le 2026-07-13, **sauf A1** (environnement OneDrive/git, volontairement laissé à Valentin).

| ID | Statut | Correctif appliqué |
|----|--------|--------------------|
| A1 | ⏳ À faire (Valentin) | Déplacement hors OneDrive + ré-init git — hors périmètre code |
| B1 | ✅ Corrigé | Pump supprimé ; `update()` programme `request_repaint_after(100 ms)` tant qu'un job ou un probe est actif — plus de limite de 10 min |
| S1 | ✅ Corrigé | `fetch_expected_sha256` + `parse_sha256_for` dans `deps.rs` : yt-dlp vérifié contre `SHA2-256SUMS`, ffmpeg contre son sidecar `.sha256` (échec ferme) ; testés |
| B2 | ✅ Corrigé | Plus aucun thread pump (même correctif que B1) |
| B3 | ✅ Corrigé | `JobHandle::cancel()` tue immédiatement l'arbre de processus (`taskkill /T /F` sous Windows) ; flag conservé en filet de sécurité |
| B4 | ✅ Corrigé | `--socket-timeout 15` sur le probe + watchdog UI (120 s) qui libère le bouton et affiche une erreur i18n |
| A2 | ✅ Corrigé | ARCHITECTURE.md §2/§3/§5/§6/§10/§13 réalignés sur le code (validation d'URL stricte, annulation, repaint, checksums, CI) |
| A3 | ✅ Corrigé | Le job `release` télécharge l'artefact testé de `windows-build` (`actions/download-artifact`) au lieu de recompiler |
| S2 | ✅ Corrigé | Bouton « ↻ yt-dlp » dans la barre du haut : supprime le yt-dlp en cache et relance le bootstrap (ffmpeg/Deno conservés) |
| S3 | ✅ Corrigé | Template custom vide → fallback `%(title)s.%(ext)s` ; avertissement UI (FR/EN) si le template peut sortir du dossier (`template_escapes_output_dir`, testé) |
| S4 | ✅ Corrigé | `on_exit` annule le job et tue l'arbre de processus avant de persister les réglages |
| B5 | ✅ Corrigé | Spawn bootstrap factorisé (`spawn_bootstrap` + `restart_bootstrap`), partagé par le démarrage, le retry et le bouton de mise à jour |
| B6 | ✅ Corrigé | Logs typés `LogLine { level, text }` : classification unique à l'ingestion, l'UI ne re-parse plus ; ligne d'annulation passée en i18n |
| B7 | ✅ Corrigé | Commentaire de `load_icon` corrigé |
| B8 | ✅ Corrigé | `format_speed` affiche `KiB/s` / `MiB/s` (cohérent avec yt-dlp) ; tests mis à jour |
| A4 | ✅ Corrigé | `cargo fmt --check` et `clippy --all-targets --all-features -- -D warnings` bloquants en CI ; AGENTS.md mis à jour |
| A5 | ✅ Partiel | Job `cargo audit` (advisory) ajouté en CI. La montée egui/eframe 0.28 → 0.3x reste à planifier (chantier séparé) |
| A6 | ✅ Partiel | User-Agent du bootstrap → `CatchYT/<version>`. `repository` dans Cargo.toml à renseigner quand le dépôt GitHub existera (dépend de A1) |

Validation : `cargo test --all` (60 tests OK), `cargo clippy --all-targets --all-features -- -D warnings` (aucun warning), `cargo fmt --check` (propre), build release recompilé.

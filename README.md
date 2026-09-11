# CatchYT

[![build](https://github.com/V-Vaal/CatchYT/actions/workflows/build.yml/badge.svg)](https://github.com/V-Vaal/CatchYT/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Téléchargeur de bureau pour **YouTube** et **YouTube Music**, écrit en **Rust** avec une interface **egui** native. CatchYT pilote `yt-dlp` + `ffmpeg` sous le capot et se concentre d'abord sur l'extraction audio (titres, albums, playlists), avec gestion des métadonnées, du nommage et du choix de format/qualité.

*A native desktop downloader for YouTube and YouTube Music, written in Rust with an egui UI, driving `yt-dlp` and `ffmpeg`. Documentation is in French; the application itself ships bilingual FR/EN.*

**Pour comprendre le projet en tant que développeur :** [`ARCHITECTURE.md`](ARCHITECTURE.md) (visite guidée du code, threading, protocole de progression, décisions techniques) et [`AUDIT.md`](AUDIT.md) (revue sécurité et code du projet par lui-même, avec le suivi des correctifs).

---

## Fonctionnalités

- **Audio ou vidéo** depuis une URL YouTube / YouTube Music (vidéo, playlist ou album).
- **Formats audio** : MP3, FLAC, Opus, M4A (AAC), WAV, Ogg Vorbis, AAC, ou « source sans ré-encodage ».
- **Qualité** : meilleure disponible, ou bitrate fixe (320 / 256 / 192 / 128 kbps) pour les formats avec perte.
- **Vidéo** : sélection de résolution (jusqu'à 4K) et conteneur (MP4 / MKV / WebM).
- **Métadonnées** : titre, artiste, album et numéro de piste embarqués ; pochette / miniature ; chapitres.
- **Nommage** : numérotation automatique des pistes quand elle existe, plus des modèles prêts à l'emploi (titre, « Artiste - Titre », ordre de playlist, `Album / 01 - Piste`) ou un modèle yt-dlp personnalisé.
- **Playlists & albums** : tout télécharger, ou une sélection (`1-5,8`).
- **Progression en temps réel** : barre de la piste courante + avancement global de l'album/playlist.
- **Erreurs visibles** : état du téléchargement affiché en grand au centre (en cours / terminé / interrompu), bannière persistante avec la cause principale ; logs détaillés masqués par défaut et activables à la demande.
- **Interface bilingue** : français par défaut, bascule FR/EN dans la barre du haut (toutes les chaînes vivent dans `src/i18n.rs`).
- **Liens vérifiés** : seuls les liens HTTPS dont l'hôte est exactement YouTube / YouTube Music / youtu.be sont acceptés ; collage via Ctrl+V ou clic droit → Coller.
- **Réglages mémorisés** : langue, format, qualité, nommage, dossier de sortie et options sont sauvegardés (`%LOCALAPPDATA%\CatchYT\settings.json`) et restaurés au lancement suivant.
- **Mise à jour de yt-dlp en un clic** : bouton « ↻ yt-dlp » dans la barre du haut — utile quand YouTube change et que les téléchargements se mettent à échouer.
- **Aucune console qui s'ouvre**, DPI-aware, dossier de destination configurable.

### À savoir sur le FLAC

La source YouTube est **compressée avec perte** (Opus ou AAC). Convertir en FLAC produit un fichier lossless *en conteneur*, mais **sans gain réel de qualité** par rapport à la source. C'est proposé pour la cohérence de bibliothèque, pas pour récupérer une qualité qui n'existe pas dans le flux d'origine.

---

## Premier lancement

CatchYT **n'embarque pas** `yt-dlp.exe`, `ffmpeg.exe` ni `deno.exe` dans son binaire. Au premier démarrage, il les télécharge depuis leurs dépôts officiels (GitHub) vers :

```
%LOCALAPPDATA%\CatchYT\bin\
```

Ce choix est délibéré : empaqueter `yt-dlp.exe` (un build PyInstaller) dans l'exe augmente nettement le risque de faux positif antivirus. Le téléchargement au premier lancement garde le binaire principal petit et propre. Les lancements suivants réutilisent le cache. Compter environ **230 Mo à télécharger** et **400–450 Mo sur disque** pour les quatre exécutables (`yt-dlp`, `ffmpeg`, `ffprobe`, `deno`). Deno fournit le runtime JavaScript désormais recommandé par yt-dlp pour YouTube.

Chaque téléchargement est **vérifié par empreinte SHA-256** avant d'être installé : Deno est épinglé (version + hash dans le code), yt-dlp et ffmpeg sont contrôlés contre les checksums publiés avec leur release. Le bouton « ↻ yt-dlp » de la barre du haut permet de retélécharger la dernière version de yt-dlp à tout moment.

---

## Obtenir l'exécutable

Deux voies, au choix.

### 1. Build automatique via GitHub Actions (aucune installation locale)

Le workflow `.github/workflows/build.yml` compile et teste sur de vrais runners Windows, puis publie l'`.exe` en artefact.

1. Onglet **Actions** du dépôt → ouvre le dernier run `build` terminé.
2. Section **Artifacts** → télécharge `catchyt-windows-x86_64` → `catchyt.exe`.

Un fork ou un clone poussé dans ton propre dépôt déclenche le même workflow automatiquement. Pour publier une release taguée avec l'exe attaché : crée un tag `vX.Y.Z` (`git tag v0.1.0 && git push --tags`) ; le job `release` rattache l'artefact **qui a passé les tests**, il ne recompile pas.

### 2. Build local sur Windows

Prérequis : installer Rust une fois depuis <https://rustup.rs> (toolchain par défaut **MSVC**).

```powershell
git clone https://github.com/V-Vaal/CatchYT.git
cd CatchYT
powershell -ExecutionPolicy Bypass -File .\build.ps1
# ou pour compiler puis lancer :
powershell -ExecutionPolicy Bypass -File .\build.ps1 -Run
```

L'exécutable final prêt à copier sur une autre machine Windows x64 :
`catchyt.exe` à la racine du projet. Le fichier intermédiaire de Cargo reste
disponible dans `target\release\catchyt.exe`.

Le runtime Visual C++ est lié statiquement : Rust et le redistribuable Visual
C++ ne sont pas requis sur la machine cible. CatchYT reste toutefois un
exécutable **transportable avec réseau au premier lancement**, et non une
distribution hors ligne : il télécharge `yt-dlp`, `ffmpeg`, `ffprobe` et `deno` dans
`%LOCALAPPDATA%\CatchYT\bin\`.

Ou en cargo direct :

```powershell
cargo test --all
cargo build --release
```

---

## Portabilité

**Aucune installation.** `catchyt.exe` est un binaire unique, lié statiquement au runtime C (pas de redistribuable Visual C++ requis, uniquement des DLL système Windows). Pas d'installateur, pas de registre, pas de service : copier l'exe suffit. Les seules écritures disque sont `%LOCALAPPDATA%\CatchYT\` (outils téléchargés + `settings.json`) et le dossier de sortie choisi — supprimer ce dossier et l'exe désinstalle tout.

**Portage desktop (Linux / macOS).** Le code est prêt : tout le spécifique-Windows est isolé derrière `cfg(windows)` avec sa variante Unix (noms d'exécutables, permissions, masquage de console), les chemins passent par la crate `directories`, et la CI compile et teste l'intégralité du crate sur Ubuntu à chaque push. Le seul travail réel est dans `src/engine/deps.rs` : les trois URLs de bootstrap pointent vers des binaires Windows ; il faut les décliner par OS (yt-dlp publie `yt-dlp_linux` / `yt-dlp_macos`, Deno et ffmpeg ont leurs archives par plateforme) et produire les builds.

**Mobile.** L'architecture (piloter des exécutables externes yt-dlp/ffmpeg/deno) ne se transpose pas telle quelle : Android demanderait un backend embarquant Python (à la youtubedl-android), et iOS interdit le lancement de sous-processus — un portage iOS impliquerait de remplacer le moteur, pas de l'adapter.

---

## Antivirus & SmartScreen

Un exécutable Rust **non signé** et fraîchement compilé peut déclencher un avertissement **SmartScreen** (« éditeur inconnu ») et, plus rarement, une heuristique antivirus — c'est vrai de tout binaire sans réputation, pas d'un problème du code.

Ce que ce projet fait déjà pour minimiser les faux positifs :

- pas de binaires tiers empaquetés (téléchargement des deps au 1er lancement) ;
- manifeste Windows + métadonnées de version propres ;
- build release *stripped*, LTO, sans code obfusqué ni patterns suspects.

**La seule façon d'éliminer réellement les alertes est de signer l'exe** avec un certificat de signature de code. Voir [`SIGNING.md`](SIGNING.md) pour le guide d'obtention et d'utilisation.

---

## Structure du projet

> Pour une visite guidée du code (architecture, threading, protocole de progression, décisions techniques, recettes d'extension), voir [`ARCHITECTURE.md`](ARCHITECTURE.md).

```
src/
  main.rs              point d'entrée (masque la console en release)
  lib.rs               racine de la bibliothèque
  app.rs               interface egui (état, options, progression)
  i18n.rs              chaînes d'interface FR / EN
  settings.rs          persistance des réglages (JSON, chargement tolérant)
  theme.rs             palette / style
  engine/
    mod.rs             ré-exports
    options.rs         DownloadOptions + constructeur d'arguments yt-dlp (cœur testé)
    deps.rs            bootstrap yt-dlp + ffmpeg/ffprobe + Deno au premier lancement
    probe.rs           aperçu métadonnées (titre, playlist, nb d'items)
    runner.rs          exécution yt-dlp + parsing de progression
tests/
  engine_integration.rs   tests d'intégration (+ test e2e opt-in)
assets/                icônes
.github/workflows/     CI (build + test + artefact exe)
build.rs               ressources Windows (icône, manifeste, version)
build.ps1              build local Windows
```

---

## Tests

```powershell
cargo test --all
```

Le bootstrap réseau réel, sans téléchargement YouTube, est opt-in :

```powershell
$env:CATCHYT_BOOTSTRAP_E2E="1"; cargo test -- --ignored e2e_bootstraps_dependencies
```

Les tests unitaires couvrent la construction des arguments (chaque format/qualité/nommage), le parsing de progression et la validation d'URL. Un test **end-to-end** réel (téléchargement d'une courte vidéo) est désactivé par défaut ; pour le lancer :

```powershell
$env:CATCHYT_E2E="1"; cargo test -- --ignored e2e_downloads_audio
```

La CI l'exécute automatiquement sur le job Windows (non bloquant si YouTube limite l'IP du runner).

---

## Légal

Télécharger du contenu depuis YouTube peut enfreindre ses conditions d'utilisation et, selon le contenu, le droit d'auteur. Cet outil est destiné à un usage légitime (contenus que tu possèdes, sous licence libre, ou dont l'usage est autorisé). Tu es responsable de l'usage que tu en fais.

## Licence

MIT. CatchYT se contente de piloter `yt-dlp` (Unlicense), `ffmpeg` (LGPL/GPL selon le build) et Deno (MIT), téléchargés séparément.

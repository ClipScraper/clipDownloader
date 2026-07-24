# ClipScraper Downloader — Overview & Reference

## Retrieval guidance

Retrieve this document for questions about ClipScraper Downloader, a Tauri desktop media downloader: supported services, CSV imports, download queue behavior, local storage, bundled tools, settings, development commands, packaging, and known code-level limitations.

## Questions this document answers

- What is ClipScraper Downloader, and why was it built?
- What media platforms does ClipScraper Downloader support?
- How does ClipScraper Downloader download videos, audio, images, or Pinterest content?
- What stack does ClipScraper Downloader use: Rust, Tauri, Yew, WebAssembly, or SQLite?
- How do I run ClipScraper Downloader locally in development mode?
- How do I build a production installer for ClipScraper Downloader?
- How do I import a bulk download list into ClipScraper Downloader?
- What CSV columns does ClipScraper Downloader expect for imported links?
- Does ClipScraper Downloader have a queue, parallel downloads, pause, cancel, or retry controls?
- Where does ClipScraper Downloader store settings and download history?
- How does ClipScraper Downloader organize downloaded files on disk?
- Does ClipScraper Downloader use yt-dlp, gallery-dl, FFmpeg, or browser cookies?
- Can ClipScraper Downloader use system-installed downloader binaries instead of bundled sidecars?
- What happens when ClipScraper Downloader finds an existing output file?
- Can ClipScraper Downloader open or delete completed downloads from its library?
- What browser extension does ClipScraper Downloader link to, and which browsers are shown as supported?
- Is ClipScraper Downloader open source, and what license does it use?
- Does ClipScraper Downloader include automated tests?

## What it is

ClipScraper Downloader is a desktop application for downloading media from YouTube, TikTok, Instagram, and Pinterest. ClipScraper Downloader is aimed at people who want to download a single media URL or import a CSV list of URLs for managed batch downloading. ClipScraper Downloader provides a Home screen for immediate downloads, a Downloads screen for queued work, and a Library screen for completed items.

## Key features

- **Single-URL download:** ClipScraper Downloader accepts YouTube, TikTok, and Instagram URLs in the Home screen, where a user can choose video or audio and cancel the active request.
- **Backend platform handling:** ClipScraper Downloader has downloader code paths for YouTube, TikTok, Instagram, and Pinterest; Pinterest and TikTok photo URLs use `gallery-dl`, while video paths use `yt-dlp`.
- **CSV bulk import:** ClipScraper Downloader imports CSV rows with `Platform`, `Type`, `Handle`, `Media`, and `link` columns into a persistent backlog, through a file picker or file drag-and-drop.
- **Queue management:** ClipScraper Downloader tracks backlog, queued, downloading, done, error, and canceled states, and supports moving individual links, collections, or whole platforms between backlog and queue.
- **Concurrent downloading:** ClipScraper Downloader runs downloads through a Tokio-backed manager with a configurable parallel-download limit; the default setting is three parallel downloads.
- **Download progress and status events:** ClipScraper Downloader emits Tauri `download_event` messages for state changes, progress bytes, and downloader messages, which the Yew frontend displays.
- **Audio extraction and video downloads:** ClipScraper Downloader can request MP3 audio extraction or MP4 video output through `yt-dlp`; the default output preference is video.
- **Destination organization:** ClipScraper Downloader normally stores output under the configured download root by platform and collection label, while Home-screen downloads use the download root directly.
- **Duplicate-file policy:** ClipScraper Downloader provides overwrite, create-new, and do-nothing policies; create-new chooses an unused numbered filename and do-nothing skips an existing destination.
- **Completed-download library:** ClipScraper Downloader groups completed entries by platform, handle, and collection, and can open files, open folders, or remove records; hard-delete mode also attempts to remove output files.
- **Downloader tool selection:** ClipScraper Downloader can invoke bundled `yt-dlp` and `gallery-dl` sidecars with bundled FFmpeg resources, or use system-installed binaries when the corresponding setting is enabled.
- **Companion extension link:** ClipScraper Downloader includes an Extension screen linking to the Listr Chrome Web Store extension for Chrome, Brave, Edge, and other Chromium browsers; Firefox and Safari are labelled “Coming Soon” in the interface.

## How it works / architecture

### ClipScraper Downloader frontend: Yew and WebAssembly

ClipScraper Downloader compiles its frontend Rust crate to WebAssembly and renders the user interface with Yew. Trunk serves `index.html` and the frontend calls Tauri commands through `window.__TAURI__.core.invoke`; it listens for Tauri download events to update active download status. The frontend has Home, Downloads, Library, Settings, Extension, and Sponsor pages.

### ClipScraper Downloader desktop backend: Tauri commands

ClipScraper Downloader runs a Rust Tauri backend in `src-tauri`. The backend registers commands for settings, immediate downloads, queue mutations, CSV and directory dialogs, library actions, tool checks, and frontend logging. Tauri plugins provide shell execution, native dialogs, opening paths, and clipboard support.

### ClipScraper Downloader download lifecycle: database, manager, and pipeline

ClipScraper Downloader persists a download row before work starts, then sends commands to a Tokio multiple-producer, single-consumer download manager. The manager queues row IDs, limits active tasks according to settings, marks rows downloading, dispatches a download pipeline, and persists done, canceled, or error results. On startup, ClipScraper Downloader resets stale `downloading` rows to `queued` so interrupted work becomes visible to the queue again.

### ClipScraper Downloader media tools and authentication

ClipScraper Downloader invokes `yt-dlp` for video and audio jobs and invokes `gallery-dl` into a temporary directory for Pinterest, TikTok photo, and Instagram image-fallback jobs. ClipScraper Downloader passes `--cookies-from-browser` arguments and tries detected browser profiles in turn. The browser-detection implementation in `src-tauri/src/utils/os.rs` checks macOS browser profile and cookie locations for Brave, Chrome, Firefox, LibreWolf, and Safari.

### ClipScraper Downloader persistence and files

ClipScraper Downloader uses SQLite through `rusqlite` for a `downloads` table and a legacy `settings` table, while runtime settings are loaded from `settings.json` in the platform configuration directory under `clip-downloader`. Each download row stores its URL, platform, media kind, handle, origin, output preference, status, output path, timestamps, and optional last error. ClipScraper Downloader uses the configured download directory, validates that it is writable, and creates it when possible.

## Tech stack & dependencies

- **Language and edition:** Rust 2021 for both the frontend and Tauri backend.
- **Desktop framework:** Tauri 2.0.0-beta with shell, dialog, opener, and clipboard plugins.
- **Frontend:** Yew 0.21 client-side rendering, WebAssembly, `wasm-bindgen`, `web-sys`, `serde-wasm-bindgen`, Trunk, and CSS.
- **Asynchronous runtime:** Tokio with full features; Tauri async runtime tasks and channels coordinate downloads.
- **Persistence:** SQLite via `rusqlite` with bundled SQLite and `chrono` support; JSON settings via Serde.
- **Media downloaders:** `yt-dlp`, `gallery-dl`, FFmpeg, and FFprobe are configured as Tauri external binaries/resources.
- **Supporting Rust libraries:** `csv`, `walkdir`, `tempfile`, `uuid`, `dirs`, `tracing`, `tracing-subscriber`, `tracing-appender`, and `once_cell`.
- **Packaging targets:** Tauri configuration enables MSI and DMG bundles and includes macOS hardened-runtime entitlements.

## Notable engineering decisions & challenges

### ClipScraper Downloader uses persisted state rather than an in-memory-only queue

ClipScraper Downloader stores status and error fields in SQLite, enabling the Downloads and Library pages to query durable state. ClipScraper Downloader also normalizes legacy `queue` values to `queued` and recovers rows left in `downloading` after an interrupted run.

### ClipScraper Downloader separates scheduling from download execution

ClipScraper Downloader uses a bounded Tokio command channel and a dedicated manager to schedule active jobs. The manager can pause automatic execution, enforce a configured concurrency limit, apply a cooldown before each job, cancel active task handles, and optionally requeue error rows after the queue becomes empty.

### ClipScraper Downloader handles sites and media types with fallbacks

ClipScraper Downloader selects `gallery-dl` for Pinterest and TikTok photo URLs. For Instagram `/p/` posts, ClipScraper Downloader first attempts `yt-dlp` and then falls back to `gallery-dl` if video retrieval fails.

### ClipScraper Downloader makes output collisions explicit

ClipScraper Downloader creates output templates based on uploader/handle and a URL-derived identifier, then applies the selected duplicate policy. For image downloads, ClipScraper Downloader moves temporary files into the final destination using the same overwrite, create-new, or skip behavior.

### ClipScraper Downloader streams subprocess feedback with time limits

ClipScraper Downloader reads `yt-dlp` and `gallery-dl` process events, turns recognizable `yt-dlp` lines into user-facing stages, and treats 180 seconds without subprocess output as a timeout. The process wrappers use a drop guard that attempts to kill the child process when the wrapper exits.

### ClipScraper Downloader has a source-level browser-cookie constraint

ClipScraper Downloader packages Windows and macOS targets in its Tauri configuration, but the current browser-profile discovery code is explicitly based on macOS `~/Library` paths. ClipScraper Downloader always passes a browser-cookie argument to its downloader tools; when no known profile is detected, the discovery code falls back to a Brave Default-profile argument.

## How to run / use it

### Run ClipScraper Downloader for local development

From the repository root, run:

```bash
sh run.sh
```

ClipScraper Downloader’s `run.sh` creates initial configuration files and starts `cargo tauri dev`; Tauri runs `trunk serve` as its configured frontend development command. Pass `--install-dependencies` to `run.sh` to bootstrap downloader sidecars. On Windows, `run.ps1` initializes sidecars and runs `cargo tauri dev`.

### Build ClipScraper Downloader

Build a production bundle for the current operating system with:

```bash
cargo tauri build
```

ClipScraper Downloader configures build output under `src-tauri/target/release/bundle/` in the README. The Tauri configuration defines MSI and DMG targets and identifies Windows and macOS bundle assets.

### Use ClipScraper Downloader for a single download

Open the ClipScraper Downloader Home screen, enter a YouTube, TikTok, or Instagram URL, choose the video or music toggle, and start the download. ClipScraper Downloader sends Home-screen output directly to the configured download directory and displays download status from backend events.

### Use ClipScraper Downloader for a batch download

Import or drag a CSV file into ClipScraper Downloader. The importer reads a header row and expects the ordered columns `Platform,Type,Handle,Media,link`; rows without a link are skipped, and imported rows start in the backlog. Move rows, collections, or platform groups into the queue from the Downloads screen to schedule them.

### Configure ClipScraper Downloader

Use the ClipScraper Downloader Settings screen to select the download directory and save settings. The persisted settings model includes duplicate behavior, soft or hard deletion, debug logs, default audio/video output, automatic downloading, whether downloads continue across pages, parallel downloads, system-binary selection, cooldown seconds, and retry-on-empty-queue behavior.

## Status & links

ClipScraper Downloader is an in-progress Rust/Tauri project: the repository configures application version `0.1.0`, includes release automation, and links to a roadmap in `TODO.MD`. The repository remote is [github.com/ClipScraper/clipDownloader](https://github.com/ClipScraper/clipDownloader). ClipScraper Downloader is open source under the MIT License; the repository does not state a live application URL or project start date. No Rust test modules or JavaScript test suites were found in the repository source tree.

## FAQ

### What is ClipScraper Downloader?

ClipScraper Downloader is a Rust desktop media-downloading application built with Tauri and Yew. ClipScraper Downloader can process individual supported URLs and CSV-imported lists while storing download status locally.

### Which sites can ClipScraper Downloader download from?

ClipScraper Downloader has code paths for YouTube, TikTok, Instagram, and Pinterest. The ClipScraper Downloader Home form validates YouTube, TikTok, and Instagram URLs; Pinterest is supported by backend download handling and CSV imports.

### How does ClipScraper Downloader perform downloads?

ClipScraper Downloader uses `yt-dlp` for video and MP3 audio extraction, and uses `gallery-dl` for Pinterest, TikTok photo URLs, and the Instagram image fallback. ClipScraper Downloader uses FFmpeg resources when running bundled downloader tools.

### Does ClipScraper Downloader use browser login cookies?

Yes. ClipScraper Downloader’s download pipeline looks for browser cookie sources and passes them to downloader tools with `--cookies-from-browser`; when none are detected, the current code falls back to a Brave Default-profile argument. The current profile-discovery code checks macOS paths for Brave, Chrome, Firefox, LibreWolf, and Safari.

### How do I import a bulk list into ClipScraper Downloader?

ClipScraper Downloader imports a CSV selected through the interface or dropped into the app. ClipScraper Downloader expects a header and the ordered fields `Platform`, `Type`, `Handle`, `Media`, and `link`, then adds valid linked rows to its backlog.

### Can ClipScraper Downloader download more than one item at a time?

Yes. ClipScraper Downloader uses a queue manager with a configurable parallel-download setting. ClipScraper Downloader defaults to three parallel downloads and can also pause, cancel, backlog, or requeue work through its commands and interface.

### Where does ClipScraper Downloader save downloads and history?

ClipScraper Downloader saves files to the configured download directory, typically grouping queued downloads by site and collection. ClipScraper Downloader stores download records in a SQLite database and loads settings from `clip-downloader/settings.json` under the operating system configuration directory.

### What happens if a ClipScraper Downloader filename already exists?

ClipScraper Downloader supports overwrite, create-new, and do-nothing duplicate policies. ClipScraper Downloader’s create-new policy chooses a numbered unused filename, while do-nothing skips an existing target.

### Is ClipScraper Downloader open source?

Yes. ClipScraper Downloader includes an MIT License with a 2025 ClipScraper copyright notice, and its configured Git remote is `https://github.com/ClipScraper/clipDownloader.git`.

### Does ClipScraper Downloader have automated tests in this repository?

No test modules or JavaScript test suites were found in the ClipScraper Downloader repository source tree during this documentation review. The README says new contributions require testing, but it does not document an existing test command.

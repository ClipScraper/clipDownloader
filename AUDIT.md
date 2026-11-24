# ClipScraper Tauri audit

The app has concrete data-integrity, process-lifecycle, and distribution problems. This pass fixes several isolated backend defects and identifies the larger changes needed before treating downloads and packaged releases as reliable.

Scope: the `clip-downloader` repository, starting at `514e467`, on local branch `fix/tauri-audit`. The review covers Rust download execution, SQLite storage, settings, CSV import, Yew event handling, Tauri configuration, and build scripts. The sibling browser extension and website are outside this audit. Findings marked **Open** remain in the code; the fixes below are not a complete security certification.

## Changes implemented

| Priority | Defect and consequence | Change |
| --- | --- | --- |
| High | `normalize_link` removed every query string. All `youtube.com/watch?v=...` URLs matched, causing import omissions and potentially opening or hard-deleting the wrong completed video. | Parse URLs and retain query identity outside Instagram; test two distinct videos, playlists, library lookup, and repeat imports. See `database.rs::normalize_link`. |
| High | CSV handles entered destination paths without component validation. A handle containing slash-separated `..` components could escape the intended collection directory. | Replace separators, control characters, and Windows-invalid characters in the complete collection label; preserve ordinary labels. See `database.rs::collection_folder_label`. Existing files are not relocated. |
| High | Downloader input could be a command-line option or a local-file URL. | Validate absolute HTTP/HTTPS URLs at manual entry, import, and execution; reject embedded credentials; put `--` before positional URLs in both downloader processes and the metadata probe. This does not restrict HTTP destinations to public addresses or a platform allowlist. |
| High | Windows library opening passed file paths into `powershell -Command`, where path text can be interpreted as code. | Use the existing Tauri opener plugin. Microsoft documents this interpretation in [PowerShell command parameters](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_powershell_exe?view=powershell-5.1#-command). Windows execution still needs platform testing. |
| Medium | Every settings read rewrote the JSON and performed a directory write probe. Concurrent readers/writers could overwrite settings or replace a partially read file with defaults. | Make reads nonmutating; save through a synced temporary file and atomic replacement. Invalid files remain available for recovery. Directory validation occurs on explicit save; download jobs report directory-creation failures. |
| Medium | CSV imports ran synchronous SQLite work inside an async command and committed each insert separately; database errors could become apparent success. | Move parsing/database work to a blocking task and use one transaction. Propagate parse, invalid-URL, and database errors through file-import commands. A failed import rolls back rather than keeping a partial import. |
| Medium | Metadata probes had neither cancellation cleanup nor a timeout. Gallery downloads detached temporary-directory ownership before starting. | Guard the probe child and impose a 30-second deadline; retain `TempDir` ownership through image processing so errors and dropped futures attempt cleanup. This does not prove termination of descendant processes on every OS. |
| Medium | A prior “already downloaded” message could override a later nonzero process exit. | Require a zero exit code for yt-dlp success. |
| Medium | `Pictures` CSV media became video; manual/pinboard/other origins became recommendation in several listings. Folder actions also opened a parent instead of the requested folder. | Recognize picture tokens, preserve supported origins, and open the actual platform/collection directory. |

The regression tests use temporary directories and an in-memory database. They never open the user's downloads database or execute a downloader.

## Open high priority findings

### Queue ownership and cancellation are inconsistent

In [manager.rs](src-tauri/src/download/manager.rs), `StartNow` appends the requested ID, then sets a general force flag. `maybe_start_next` drains from the queue's front up to the parallel limit. With queued A and B, paused downloads, and a manual request C, it can start A and B instead of C, and C can remain paused after they finish.

The deletion commands in [list.rs](src-tauri/src/commands/list.rs) remove database rows without notifying the manager. Active processes therefore continue downloading after deletion. The legacy move commands also change SQL status without directly updating the manager's in-memory queue.

Cancellation calls `abort()` without awaiting termination. A task may already have dispatched an uncancelable blocking database write. `TaskFinished` carries only a row ID, so a late completion from a previous attempt can remove a replacement attempt from `active`. These are race risks from the control flow, not stress-tested reproductions.

Route every queue mutation and deletion through the manager. Use attempt IDs, await cancellation cleanup, and commit final state only for the current attempt. Add deterministic scheduling tests for paused manual starts, cancellation during finalization, delete-while-active, and immediate restart.

### Completion does not reliably mean a usable file exists

[manager.rs](src-tauri/src/download/manager.rs), `maybe_start_next`, ignores final database-write errors and emits `Done` anyway. It writes status, clears errors, and records the file path separately. A failure between those operations can leave an incomplete library record.

[parse.rs](src-tauri/src/commands/parse.rs), `parse_multiple_filenames_from_output`, accepts intermediate destination lines and returns an empty-path fallback. [pipeline.rs](src-tauri/src/download/pipeline.rs) selects the first candidate without checking that it is the final existing media file. Merged video streams can therefore leave the library pointing at a removed intermediate file. Image sets save only their first path.

Use an explicit tagged `after_move` result, verify output existence, and atomically persist the final status and all output files. Propagate commit failure instead of reporting success.

### Image duplicate handling can lose data

In [pipeline.rs](src-tauri/src/download/pipeline.rs), `move_with_policy` removes the existing target before copying the replacement. A failed copy can destroy the old file. `CreateNew` checks existence and then copies, so concurrent downloads can choose the same name. The move loop logs errors but returns success if another file moved; all-skipped image sets are treated as failure and retried with another browser.

Reserve names atomically, stage replacements beside the target, and distinguish created, skipped, and failed files. Do not delete the last good copy before the replacement is ready. Large synchronous copies should run outside async worker threads.

### Authentication depends on incorrect browser discovery

[utils/os.rs](src-tauri/src/utils/os.rs) hardcodes macOS browser locations. For Chromium browsers, `installed_browsers` passes the `Default` directory to a function that expects its parent and searches for `Default` or `Profile N` beneath it. Normal profiles are missed. Windows/Linux paths and Chromium `Network/Cookies` are unsupported here.

When discovery fails, the code still forces `brave:Default`; neither downloader tries anonymous access. Public downloads can fail solely because Brave is absent. Use platform-specific profile roots and an anonymous attempt, then opt into cookie sources when required. Avoid multiplying metadata probes across every browser for failures unrelated to authentication.

### Bundled macOS FFmpeg is not portable

[run.sh](run.sh) and the workflows copy Homebrew binaries without bundling their dependent libraries. `otool -L src-tauri/resources/ffmpeg` in this checkout shows dependencies on `/opt/homebrew/Cellar/ffmpeg/8.0.1/...` and many `/opt/homebrew/opt/...` libraries. A clean Mac without those libraries cannot run that binary.

The script also labels binaries with a requested target triple without ensuring their actual architecture matches. Package architecture-correct standalone builds, or bundle and rewrite/sign all dependent libraries. Verify the installed app on clean Intel and Apple Silicon machines. The Windows resource names also need alignment: scripts create `resources/ffmpeg.exe` and `ffprobe.exe`, while `tauri.conf.json` names paths without `.exe`.

## Open medium priority findings

### The webview has no content security policy

[tauri.conf.json](src-tauri/tauri.conf.json) explicitly sets `csp` to `null`. The capability file exposes shell execution permissions even though the backend launches download processes itself. This increases the impact of a future webview injection; this audit did not demonstrate an injection vulnerability. Tauri documents that CSP protection requires an explicit policy in its [CSP guidance](https://v2.tauri.app/security/csp/).

Introduce a policy compatible with the compiled WASM and required assets, remove unused frontend permissions, and constrain custom file-opening and file-reading commands. Validate this in a packaged webview; a compile alone is insufficient.

### Home events retain stale state and listeners

In [home.rs](src/pages/home.rs), the listener installed by `use_effect_once` captures the initial `UseStateHandle` for `active_download_id`, which is initially `None`. It does not refresh that captured value on later renders. Progress/completion handling can therefore ignore the newly started download. The effect calls `closure.forget()` and discards the unlisten function, so returning to Home accumulates listeners.

Use a current-value ref or shared app state, retain the unlisten handle, and dispose of it on unmount, including the case where registration resolves after unmount. Exercise start, finish, cancel, navigation away, and navigation back.

### Large libraries incur repeated scans and copies

`link_exists_in_collection` scans and normalizes every existing link in a collection for each CSV row. The import transaction reduces commit overhead, but this remains quadratic for a growing collection. Database initialization also runs schema checks and a legacy status update whenever `Database::new` is called.

In [app.rs](src/app.rs), every download event clones the full entries map; `commit_download_map` clones it again. [dom.rs](src/dom.rs) recursively inspects text and assigns missing IDs after renders. Generated IDs depend on child positions and changing text, so they do not meet the stable explicit-ID requirement. Existing duplicate IDs are collected but not repaired.

Persist indexed normalized identities, migrate once, and reuse connections or a dedicated database worker. Update individual entries and batch progress events. Replace runtime ID generation with explicit component/record IDs and verify uniqueness in the rendered UI. The frontend was audited but not edited in this pass.

### Dependency resolution and CI are not reproducible

`Cargo.lock` exists locally but is ignored and untracked. The manifests contain beta-era version requirements, but the local lock actually resolves Tauri 2.8.5 and shell plugin 2.3.1; the requirement text alone does not establish that the app runs a beta. Fresh clones can resolve different dependencies. Sidecar downloads and Python installs also use moving latest versions without digest verification.

The PR workflow uses Ubuntu 20.04 and installs WebKitGTK 4.0, while release workflows use different Linux dependencies. `build-release.yml` triggers on `master`, whereas `origin/HEAD` is `main`. These workflows need consolidation and an end-to-end run against the intended branch and OS matrix.

Commit an application lockfile, pin and verify sidecars, and add regression tests plus a dependency advisory check to CI. This pass did not perform a RustSec advisory scan or claim dependencies are vulnerability-free.

## Verification

Validation on macOS Apple Silicon with Rust 1.90.0 and the existing local lockfile:

- `cargo test -p clip-downloader --lib --locked`: passed, 9 tests, 0 failures. Includes concurrent settings reads during atomic saves.
- `cargo check -p clip-downloader-ui --target wasm32-unknown-unknown --locked`: passed.
- `git diff --check`: passed.
- Existing warnings remain: backend dead code and camelCase parameter names; two frontend unused assignments.
- Audit relative links resolve. The papercut log is untracked and locally ignored.

No real media download, cookie extraction, GUI launch, installer execution, or release publication was performed. Source-level Windows/Linux findings, native opener behavior, and process-tree cleanup require platform integration tests. The WASM check verifies compilation, not rendered DOM IDs or interactive behavior.

The existing deletion of `pyinstaller-hooks/__pycache__/hook-gallery_dl.cpython-313.pyc` was preserved. Changes are uncommitted and local on `fix/tauri-audit`; nothing was pushed, merged, or deployed.

# Settings & Download Issues — Investigation & Plan

---

## Issue 1: Parallel Downloads resets to 3

### What was verified

- `settings.json` (`~/Library/Application Support/clip-downloader/settings.json`): file is
  written correctly by `save_settings` — confirmed manually, value 5 was on disk.
- `settings.rs :: save_settings()` writes all fields including `parallel_downloads`.
- `settings.rs :: load_settings()` reads all fields; `serde_json::from_str` with
  `unwrap_or_default()`. If deserialization fails for any reason, it silently falls back
  to `Settings::default()` (pd: 3) **and writes that default back to disk**, overwriting
  the saved value. No failure observed in practice, but the pattern is fragile.
- `settings_cmd.rs :: save_settings` command: persists to JSON, then sends `RefreshSettings`
  to the download manager (which rereads the file and updates `max_parallel`). Backend path: no bug.

### Root cause: Yew state-timing bug

`src/pages/settings.rs` — the number input used `onchange` (fires on blur):

In Yew 0.21, `UseStateHandle<T>` stores an `Rc<T>` populated at render time.
`handle.set(new_value)` queues a re-render but does **not** update the `Rc` held by
existing callbacks. Every closure created in render N reads the render-N value.

When the user types "2" and clicks Save, the browser fires within one event-loop turn:

1. `mousedown` on Save → blur on input
2. `change` → `on_parallel_downloads_change` fires → `settings.set(pd: 2)`
   — re-render scheduled, not yet executed
3. `click` → `on_save` fires → `(*settings).clone()` reads the render-N handle → **old value**
4. `save_settings(old value)` sent to backend

The re-render happens after the save. The typed value is never actually saved.
This is why it "always resets to 3" — the initial default (3) gets re-saved on every attempt.

Spinbox arrows (▲/▼) work because each click fires `change` and Yew re-renders
before the next click, replacing `on_save` with a closure that sees the new state.

### Fix applied

Changed `onchange` → `oninput` with `web_sys::InputEvent`. Every keystroke updates
state and triggers a re-render, so by the time the user clicks Save, `on_save` belongs
to the latest render.

**Status: FIXED** (committed in the previous session).

---

## Issue 2: Instagram p/ posts — "gallery-dl failed (IG fallback)"

### What the screenshot shows

Every issue item is an Instagram `p/` URL (photo or carousel post) with error:
```
gallery-dl failed (IG fallback) tmp=/var/folders/93/.../tmpXXXX
```

The tmp path and raw verbose gallery-dl output are being stored verbatim as the
`last_error` for the row.

### Pipeline trace (`download/pipeline.rs`)

For any Instagram URL where `is_ig_post_p = true` (contains `/p/`):
1. yt-dlp is tried first with the browser's cookies.
2. If yt-dlp returns `false` or errors → gallery-dl is run as an image fallback.
3. If gallery-dl also fails → the error message is:
   ```rust
   format!("gallery-dl failed (IG fallback) tmp={}\n{}", tmp_dir.display(), output)
   ```
   This includes the full tmp path + entire verbose gallery-dl output.

### Identified causes

**A. yt-dlp always fails on Instagram photo posts.**
   yt-dlp is a video downloader. For Instagram `p/` carousel/image posts it returns
   a non-zero exit code (no video formats available). The fallback to gallery-dl is
   intentional — but gallery-dl is also failing.

**B. gallery-dl cookie argument format mismatch (high probability).**
   The `cookie_arg` built in `utils/os.rs` is formatted as `browser:Profile` (e.g.,
   `brave:Default`, `chrome:Profile 1`). yt-dlp explicitly supports this
   `browser:profile` syntax. Gallery-dl's `--cookies-from-browser` flag may not
   accept the profile suffix on all versions, resulting in an "unknown browser"
   or cookie-read error that is not caught by `friendly_browser_error`.

**C. `friendly_browser_error` only catches two patterns.**
   It checks for `"find-generic-password failed"` and `"cannot decrypt v10 cookies"`.
   Any other gallery-dl failure (wrong browser format, login required, rate limit,
   extractor error) falls through and the raw verbose output is stored as the error.

**D. The error message shown in the UI is unusable.**
   It includes an internal tmp path (`/var/folders/...`) and a wall of gallery-dl
   `--verbose` output. Neither is actionable for the user.

### Plan

1. **Strip the tmp path from the error message.** Users never need to see it.
   Change the format string to just show a clean summary.

2. **Fix gallery-dl's cookie_arg.** When invoking gallery-dl, strip any `:profile`
   suffix from `cookie_arg` unless the gallery-dl version is known to support it,
   OR pass only the browser name (e.g., `brave`, `chrome`) and let gallery-dl use
   its default profile. Alternatively, use a `--cookies` file export approach.

3. **Improve `friendly_browser_error` to catch more gallery-dl patterns** such as
   "Login required", "HTTP Error 401", "No results", etc., to give actionable messages.

4. **Remove `--verbose` from gallery-dl invocation** (or keep verbose only for debug
   mode). The verbose output floods the error string with irrelevant internal details.

5. **Consider passing only the gallery-dl-compatible browser name** (strip profile):
   - `brave:Default` → `brave`
   - `chrome:Profile 1` → `chrome`
   - `firefox:/path/to/profile` → `firefox` (gallery-dl reads the default profile itself)

---

## Issue 3: Auto-detect system libraries and make it the default

### Current state

- `use_system_binaries: bool` in `Settings` (default: `false`, meaning use Tauri sidecar).
- Setting IS persistent (written to `settings.json`).
- **The checkbox is hidden** until the user manually clicks "Check for local libraries".
  It only appears when `libs` state is `Some` AND all three tools are detected:
  ```rust
  if let Some(stats) = (*libs).clone() {
      if stats.yt_dlp && stats.gallery_dl && stats.ffmpeg {
          // show checkbox
      }
  }
  ```
  `libs` starts as `None` and is never auto-populated.
- Result: users who have system libraries installed don't see the option unless they
  know to click "Check". The setting is invisible and stays `false`.

### Screenshot 2 context

The user checked and found all three libraries (yt-dlp ✓, gallery-dl ✓, ffmpeg ✓),
and has "Use local dependencies instead of sidecar" checked. But on a fresh app launch
they'd need to click "Check" again just to see this option.

### Plan

**Goal:** If system libraries are present, use them by default. Make this automatic and visible.

#### A. Auto-detect on settings page mount

In `settings_page()`, replace the single `use_effect_with((), ...)` with two effects:
- The existing one: calls `load_settings` and populates `settings` state.
- A new one: calls `check_sidecar_tools` automatically on mount, populates `libs` state.

This way, the library status is known immediately when the page opens, without the user
needing to click "Check".

#### B. Always show the `use_system_binaries` checkbox

Remove the conditional `if let Some(stats) = (*libs).clone()` gate around the checkbox.
Show it always (grayed out or disabled when detection is in progress, enabled once done).
This makes it clear the option exists regardless of whether the user has clicked "Check".

#### C. Default to `true` when libraries are available (first launch)

The challenge: how to distinguish "user explicitly set to false" from "never configured".

**Solution: add an `Option<bool>` for `use_system_binaries`** in `Settings`:
- `None` → never configured; backend resolves to auto-detect result
- `Some(true)` → explicitly on
- `Some(false)` → explicitly off

At app start (`lib.rs :: run()`), if `use_system_binaries` is `None`, run a sync
check (or spawn a quick task) for yt-dlp/gallery-dl/ffmpeg in PATH. If all present,
resolve to `true`; save to file as `Some(true)`.

Frontend: treat `None` as "auto" — show the effective value but indicate it was
auto-detected. Allow the user to explicitly override it.

**Alternative (simpler, lower-fidelity):** Keep `use_system_binaries: bool` but change
`Default` to `true` only when compiling for platforms where system tools are common
(macOS/Linux). At `load_settings`, if this is the first launch (file didn't exist), run
a binary probe at startup. If tools are found, write `true`; otherwise `false`.

**Recommended approach:** Option<bool> in Settings — it cleanly separates
"never touched" from "explicitly configured".

#### D. Persistence (already works, but needs visibility fix)

The `use_system_binaries` value is already included in `save_settings` and read back
by `load_settings`. No backend change needed for persistence itself.

The only problem is that once the checkbox was never shown, the user never triggers
`on_use_system_binaries_change`, and the default (`false`) persists. Fixing visibility
(plan B above) resolves this.

#### Summary of changes

| Area | File | Change |
|---|---|---|
| Auto-detect on mount | `src/pages/settings.rs` | Add `use_effect_with((), ...)` that calls `check_sidecar_tools` |
| Always-visible checkbox | `src/pages/settings.rs` | Remove `if let Some(stats)` gate; show checkbox always |
| First-launch default | `src-tauri/src/settings.rs` | Change `use_system_binaries` field to `Option<bool>`; resolve at startup |
| Backend type | `src-tauri/src/database.rs` | `use_system_binaries: Option<bool>` with serde default `None` |
| Effective value | `src-tauri/src/download/video.rs`, `image.rs` | `settings.use_system_binaries.unwrap_or(false)` |
| Frontend type | `src/pages/settings.rs` | `use_system_binaries: Option<bool>` or keep `bool` and rely on auto-detect write |

---

## Feature 4: Cooldown setting

Adds a configurable delay (in seconds) before each download actually starts. Useful for
rate-limiting or respecting per-site request pacing. A value of 0 disables the feature.

### New Settings field

```rust
// src-tauri/src/settings.rs — Settings struct
#[serde(default)]
pub cooldown_secs: u32,   // 0 = disabled
```

`default` resolves to `0u32` via `Default::default()`, so old settings.json files without
this field are read correctly without any migration.

### Manager changes (`src-tauri/src/download/manager.rs`)

Add a `cooldown_secs: u32` state variable alongside `max_parallel`:

```rust
let mut cooldown_secs = initial_settings.cooldown_secs;
```

Update it in `RefreshSettings` (which currently only updates `max_parallel`):

```rust
DownloadCommand::RefreshSettings => {
    let s = settings::load_settings();
    max_parallel = s.parallel_downloads.max(1) as usize;
    cooldown_secs = s.cooldown_secs;
}
```

Pass `cooldown_secs` into `maybe_start_next` as a new parameter. Inside the spawned
async block (before calling `run_download_with_progress`), add:

```rust
if cooldown_secs > 0 {
    tokio::time::sleep(std::time::Duration::from_secs(cooldown_secs as u64)).await;
}
```

This keeps the sleep inside the task, so it counts against active slots — the manager
won't overfill the slot limit during the sleep. The status is already set to
`Downloading` before the sleep, which is accurate (the slot is reserved).

### Frontend changes (`src/pages/settings.rs`)

Add `cooldown_secs: u32` to the `Settings` struct with `#[serde(default)]`.

Add a handler using `oninput` (same pattern as `parallel_downloads`):

```rust
let on_cooldown_change = {
    let settings = settings.clone();
    Callback::from(move |e: web_sys::InputEvent| {
        let value = e.target_unchecked_into::<web_sys::HtmlInputElement>().value_as_number() as u32;
        let mut s = (*settings).clone();
        s.cooldown_secs = value;
        settings.set(s);
    })
};
```

UI element (place after the parallel downloads row):

```html
<div class="form-group row">
    <label for="cooldown">{"Cooldown between downloads (seconds)"}</label>
    <input type="number" id="cooldown" min="0" value={settings.cooldown_secs.to_string()} oninput={on_cooldown_change} />
</div>
```

### Summary of changes

| File | Change |
|---|---|
| `src-tauri/src/settings.rs` | Add `cooldown_secs: u32` field with `#[serde(default)]` |
| `src-tauri/src/download/manager.rs` | Add `cooldown_secs` state var; update in `RefreshSettings`; pass to `maybe_start_next`; sleep before `run_download_with_progress` |
| `src/pages/settings.rs` | Add `cooldown_secs: u32` to `Settings`; add `on_cooldown_change` handler; add number input |

---

## Feature 5: Retry failed downloads at end of queue

When the queue drains and no tasks are active, automatically re-enqueue all items that
have `status='error'`. Prevents the user from having to manually click retry on each one.

### New Settings field

```rust
// src-tauri/src/settings.rs — Settings struct
#[serde(default)]
pub retry_on_queue_empty: bool,   // false = disabled
```

### New DB function (`src-tauri/src/database.rs`)

```rust
pub fn list_error_ids_conn(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM downloads WHERE status='error' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
```

Export it from `database.rs` and add it to the `use crate::database::` import list in
`manager.rs`.

### Manager changes (`src-tauri/src/download/manager.rs`)

Add two new state variables:

```rust
let mut retry_on_queue_empty = initial_settings.retry_on_queue_empty;
let mut auto_retried: std::collections::HashSet<i64> = HashSet::new();
```

Update in `RefreshSettings`:

```rust
retry_on_queue_empty = s.retry_on_queue_empty;
```

Clear `auto_retried` on fresh `Enqueue` (the user explicitly added new items, signaling
a new pass — retried failures should be eligible again):

```rust
DownloadCommand::Enqueue { ids } => {
    auto_retried.clear();
    enqueue_ids(...).await;
}
```

In `TaskFinished`, after `active.remove(&id)`, check whether a retry pass is warranted
**before** calling `maybe_start_next` (which will consume the newly-queued items):

```rust
DownloadCommand::TaskFinished { id } => {
    active.remove(&id);
    if retry_on_queue_empty && !paused && queue.is_empty() && active.is_empty() {
        let db_clone = db.clone();
        let error_ids = tauri::async_runtime::spawn_blocking(move || {
            let conn = db_clone.blocking_lock();
            list_error_ids_conn(&*conn).unwrap_or_default()
        })
        .await
        .unwrap_or_default();

        for eid in error_ids {
            if !auto_retried.contains(&eid) {
                auto_retried.insert(eid);
                queue.push_back(eid);
            }
        }
    }
}
```

Then `maybe_start_next` runs as usual after the match block and picks up the newly
enqueued retry items.

The DB status of retried items is still `'error'` when enqueued. `maybe_start_next`
calls `set_status(..., Downloading)` which transitions them correctly; on success/failure
the final status is written as always.

### Loop prevention

`auto_retried` accumulates every ID that has been scheduled for auto-retry. On subsequent
`TaskFinished` events, those IDs are filtered out — so a repeatedly-failing item is only
auto-retried once per "pass". A new pass begins when the user explicitly enqueues new
items (`Enqueue` clears `auto_retried`).

### Frontend changes (`src/pages/settings.rs`)

Add `retry_on_queue_empty: bool` to the `Settings` struct with `#[serde(default)]`.

Handler (standard `onchange` checkbox pattern):

```rust
let on_retry_on_queue_empty_change = {
    let settings = settings.clone();
    Callback::from(move |e: Event| {
        let checked = e.target_unchecked_into::<web_sys::HtmlInputElement>().checked();
        let mut s = (*settings).clone();
        s.retry_on_queue_empty = checked;
        settings.set(s);
    })
};
```

UI element (place after the cooldown row):

```html
<div class="form-group row">
    <label for="retry-on-empty">{"Retry failed downloads when queue empties"}</label>
    <input type="checkbox" id="retry-on-empty" checked={settings.retry_on_queue_empty} onchange={on_retry_on_queue_empty_change} />
</div>
```

### Edge cases

| Case | Behaviour |
|---|---|
| Item fails again on retry | Gets status=error again; `auto_retried` blocks a second auto-retry in the same pass |
| User manually retries while auto-retry is armed | Fine — item was already in `auto_retried` or will be deduplicated by `queue.contains` check in `enqueue_ids` |
| `retry_on_queue_empty` toggled off mid-queue | `retry_on_queue_empty` is updated on next `RefreshSettings`; already-queued retry items proceed normally |
| cooldown + retry | Cooldown applies to retried items the same way — they sleep inside their spawned task |
| App restart with error items | On startup, `queue` is populated from `status='queued'` only; error items are NOT auto-enqueued until the next `TaskFinished` check triggers. This is intentional — the feature acts on queue-drain events, not on startup |

### Summary of changes

| File | Change |
|---|---|
| `src-tauri/src/database.rs` | Add `list_error_ids_conn` function |
| `src-tauri/src/settings.rs` | Add `retry_on_queue_empty: bool` with `#[serde(default)]` |
| `src-tauri/src/download/manager.rs` | Add `retry_on_queue_empty` + `auto_retried` state vars; update in `RefreshSettings`; clear `auto_retried` on `Enqueue`; trigger retry logic in `TaskFinished`; import `list_error_ids_conn` |
| `src/pages/settings.rs` | Add field, handler, and checkbox UI |

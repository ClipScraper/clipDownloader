# Downloads Page Investigation And Fix Plan

## Scope

This document covers the remaining Downloads page consistency bug where the page can briefly or fully empty while downloads are finishing, then repopulate after navigating away and back. It also proposes a new `Issues` section for failed items that should remain visible for later investigation.

No application code is changed in this pass. This is a planning document only.

## Symptoms Observed

- The Downloads page sometimes shows only the section headers and no rows.
- The missing rows often come back after leaving the page and returning.
- The blanking tends to happen around download completion or other refresh moments.
- Failed downloads currently disappear from the active UI instead of being retained somewhere actionable.

## Findings

### 1. The download event listener is mutating state from a stale snapshot

The main frontend risk is in the long-lived `download_event` listener in [src/app.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/app.rs:333).

Inside that listener, the code does:

- clone `downloads` into `map`
- mutate `map`
- write it back with `downloads.set(map)`

The problem is that this listener is created once with `use_effect_with((), ...)`, so it can keep dereferencing an old `UseStateHandle` value. Yew documents this exact pitfall:

- [yew-0.21.0/src/functional/hooks/use_state.rs](/Users/hjoncour/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/yew-0.21.0/src/functional/hooks/use_state.rs:61)

That warning is directly relevant here:

- a completion event can write back an older map
- an older map may be partially populated or empty
- a later full reload restores the correct rows, which matches the observed "leave page and come back" recovery

### 2. Reloads replace the entire snapshot instead of merging

`spawn_reload_downloads(...)` calls `list_downloads` and replaces the whole in-memory map with `build_download_entries(rows)` in [src/app.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/app.rs:119).

That means every refresh:

- drops current stage text
- drops current progress bytes
- drops any in-memory detail that is not already persisted in the DB

Even when the data is valid, this full replacement makes refreshes visually harsher than necessary.

### 3. `reconcile_downloads` is not actually awaited to completion

The current frontend reload path does this:

1. invoke `reconcile_downloads`
2. invoke `list_downloads`

But `reconcile_downloads` only sends a manager command over a channel in [src-tauri/src/commands/downloader.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/commands/downloader.rs:106). That command is processed later by the manager loop in [src-tauri/src/download/manager.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/download/manager.rs:201).

So the frontend currently assumes "reconcile then list", but in reality it is "request reconcile, then immediately list". That leaves a race where the UI can fetch a snapshot before reconciliation has finished.

This is probably not the primary blanking bug, but it is still a consistency bug and should be fixed as part of the same cleanup.

### 4. Unknown terminal events do not force recovery

When the UI receives events for IDs that are not present in the in-memory map, it only schedules a refresh for unknown non-terminal status changes in [src/app.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/app.rs:384).

Current gaps:

- unknown `Progress` events do nothing
- unknown `Message` events do nothing
- unknown terminal `StatusChanged` events do not schedule a recovery refresh

That makes it easier for the page to stay inconsistent until manual navigation triggers a fresh snapshot.

### 5. Failed items are not retained anywhere visible

The frontend currently drops `Done`, `Error`, and `Canceled` rows from the in-memory Downloads page map in [src/app.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/app.rs:69) and also removes terminal rows on event receipt in [src/app.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/app.rs:365).

That means failed items vanish instead of remaining visible for follow-up. There is also no persisted `last_error` field in the `downloads` table today ([src-tauri/src/database.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/database.rs:13)), so even if the status remains `error`, the useful reason is not stored durably.

## Proposed Fix

### A. Move Downloads UI state to a reducer

Replace the current `use_state(HashMap<i64, DownloadEntry>)` mutation pattern with a reducer-driven state model.

Recommended shape:

- `DownloadUiState`
- `entries: HashMap<i64, DownloadEntry>`
- `ready: bool`
- `refreshing: bool`
- optional `last_snapshot_seq: u64`

Recommended actions:

- `ReplaceSnapshot(Vec<ClipRow>)`
- `StatusChanged { id, status }`
- `Progress { id, progress, downloaded_bytes, total_bytes }`
- `Message { id, message }`
- `SetInitialReady`
- `BeginRefresh`
- `EndRefresh`

Why this is the right fix:

- reducer dispatchers are stable across renders
- event listeners can dispatch actions without reading stale state
- the reducer always runs against the current state
- it removes the fragile "clone map from closure, mutate, set" pattern

Target file:

- [src/app.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/app.rs)

### B. Merge snapshots instead of replacing volatile fields

When a fresh DB snapshot arrives, merge it into the reducer state instead of rebuilding a brand-new map blindly.

Rules:

- keep DB-backed row fields authoritative
- preserve transient progress and stage text for rows that still exist and are still `downloading`
- preserve failure detail if an item is currently in `error` and its persisted error metadata matches
- remove rows only when they genuinely no longer belong on the page

This will reduce visual churn and prevent refreshes from wiping useful download-stage context.

### C. Make refreshes non-destructive

The Downloads page should not clear existing rows while a background refresh is happening.

Recommended behavior:

- only show `Loading downloads...` on the first load
- after that, keep existing content on screen while refreshing
- if needed, show a small `Refreshing...` indicator rather than blanking the page

This is primarily a UX rule, but it also acts as a guardrail against future state regression.

### D. Make reconciliation and listing atomic

The current two-step frontend sequence should be replaced with a backend command that returns a consistent post-reconcile snapshot.

Recommended backend command:

- `refresh_download_snapshot`

Behavior:

1. reconcile queue/active state
2. read the final list from the same backend flow
3. return the rows directly to the frontend

This can be implemented either:

- inside the manager with a reply channel, or
- in a direct DB command if reconciliation logic is moved behind a synchronous API

Goal:

- remove the fake "await reconcile" assumption
- guarantee that reloads always see a coherent snapshot

Likely files:

- [src-tauri/src/commands/downloader.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/commands/downloader.rs)
- [src-tauri/src/download/manager.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/download/manager.rs)
- [src-tauri/src/commands/list.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/commands/list.rs)

### E. Always recover on unknown IDs

If the event listener receives any event for an unknown ID, schedule a debounced snapshot refresh.

That rule should apply to:

- `StatusChanged`
- `Progress`
- `Message`

Terminal unknown events are especially important because they currently do not repair the UI.

### F. Add an `Issues` section for failed items

Add a third list section to the Downloads page for items with `status = error`.

Suggested order:

1. Downloading
2. Queue
3. Backlog
4. Issues

This section is for items that failed and should remain available for investigation or retry later.

Minimum behavior:

- show all `error` rows
- do not auto-remove them from the Downloads page
- allow retry
- allow move back to backlog
- allow delete

Recommended row content:

- collection title
- item label
- last failure reason
- actions: `Retry`, `Backlog`, `Delete`

Target files:

- [src/pages/downloads.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/pages/downloads.rs)
- [src/app.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/app.rs)

### G. Persist failure reason in the database

To make the `Issues` section genuinely useful, add a nullable `last_error` column to `downloads`.

Recommended behavior:

- set `last_error` when a download fails
- clear `last_error` when retrying
- clear `last_error` on successful completion
- expose `last_error` in the UI row model

This lets failed items survive app restarts with enough context to be actionable.

Likely files:

- [src-tauri/src/database.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/database.rs)
- [src-tauri/src/download/manager.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/download/manager.rs)
- [src-tauri/src/download/pipeline.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src-tauri/src/download/pipeline.rs)
- [src/types.rs](/Users/hjoncour/Projects/clipscraper/clip-downloader/src/types.rs)

## Implementation Order

### Phase 1: Stabilize state

1. Introduce reducer-based download UI state in `src/app.rs`.
2. Convert the `download_event` listener to dispatch reducer actions only.
3. Stop removing terminal rows in the listener until the reducer has explicit rules for where they belong.
4. Ensure any unknown event type for any unknown ID schedules a debounced refresh.

### Phase 2: Fix reload semantics

1. Replace `reconcile_downloads + list_downloads` with one atomic refresh/snapshot command.
2. Merge snapshots instead of recreating a blank transient state.
3. Keep existing rows visible during background refreshes.

### Phase 3: Add Issues section

1. Extend the UI row type with `last_error`.
2. Persist `last_error` in SQLite with a migration.
3. Render `Issues` in the Downloads page.
4. Add actions for retry, move to backlog, and delete.

## Validation Plan

### Core regression checks

- Open Downloads with backlog, queue, and active rows already present.
- Stay on Downloads while several active items complete.
- Confirm the page never blanks and never requires navigation to repopulate.
- Confirm queue/backlog counts update in place.

### Event ordering checks

- Trigger multiple rapid completions.
- Trigger progress and message updates during a background refresh.
- Confirm stale or out-of-order refreshes do not wipe the visible list.

### Failure handling checks

- Force a known download failure.
- Confirm the row lands in `Issues` instead of disappearing.
- Confirm the failure reason remains visible after restart.
- Confirm `Retry` clears the old error and requeues the row.

## Expected Outcome

After this fix:

- the Downloads page should no longer blank during finishes or refreshes
- page state should remain stable without requiring navigation
- failed items should remain visible under `Issues`
- the UI should provide enough context to investigate failures later

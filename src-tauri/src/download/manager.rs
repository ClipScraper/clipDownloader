use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use tauri::{AppHandle, Emitter};

use crate::database::{
    find_download_by_id_conn, list_all_ui_conn, list_downloading_ids_conn, list_error_ids_conn,
    list_queued_ids_conn, mark_id_done_conn, reset_stale_downloading_to_queued_conn,
    set_last_error_by_id_conn, set_status_by_id_conn, DownloadStatus, UiBacklogRow,
};
use crate::download::pipeline;
use crate::settings;
use rusqlite::Connection;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum DownloadEvent {
    StatusChanged {
        id: i64,
        status: DownloadStatus,
    },
    Progress {
        id: i64,
        progress: f32,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Message {
        id: i64,
        message: String,
    },
}

#[derive(Debug)]
pub enum DownloadCommand {
    Enqueue {
        ids: Vec<i64>,
    },
    MoveToBacklog {
        ids: Vec<i64>,
    },
    Cancel {
        id: i64,
    },
    StartNow {
        id: i64,
        overrides: Option<DownloadOverrides>,
    },
    RefreshSettings,
    SetPaused(bool),
    ReconcileState,
    RefreshSnapshot {
        reply: oneshot::Sender<Result<Vec<UiBacklogRow>, String>>,
    },
    TaskFinished {
        id: i64,
    },
}

#[derive(Debug, Clone)]
pub struct DownloadOverrides {
    pub force_audio: Option<bool>,
    pub flat_destination: bool,
}

#[derive(Clone)]
pub struct DownloadManager {
    cmd_tx: mpsc::Sender<DownloadCommand>,
}

impl DownloadManager {
    pub fn new(cmd_tx: mpsc::Sender<DownloadCommand>) -> Self {
        Self { cmd_tx }
    }

    pub async fn send(&self, cmd: DownloadCommand) -> Result<(), String> {
        self.cmd_tx
            .send(cmd)
            .await
            .map_err(|e| format!("failed to send command: {e}"))
    }
}

impl fmt::Debug for DownloadManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DownloadManager").finish_non_exhaustive()
    }
}

struct ActiveTask {
    handle: tauri::async_runtime::JoinHandle<()>,
}

pub async fn run_download_manager(
    app: AppHandle,
    db: Arc<tokio::sync::Mutex<Connection>>,
    mut cmd_rx: mpsc::Receiver<DownloadCommand>,
    cmd_tx: mpsc::Sender<DownloadCommand>,
) {
    let mut queue: VecDeque<i64> = VecDeque::new();
    let mut active: HashMap<i64, ActiveTask> = HashMap::new();
    let mut overrides: HashMap<i64, DownloadOverrides> = HashMap::new();
    let initial_settings = settings::load_settings();
    let mut paused = !initial_settings.download_automatically;
    let mut max_parallel = initial_settings.parallel_downloads.max(1) as usize;
    let mut cooldown_secs = initial_settings.cooldown_secs;
    let mut retry_on_queue_empty = initial_settings.retry_on_queue_empty;
    let mut auto_retried: HashSet<i64> = HashSet::new();

    // On startup, recover any rows stuck in 'downloading' from a previous run
    {
        let db_clone = db.clone();
        let queued_ids = tauri::async_runtime::spawn_blocking(move || {
            let conn = db_clone.blocking_lock();
            if let Ok(n) = reset_stale_downloading_to_queued_conn(&*conn) {
                if n > 0 {
                    tracing::info!("Recovered {n} rows from 'downloading' → 'queued' on startup");
                }
            }
            list_queued_ids_conn(&*conn)
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();

        for id in queued_ids {
            if !queue.contains(&id) {
                queue.push_back(id);
            }
        }
    }

    maybe_start_next(
        &app,
        db.clone(),
        &mut queue,
        &mut active,
        &mut overrides,
        paused,
        max_parallel,
        cooldown_secs,
        &cmd_tx,
        false,
    )
    .await;

    while let Some(cmd) = cmd_rx.recv().await {
        let mut force_start = false;
        match cmd {
            DownloadCommand::Enqueue { ids } => {
                auto_retried.clear();
                enqueue_ids(
                    &app,
                    db.clone(),
                    &ids,
                    &mut queue,
                    &active,
                    DownloadStatus::Queued,
                )
                .await;
            }
            DownloadCommand::MoveToBacklog { ids } => {
                move_to_backlog(
                    &app,
                    db.clone(),
                    &ids,
                    &mut queue,
                    &mut active,
                    &mut overrides,
                )
                .await;
            }
            DownloadCommand::Cancel { id } => {
                cancel_active(
                    &app,
                    db.clone(),
                    id,
                    &mut queue,
                    &mut active,
                    &mut overrides,
                )
                .await;
            }
            DownloadCommand::StartNow { id, overrides: ov } => {
                if let Some(custom) = ov {
                    overrides.insert(id, custom);
                }
                enqueue_ids(
                    &app,
                    db.clone(),
                    &[id],
                    &mut queue,
                    &active,
                    DownloadStatus::Queued,
                )
                .await;
                force_start = true;
            }
            DownloadCommand::RefreshSettings => {
                let s = settings::load_settings();
                max_parallel = s.parallel_downloads.max(1) as usize;
                cooldown_secs = s.cooldown_secs;
                retry_on_queue_empty = s.retry_on_queue_empty;
                tracing::info!("Updated max_parallel={} cooldown={}s retry_on_empty={}", max_parallel, cooldown_secs, retry_on_queue_empty);
            }
            DownloadCommand::SetPaused(next) => {
                paused = next;
            }
            DownloadCommand::ReconcileState => {
                reconcile_state(&app, db.clone(), &mut queue, &active).await;
            }
            DownloadCommand::RefreshSnapshot { reply } => {
                reconcile_state(&app, db.clone(), &mut queue, &active).await;
                let _ = reply.send(snapshot_downloads(db.clone()).await);
            }
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
        }

        maybe_start_next(
            &app,
            db.clone(),
            &mut queue,
            &mut active,
            &mut overrides,
            paused,
            max_parallel,
            cooldown_secs,
            &cmd_tx,
            force_start,
        )
        .await;
    }
}

async fn reconcile_state(
    app: &AppHandle,
    db: Arc<tokio::sync::Mutex<Connection>>,
    queue: &mut VecDeque<i64>,
    active: &HashMap<i64, ActiveTask>,
) {
    let db_clone = db.clone();
    let (queued_ids, downloading_ids) = tauri::async_runtime::spawn_blocking(move || {
        let conn = db_clone.blocking_lock();
        let queued = list_queued_ids_conn(&*conn).unwrap_or_default();
        let downloading = list_downloading_ids_conn(&*conn).unwrap_or_default();
        (queued, downloading)
    })
    .await
    .unwrap_or_default();

    for id in downloading_ids {
        if active.contains_key(&id) {
            continue;
        }
        if let Ok(changed) = set_status(db.clone(), id, DownloadStatus::Queued).await {
            if changed {
                emit_event(
                    app,
                    DownloadEvent::StatusChanged {
                        id,
                        status: DownloadStatus::Queued,
                    },
                );
            }
        }
        if !queue.contains(&id) {
            queue.push_back(id);
        }
    }

    for id in queued_ids {
        if active.contains_key(&id) || queue.contains(&id) {
            continue;
        }
        queue.push_back(id);
    }
}

async fn snapshot_downloads(
    db: Arc<tokio::sync::Mutex<Connection>>,
) -> Result<Vec<UiBacklogRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.blocking_lock();
        list_all_ui_conn(&*conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Join error: {e}"))?
}

async fn enqueue_ids(
    app: &AppHandle,
    db: Arc<tokio::sync::Mutex<Connection>>,
    ids: &[i64],
    queue: &mut VecDeque<i64>,
    active: &HashMap<i64, ActiveTask>,
    status: DownloadStatus,
) {
    for id in ids {
        if active.contains_key(id) {
            continue;
        }
        if queue.contains(id) {
            continue;
        }
        let changed = match set_status(db.clone(), *id, status).await {
            Ok(c) => c,
            Err(err) => {
                emit_event(
                    app,
                    DownloadEvent::Message {
                        id: *id,
                        message: format!("Failed to set status: {err}"),
                    },
                );
                continue;
            }
        };
        if changed {
            emit_event(app, DownloadEvent::StatusChanged { id: *id, status });
        }
        queue.push_back(*id);
    }
}

async fn move_to_backlog(
    app: &AppHandle,
    db: Arc<tokio::sync::Mutex<Connection>>,
    ids: &[i64],
    queue: &mut VecDeque<i64>,
    active: &mut HashMap<i64, ActiveTask>,
    overrides: &mut HashMap<i64, DownloadOverrides>,
) {
    for id in ids {
        queue.retain(|queued| queued != id);
        if let Some(task) = active.remove(id) {
            task.handle.abort();
        }
        overrides.remove(id);
        let changed = match set_status(db.clone(), *id, DownloadStatus::Backlog).await {
            Ok(c) => c,
            Err(err) => {
                emit_event(
                    app,
                    DownloadEvent::Message {
                        id: *id,
                        message: format!("Failed to move to backlog: {err}"),
                    },
                );
                continue;
            }
        };
        if changed {
            emit_event(
                app,
                DownloadEvent::StatusChanged {
                    id: *id,
                    status: DownloadStatus::Backlog,
                },
            );
        }
    }
}

async fn cancel_active(
    app: &AppHandle,
    db: Arc<tokio::sync::Mutex<Connection>>,
    id: i64,
    queue: &mut VecDeque<i64>,
    active: &mut HashMap<i64, ActiveTask>,
    overrides: &mut HashMap<i64, DownloadOverrides>,
) {
    queue.retain(|queued| *queued != id);
    overrides.remove(&id);
    if let Some(task) = active.remove(&id) {
        task.handle.abort();
    }
    let changed = match set_status(db.clone(), id, DownloadStatus::Canceled).await {
        Ok(c) => c,
        Err(err) => {
            emit_event(
                app,
                DownloadEvent::Message {
                    id,
                    message: format!("Failed to cancel: {err}"),
                },
            );
            return;
        }
    };
    // If status actually changed, emit it
    if changed {
        emit_event(
            app,
            DownloadEvent::StatusChanged {
                id,
                status: DownloadStatus::Canceled,
            },
        );
    }
}

async fn maybe_start_next(
    app: &AppHandle,
    db: Arc<tokio::sync::Mutex<Connection>>,
    queue: &mut VecDeque<i64>,
    active: &mut HashMap<i64, ActiveTask>,
    overrides: &mut HashMap<i64, DownloadOverrides>,
    paused: bool,
    max_parallel: usize,
    cooldown_secs: u32,
    cmd_tx: &mpsc::Sender<DownloadCommand>,
    force: bool,
) {
    if paused && !force {
        return;
    }
    while active.len() < max_parallel {
        let Some(id) = queue.pop_front() else {
            break;
        };
        if active.contains_key(&id) {
            continue;
        }

        let changed = match set_status(db.clone(), id, DownloadStatus::Downloading).await {
            Ok(c) => c,
            Err(err) => {
                emit_event(
                    app,
                    DownloadEvent::Message {
                        id,
                        message: format!("Failed to mark downloading: {err}"),
                    },
                );
                continue;
            }
        };
        if changed {
            emit_event(
                app,
                DownloadEvent::StatusChanged {
                    id,
                    status: DownloadStatus::Downloading,
                },
            );
        }

        let app_clone = app.clone();
        let tx_clone = cmd_tx.clone();
        let db_clone = db.clone();
        let opts = overrides.remove(&id);
        let handle = tauri::async_runtime::spawn(async move {
            if cooldown_secs > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(cooldown_secs as u64)).await;
            }
            match run_download_with_progress(&app_clone, db_clone.clone(), id, opts).await {
                Ok(path) => {
                    let _ = set_status(db_clone.clone(), id, DownloadStatus::Done).await;
                    let _ = set_last_error(db_clone.clone(), id, None).await;
                    let final_path = path.unwrap_or_default();
                    let _ = mark_download_done(db_clone.clone(), id, &final_path).await;
                    emit_event(
                        &app_clone,
                        DownloadEvent::StatusChanged {
                            id,
                            status: DownloadStatus::Done,
                        },
                    );
                }
                Err(err_msg) => {
                    let _ = set_last_error(db_clone.clone(), id, Some(err_msg.clone())).await;
                    let _ = set_status(db_clone.clone(), id, DownloadStatus::Error).await;
                    emit_event(
                        &app_clone,
                        DownloadEvent::Message {
                            id,
                            message: err_msg.clone(),
                        },
                    );
                    emit_event(
                        &app_clone,
                        DownloadEvent::StatusChanged {
                            id,
                            status: DownloadStatus::Error,
                        },
                    );
                }
            }
            let _ = tx_clone.send(DownloadCommand::TaskFinished { id }).await;
        });

        active.insert(id, ActiveTask { handle });
    }
}

async fn set_status(
    db: Arc<tokio::sync::Mutex<Connection>>,
    id: i64,
    status: DownloadStatus,
) -> Result<bool, String> {
    let changed = tauri::async_runtime::spawn_blocking(move || {
        let conn = db.blocking_lock();
        set_status_by_id_conn(&*conn, id, status).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Join error: {e}"))??;
    Ok(changed > 0)
}

async fn set_last_error(
    db: Arc<tokio::sync::Mutex<Connection>>,
    id: i64,
    last_error: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.blocking_lock();
        set_last_error_by_id_conn(&*conn, id, last_error.as_deref())
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Join error: {e}"))??;
    Ok(())
}

fn emit_event(app: &AppHandle, event: DownloadEvent) {
    if let Err(err) = app.emit("download_event", &event) {
        eprintln!("emit_event failed: {err}");
    } else {
        println!("[DownloadEvent] {:?}", event);
    }
}

async fn mark_download_done(
    db: Arc<tokio::sync::Mutex<Connection>>,
    id: i64,
    path: &str,
) -> Result<(), String> {
    let path = path.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.blocking_lock();
        mark_id_done_conn(&*conn, id, &path)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Join error: {e}"))??;
    Ok(())
}

async fn run_download_with_progress(
    app: &AppHandle,
    db: Arc<tokio::sync::Mutex<Connection>>,
    id: i64,
    overrides: Option<DownloadOverrides>,
) -> Result<Option<String>, String> {
    let db_clone = db.clone();
    let row = tauri::async_runtime::spawn_blocking(move || {
        let conn = db_clone.blocking_lock();
        find_download_by_id_conn(&*conn, id)
            .map_err(|e| e.to_string())
            .and_then(|row| row.ok_or_else(|| "Download not found".to_string()))
    })
    .await
    .map_err(|e| format!("Join error: {e}"))??;

    let emitter: Arc<dyn Fn(DownloadEvent) + Send + Sync> = {
        let app_clone = app.clone();
        Arc::new(move |event: DownloadEvent| {
            emit_event(&app_clone, event);
        })
    };

    pipeline::execute_download_job(app.clone(), row, overrides, emitter).await
}

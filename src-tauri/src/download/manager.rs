use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use tokio::sync::mpsc;

use tauri::{AppHandle, Emitter};

use crate::database::{Database, DownloadStatus};
use crate::download::pipeline;
use crate::settings;

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
    mut cmd_rx: mpsc::Receiver<DownloadCommand>,
    cmd_tx: mpsc::Sender<DownloadCommand>,
) {
    let mut queue: VecDeque<i64> = VecDeque::new();
    let mut active: HashMap<i64, ActiveTask> = HashMap::new();
    let mut overrides: HashMap<i64, DownloadOverrides> = HashMap::new();
    let initial_settings = settings::load_settings();
    let mut paused = !initial_settings.download_automatically;
    let mut max_parallel = initial_settings.parallel_downloads.max(1) as usize;

    while let Some(cmd) = cmd_rx.recv().await {
        let mut force_start = false;
        match cmd {
            DownloadCommand::Enqueue { ids } => {
                enqueue_ids(&app, &ids, &mut queue, &active, DownloadStatus::Queued).await;
            }
            DownloadCommand::MoveToBacklog { ids } => {
                move_to_backlog(&app, &ids, &mut queue, &mut active, &mut overrides).await;
            }
            DownloadCommand::Cancel { id } => {
                cancel_active(&app, id, &mut queue, &mut active, &mut overrides).await;
            }
            DownloadCommand::StartNow { id, overrides: ov } => {
                if let Some(custom) = ov {
                    overrides.insert(id, custom);
                }
                enqueue_ids(&app, &[id], &mut queue, &active, DownloadStatus::Queued).await;
                force_start = true;
            }
            DownloadCommand::RefreshSettings => {
                max_parallel = settings::load_settings().parallel_downloads.max(1) as usize;
            }
            DownloadCommand::SetPaused(next) => {
                paused = next;
            }
            DownloadCommand::TaskFinished { id } => {
                active.remove(&id);
            }
        }

        maybe_start_next(
            &app,
            &mut queue,
            &mut active,
            &mut overrides,
            paused,
            max_parallel,
            &cmd_tx,
            force_start,
        )
        .await;
    }
}

async fn enqueue_ids(
    app: &AppHandle,
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
        if let Err(err) = set_status(*id, status).await {
            emit_event(
                app,
                DownloadEvent::Message {
                    id: *id,
                    message: format!("Failed to set status: {err}"),
                },
            );
            continue;
        }
        emit_event(app, DownloadEvent::StatusChanged { id: *id, status });
        queue.push_back(*id);
    }
}

async fn move_to_backlog(
    app: &AppHandle,
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
        if let Err(err) = set_status(*id, DownloadStatus::Backlog).await {
            emit_event(
                app,
                DownloadEvent::Message {
                    id: *id,
                    message: format!("Failed to move to backlog: {err}"),
                },
            );
            continue;
        }
        emit_event(
            app,
            DownloadEvent::StatusChanged {
                id: *id,
                status: DownloadStatus::Backlog,
            },
        );
    }
}

async fn cancel_active(
    app: &AppHandle,
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
    if let Err(err) = set_status(id, DownloadStatus::Canceled).await {
        emit_event(
            app,
            DownloadEvent::Message {
                id,
                message: format!("Failed to cancel: {err}"),
            },
        );
        return;
    }
    emit_event(
        app,
        DownloadEvent::StatusChanged {
            id,
            status: DownloadStatus::Canceled,
        },
    );
}

async fn maybe_start_next(
    app: &AppHandle,
    queue: &mut VecDeque<i64>,
    active: &mut HashMap<i64, ActiveTask>,
    overrides: &mut HashMap<i64, DownloadOverrides>,
    paused: bool,
    max_parallel: usize,
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

        if let Err(err) = set_status(id, DownloadStatus::Downloading).await {
            emit_event(
                app,
                DownloadEvent::Message {
                    id,
                    message: format!("Failed to mark downloading: {err}"),
                },
            );
            continue;
        }
        emit_event(
            app,
            DownloadEvent::StatusChanged {
                id,
                status: DownloadStatus::Downloading,
            },
        );

        let app_clone = app.clone();
        let tx_clone = cmd_tx.clone();
        let opts = overrides.remove(&id);
        let handle = tauri::async_runtime::spawn(async move {
            match run_download_with_progress(&app_clone, id, opts).await {
                Ok(path) => {
                    let _ = set_status(id, DownloadStatus::Done).await;
                    if let Ok(db) = Database::new() {
                        let _ = db.mark_id_done(id, &path.unwrap_or_default());
                    }
                    emit_event(
                        &app_clone,
                        DownloadEvent::StatusChanged {
                            id,
                            status: DownloadStatus::Done,
                        },
                    );
                }
                Err(err_msg) => {
                    let _ = set_status(id, DownloadStatus::Error).await;
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

async fn set_status(id: i64, status: DownloadStatus) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.set_status_by_id(id, status)
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

async fn run_download_with_progress(
    app: &AppHandle,
    id: i64,
    overrides: Option<DownloadOverrides>,
) -> Result<Option<String>, String> {
    let row = tauri::async_runtime::spawn_blocking(move || {
        let db = Database::new().map_err(|e| e.to_string())?;
        db.find_download_by_id(id)
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

//! Asynchronous HTTP API service (axum) for Daily Digest (Phase 35).
//!
//! Provides endpoints for multi-container architectures and bot integrations:
//! - `GET /health`: Readiness and service status
//! - `POST /generate`: Non-blocking generation dispatch returning `202 Accepted` + `task_id`
//! - `GET /tasks/{task_id}`: Background task status and result polling
//! - `GET /tasks`: List all recent generation tasks
//! - Webhook dispatch with optional `X-Secret-Token` authentication header

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::export::DigestExporter;
use crate::pipeline::DigestPipeline;

static TASK_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Health check response model.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub timestamp: String,
    pub summarization_enabled: bool,
}

/// Request payload for asynchronous digest generation.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GenerateRequest {
    pub hours: Option<i64>,
    pub format: Option<String>,
    pub output_dir: Option<PathBuf>,
    pub enable_summarization: Option<bool>,
    pub webhook_url: Option<String>,
    pub webhook_secret_token: Option<String>,
}

/// Accepted response returned immediately upon generation dispatch.
#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub task_id: String,
    pub status: String,
    pub message: String,
}

/// Execution metadata for completed generation tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub total_collected: usize,
    pub total_unique: usize,
    pub time_window_hours: i64,
    pub output_path: String,
    pub items_count: usize,
    pub clusters_count: usize,
}

/// Status response for background generation tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub task_id: String,
    pub status: String, // "pending", "running", "completed", "failed"
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<TaskResult>,
}

/// Internal in-memory task tracking record.
#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub task_id: String,
    pub request: GenerateRequest,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub result: Option<TaskResult>,
}

impl TaskRecord {
    pub fn new(task_id: String, request: GenerateRequest) -> Self {
        Self {
            task_id,
            request,
            status: "pending".to_string(),
            created_at: Utc::now(),
            completed_at: None,
            error: None,
            result: None,
        }
    }

    pub fn to_response(&self) -> TaskStatusResponse {
        TaskStatusResponse {
            task_id: self.task_id.clone(),
            status: self.status.clone(),
            created_at: self.created_at.to_rfc3339(),
            completed_at: self.completed_at.map(|dt| dt.to_rfc3339()),
            error: self.error.clone(),
            result: self.result.clone(),
        }
    }
}

/// Shared application state for axum handlers.
pub struct AppState {
    pub config: AppConfig,
    pub tasks: RwLock<HashMap<String, TaskRecord>>,
}

/// Create the axum routing tree.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/generate", post(generate_digest))
        .route("/tasks/:task_id", get(get_task_status))
        .route("/tasks", get(list_tasks))
        .with_state(state)
}

/// Health check endpoint to probe service readiness.
async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "daily-digest".to_string(),
        timestamp: Utc::now().to_rfc3339(),
        summarization_enabled: state.config.summarization.enabled,
    })
}

/// Non-blocking digest generation endpoint returning 202 Accepted.
async fn generate_digest(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GenerateRequest>,
) -> (StatusCode, Json<GenerateResponse>) {
    let task_id = format!(
        "task_{}_{:08x}",
        Utc::now().timestamp_millis(),
        TASK_COUNTER.fetch_add(1, Ordering::Relaxed)
    );

    let record = TaskRecord::new(task_id.clone(), payload.clone());
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task_id.clone(), record);
    }

    // Spawn execution in background task
    let bg_state = state.clone();
    let bg_task_id = task_id.clone();
    tokio::spawn(async move {
        run_background_task(bg_state, bg_task_id).await;
    });

    (
        StatusCode::ACCEPTED,
        Json(GenerateResponse {
            task_id,
            status: "accepted".to_string(),
            message: "Digest generation scheduled in background".to_string(),
        }),
    )
}

/// Query status of a specific background task.
async fn get_task_status(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    let tasks = state.tasks.read().await;
    match tasks.get(&task_id) {
        Some(record) => Ok(Json(record.to_response())),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "detail": format!("Task '{}' not found", task_id)
            })),
        )),
    }
}

/// List all registered background tasks.
async fn list_tasks(State(state): State<Arc<AppState>>) -> Json<Vec<TaskStatusResponse>> {
    let tasks = state.tasks.read().await;
    let mut list: Vec<TaskStatusResponse> = tasks.values().map(|r| r.to_response()).collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(list)
}

/// Execute a background generation task and notify webhook caller if requested.
async fn run_background_task(state: Arc<AppState>, task_id: String) {
    // 1. Mark task running
    let request = {
        let mut tasks = state.tasks.write().await;
        let Some(record) = tasks.get_mut(&task_id) else { return; };
        record.status = "running".to_string();
        record.request.clone()
    };

    tracing::info!("Starting background digest task {}", task_id);

    // 2. Run pipeline
    let pipeline = DigestPipeline::new(state.config.clone());
    let result = pipeline
        .run(
            request.hours,
            request.format.as_deref(),
            request.output_dir.clone(),
            request.enable_summarization,
            None,
        )
        .await;

    // 3. Record task completion or failure
    let completed_record = {
        let mut tasks = state.tasks.write().await;
        let Some(record) = tasks.get_mut(&task_id) else { return; };
        record.completed_at = Some(Utc::now());

        match result {
            Ok(pipeline_res) => {
                record.status = "completed".to_string();
                record.result = Some(TaskResult {
                    total_collected: pipeline_res.total_collected,
                    total_unique: pipeline_res.total_unique,
                    time_window_hours: pipeline_res.time_window_hours,
                    output_path: pipeline_res.output_path.to_string_lossy().to_string(),
                    items_count: pipeline_res.items.len(),
                    clusters_count: pipeline_res.clusters_found,
                });
                tracing::info!("Background digest task {} completed successfully", task_id);
            }
            Err(err) => {
                record.status = "failed".to_string();
                record.error = Some(err.to_string());
                tracing::error!("Background digest task {} failed: {:#}", task_id, err);
            }
        }

        record.clone()
    };

    // 4. Dispatch optional webhook notification
    dispatch_webhook(&completed_record).await;
}

/// Deliver completion notification to webhook URL with optional X-Secret-Token authentication.
async fn dispatch_webhook(record: &TaskRecord) {
    let Some(url) = &record.request.webhook_url else { return; };
    if url.trim().is_empty() {
        return;
    }

    let payload = record.to_response();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let Ok(client) = client else { return; };

    let mut req = client.post(url).json(&payload);
    if let Some(token) = &record.request.webhook_secret_token {
        if !token.trim().is_empty() {
            req = req.header("X-Secret-Token", token.trim());
        }
    }

    tracing::info!(
        "Dispatching webhook notification for task {} to {}",
        record.task_id,
        url
    );

    match req.send().await {
        Ok(resp) => {
            tracing::info!(
                "Webhook for task {} returned HTTP {}",
                record.task_id,
                resp.status()
            );
        }
        Err(err) => {
            tracing::warn!(
                "Failed to deliver webhook for task {} to {}: {}",
                record.task_id,
                url,
                err
            );
        }
    }
}

/// Start axum web server listening on configured host and port.
pub async fn run_server(config: AppConfig) -> anyhow::Result<()> {
    let host = config.server.host.clone();
    let port = config.server.port;
    let addr = format!("{}:{}", host, port);

    // Startup: clean up orphan temporary files to prevent corrupted state
    let exporter = DigestExporter::new(config.output.clone(), true);
    exporter.cleanup_temporary_files();

    let state = Arc::new(AppState {
        config,
        tasks: RwLock::new(HashMap::new()),
    });

    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Daily Digest API server listening on http://{}", addr);
    println!("\nDaily Digest API server listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        let mut config = AppConfig::default();
        config.server.host = "127.0.0.1".to_string();
        config.server.port = 0;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(AppState {
            config,
            tasks: RwLock::new(HashMap::new()),
        });
        let app = create_router(state);

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{}/health", port))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body: HealthResponse = resp.json().await.unwrap();
        assert_eq!(body.status, "ok");
        assert_eq!(body.service, "daily-digest");
    }

    #[tokio::test]
    async fn test_generate_and_task_polling() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.output.directory = temp_dir.path().to_path_buf();
        config.sources.clear(); // Empty sources for fast instant run

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(AppState {
            config,
            tasks: RwLock::new(HashMap::new()),
        });
        let app = create_router(state);

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = reqwest::Client::new();
        let gen_resp = client
            .post(format!("http://127.0.0.1:{}/generate", port))
            .json(&serde_json::json!({"hours": 24}))
            .send()
            .await
            .unwrap();

        assert_eq!(gen_resp.status(), StatusCode::ACCEPTED);
        let gen_body: GenerateResponse = gen_resp.json().await.unwrap();
        assert_eq!(gen_body.status, "accepted");
        let task_id = gen_body.task_id;

        // Poll task status
        let poll_resp = client
            .get(format!("http://127.0.0.1:{}/tasks/{}", port, task_id))
            .send()
            .await
            .unwrap();

        assert_eq!(poll_resp.status(), StatusCode::OK);
        let status_body: TaskStatusResponse = poll_resp.json().await.unwrap();
        assert_eq!(status_body.task_id, task_id);
    }
}

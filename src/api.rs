use std::{path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    MAX_BLOG_BYTES,
    jobs::{ARTIFACTS, RunRequest, Studio},
};

type AppState = State<Arc<Studio>>;

pub fn router(state: Arc<Studio>) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/studio.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/studio.css"),
                )
            }),
        )
        .route(
            "/studio.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/studio.js"),
                )
            }),
        )
        .route(
            "/examples/x-memory.blog",
            get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
                        (
                            header::CONTENT_DISPOSITION,
                            "attachment; filename=\"x-memory.blog\"",
                        ),
                    ],
                    include_str!("../examples/x-memory.blog"),
                )
            }),
        )
        .route("/api/health", get(health))
        .route("/api/jobs", get(list).post(submit))
        .route("/api/jobs/{id}", get(report).delete(remove))
        .route("/api/jobs/{id}/cancel", post(cancel))
        .route("/api/jobs/{id}/replay", get(replay))
        .route("/api/jobs/{id}/artifacts/{name}", get(artifact))
        .route("/runs/{id}/player", get(player_page))
        .route("/player/{*name}", get(player_asset))
        .layer(DefaultBodyLimit::max(MAX_BLOG_BYTES * 2 + 4096))
        .with_state(state)
}

fn error(status: StatusCode, code: &str, message: impl ToString) -> Response {
    (
        status,
        Json(json!({"error": {"code":code,"message":message.to_string()}})),
    )
        .into_response()
}

async fn health(State(state): AppState) -> Json<Value> {
    Json(
        json!({"schema":1,"version":env!("CARGO_PKG_VERSION"),"upload_format":"blog",
        "max_upload_bytes":MAX_BLOG_BYTES,"max_jobs":state.config.max_jobs,
        "targets":state.targets.keys().collect::<Vec<_>>(),"default_target":state.default_target,"bloq_revision":crate::BLOQ_REVISION,"player_revision":crate::PLAYER_REVISION}),
    )
}

async fn list(State(state): AppState) -> Json<Value> {
    let jobs: Vec<_> = state.jobs.read().await.values().cloned().collect();
    let mut reports = Vec::new();
    for job in jobs {
        reports.push(job.report.lock().await.clone());
    }
    reports.sort_by_key(|report| report.submitted_at_ms);
    Json(json!({"jobs": reports}))
}

async fn submit(
    State(state): AppState,
    request: Result<Json<RunRequest>, JsonRejection>,
) -> Response {
    let request = match request {
        Ok(Json(request)) => request,
        Err(error_value) => {
            return error(
                error_value.status(),
                "invalid_request",
                error_value.body_text(),
            );
        }
    };
    if let Err(reason) = request.validate() {
        return error(StatusCode::BAD_REQUEST, "invalid_input", reason);
    }
    if request
        .target
        .as_ref()
        .is_some_and(|id| !state.targets.contains_key(id))
    {
        return error(
            StatusCode::BAD_REQUEST,
            "unknown_target",
            "Unknown target selection.",
        );
    }
    match state.submit(request).await {
        Ok(job) => (StatusCode::ACCEPTED, Json(job.report.lock().await.clone())).into_response(),
        Err(reason) if reason.to_string().starts_with("Job capacity") => {
            error(StatusCode::SERVICE_UNAVAILABLE, "capacity_full", reason)
        }
        Err(reason) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "submission_failed",
            reason,
        ),
    }
}

async fn report(State(state): AppState, Path(id): Path<Uuid>) -> Response {
    match state.get(id).await {
        Some(job) => Json(job.report.lock().await.clone()).into_response(),
        None => error(StatusCode::NOT_FOUND, "unknown_job", "Run not found."),
    }
}

async fn cancel(State(state): AppState, Path(id): Path<Uuid>) -> Response {
    let Some(job) = state.get(id).await else {
        return error(StatusCode::NOT_FOUND, "unknown_job", "Run not found.");
    };
    if job.report.lock().await.stage.terminal() {
        return error(
            StatusCode::CONFLICT,
            "already_finished",
            "Run is already finished.",
        );
    }
    job.cancel.send_replace(true);
    (
        StatusCode::ACCEPTED,
        Json(json!({"id":id,"cancellation_requested":true})),
    )
        .into_response()
}

async fn remove(State(state): AppState, Path(id): Path<Uuid>) -> Response {
    let mut jobs = state.jobs.write().await;
    let Some(job) = jobs.get(&id) else {
        return error(StatusCode::NOT_FOUND, "unknown_job", "Run not found.");
    };
    if !job.report.lock().await.stage.terminal() {
        return error(
            StatusCode::CONFLICT,
            "run_active",
            "Cancel the run and wait for completion before removing it.",
        );
    }
    if let Err(reason) = tokio::fs::remove_dir_all(&job.directory).await {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "removal_failed", reason);
    }
    jobs.remove(&id);
    StatusCode::NO_CONTENT.into_response()
}

async fn serve_file(path: PathBuf, mime: &'static str, download: Option<&str>) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let mut response = (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, "no-store"),
                    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                ],
                Body::from(bytes),
            )
                .into_response();
            if let Some(name) = download {
                response.headers_mut().insert(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{name}\"").parse().unwrap(),
                );
            }
            response
        }
        Err(_) => error(
            StatusCode::NOT_FOUND,
            "missing_artifact",
            "Artifact is unavailable.",
        ),
    }
}

async fn player_asset(State(state): AppState, Path(name): Path<String>) -> Response {
    let Some((_, mime)) = crate::PLAYER_ASSETS
        .iter()
        .find(|(asset, _)| *asset == name)
    else {
        return error(
            StatusCode::NOT_FOUND,
            "unknown_asset",
            "Player asset not found.",
        );
    };
    serve_file(state.config.player_root.join(name), mime, None).await
}

async fn artifact(State(state): AppState, Path((id, name)): Path<(Uuid, String)>) -> Response {
    if !ARTIFACTS.contains(&name.as_str()) {
        return error(
            StatusCode::NOT_FOUND,
            "unknown_artifact",
            "Artifact not found.",
        );
    }
    let Some(job) = state.get(id).await else {
        return error(StatusCode::NOT_FOUND, "unknown_job", "Run not found.");
    };
    if !job.report.lock().await.stage.terminal() {
        return error(
            StatusCode::CONFLICT,
            "run_active",
            "Artifacts become available when the run finishes.",
        );
    }
    let mime = if name.ends_with(".json") {
        "application/json"
    } else if name.ends_with(".elf") || name.ends_with(".bc") || name.ends_with(".bin") {
        "application/octet-stream"
    } else {
        "text/plain; charset=utf-8"
    };
    serve_file(job.directory.join(&name), mime, Some(&name)).await
}

async fn replay(State(state): AppState, Path(id): Path<Uuid>) -> Response {
    let Some(job) = state.get(id).await else {
        return error(StatusCode::NOT_FOUND, "unknown_job", "Run not found.");
    };
    let report = job.report.lock().await;
    if !report.stage.terminal() || !report.replay_ready {
        return error(
            StatusCode::CONFLICT,
            "replay_unavailable",
            "A validated replay is not available for this run.",
        );
    }
    drop(report);
    serve_file(job.directory.join("replay.json"), "application/json", None).await
}

async fn player_page(State(state): AppState, Path(id): Path<Uuid>) -> Response {
    let Some(job) = state.get(id).await else {
        return error(StatusCode::NOT_FOUND, "unknown_job", "Run not found.");
    };
    let report = job.report.lock().await;
    if !report.stage.terminal() || !report.replay_ready {
        return error(
            StatusCode::CONFLICT,
            "replay_unavailable",
            "Run has no validated replay.",
        );
    }
    Html(
        include_str!("../web/player.html").replace("{{BUNDLE}}", &format!("/api/jobs/{id}/replay")),
    )
    .into_response()
}

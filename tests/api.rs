use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use clap::Parser;
use http_body_util::BodyExt;
use qsbit_studio::{Config, api::router, jobs::Studio};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn setup(max_jobs: u32) -> (tempfile::TempDir, Arc<Studio>, Router) {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::parse_from(["test"]);
    config.player_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("external/qsbit-sim/web");
    config.work_dir = directory.path().to_owned();
    config.exporter = Some("/bin/false".into());
    config.max_jobs = max_jobs;
    let state = Studio::new(config).await.unwrap();
    (directory, state.clone(), router(state))
}

async fn request(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    )
}

fn upload() -> Value {
    json!({"filename":"memory.blog","source":"BLOG 1.0\n0: ZXX [0,0,0]","backend":"mock","distance":3,"seed":1})
}

#[tokio::test]
async fn player_assets_are_the_exact_simulator_sources() {
    let (_directory, state, app) = setup(1).await;
    for (filename, mime) in qsbit_studio::PLAYER_ASSETS {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/player/{filename}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], *mime);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            bytes.as_ref(),
            std::fs::read(state.config.player_root.join(filename)).unwrap()
        );
    }
    let (status, health) = request(&app, "GET", "/api/health", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["player_revision"], qsbit_studio::PLAYER_REVISION);
    assert_eq!(health["upload_format"], "blog");
    state.shutdown().await;
}

#[tokio::test]
async fn malformed_uploads_are_rejected_before_admission() {
    let (_directory, state, app) = setup(1).await;
    for body in [
        json!({"filename":"../x.blog","source":"BLOG 1.0"}),
        json!({"filename":"x.blog","source":"BLOG 1.0","backend":"shell"}),
        json!({"filename":"x.blog","source":"BLOG 1.0","seed":-1}),
        json!({"filename":"x.blog","source":"BLOG 1.0","extra":true}),
    ] {
        let (status, body) = request(&app, "POST", "/api/jobs", body).await;
        assert!(status.is_client_error());
        assert!(body["error"]["code"].is_string());
    }
    assert!(state.jobs.read().await.is_empty());
    state.shutdown().await;
}

#[tokio::test]
async fn failed_tools_keep_artifacts_capacity_is_observable_and_removal_releases_it() {
    let (_directory, state, app) = setup(1).await;
    let (status, first) = request(&app, "POST", "/api/jobs", upload()).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let id = first["id"].as_str().unwrap();
    let (status, body) = request(&app, "POST", "/api/jobs", upload()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "capacity_full");
    let report = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (_, report) = request(&app, "GET", &format!("/api/jobs/{id}"), Value::Null).await;
            if report["stage"] == "failed" {
                break report;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(report["failed_stage"], "exporting");
    assert_eq!(report["replay_ready"], false);
    assert!(
        report["artifacts"]
            .as_array()
            .unwrap()
            .contains(&json!("export.log"))
    );
    assert!(report["host_duration_ms"].is_number());
    assert_eq!(
        request(&app, "GET", &format!("/api/jobs/{id}/replay"), Value::Null)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("/api/jobs/{id}/artifacts/Cargo.toml"),
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, "DELETE", &format!("/api/jobs/{id}"), Value::Null)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert!(state.jobs.read().await.is_empty());
    assert_eq!(
        request(&app, "POST", "/api/jobs", upload()).await.0,
        StatusCode::ACCEPTED
    );
    state.shutdown().await;
    assert!(!state.directory.exists());
}

#[tokio::test]
async fn body_size_limit_prevents_large_uploads() {
    let (_directory, state, app) = setup(1).await;
    let mut body = upload();
    body["source"] = format!("BLOG 1.0\n{}", "x".repeat(qsbit_studio::MAX_BLOG_BYTES * 3)).into();
    let (status, body) = request(&app, "POST", "/api/jobs", body).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(state.jobs.read().await.is_empty());
    state.shutdown().await;
}

#[tokio::test]
async fn selected_venv_interpreter_keeps_its_symlink_path() {
    let directory = tempfile::tempdir().unwrap();
    let interpreter = directory.path().join("python");
    std::os::unix::fs::symlink("/bin/sh", &interpreter).unwrap();
    let mut config = Config::parse_from(["test"]);
    config.player_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("external/qsbit-sim/web");
    config.work_dir = directory.path().join("runs");
    config.python = Some(interpreter.clone());
    let state = Studio::new(config).await.unwrap();
    assert_eq!(state.config.python.as_ref(), Some(&interpreter));
    state.shutdown().await;
}

fn mock_tool(directory: &std::path::Path, name: &str, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join(name);
    std::fs::write(&path, format!("#!/bin/sh\nset -e\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

async fn terminal_report(app: &Router, id: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (_, report) = request(app, "GET", &format!("/api/jobs/{id}"), Value::Null).await;
            if matches!(
                report["stage"].as_str(),
                Some("completed" | "failed" | "cancelled")
            ) {
                break report;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn model_faults_and_nonzero_program_status_keep_validated_replays() {
    for (kind, success, exit_code, child_status) in [
        ("Fault", false, Value::Null, 1),
        ("SimulationCompleted", true, json!(2), 0),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::parse_from(["test"]);
        config.player_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("external/qsbit-sim/web");
        config.work_dir = directory.path().join("runs");
        config.exporter = Some(mock_tool(
            directory.path(),
            "export",
            r#"printf '%s' '{"schema":1,"qubits":1,"output_labels":[]}' > export.json"#,
        ));
        config.compiler = mock_tool(
            directory.path(),
            "compiler",
            r#"printf '%s' '{"schema":1,"profile":{"seed":0}}' > program.run.json"#,
        );
        let summary = json!({"schema":1,"success":success,"exit_code":exit_code,"stop_tick":5,"backend":"mock"});
        let events = [
            json!({"schema":1,"kind":"SessionStarted","tick":0}),
            json!({"schema":1,"kind":kind,"tick":5}),
        ];
        let script = format!(
            "cat > summary.json <<'SUMMARY'\n{summary}\nSUMMARY\ncat > trace.jsonl <<'TRACE'\n{}\n{}\nTRACE\nexit {child_status}",
            events[0], events[1]
        );
        config.simulator = mock_tool(directory.path(), "simulator", &script);
        let state = Studio::new(config).await.unwrap();
        let app = router(state.clone());
        let (status, submitted) = request(&app, "POST", "/api/jobs", upload()).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let id = submitted["id"].as_str().unwrap();
        let report = terminal_report(&app, id).await;
        assert_eq!(report["stage"], "failed");
        assert_eq!(report["failed_stage"], "simulating");
        assert_eq!(report["replay_ready"], true);
        assert_eq!(report["metrics"]["simulator_success"], success);
        assert_eq!(report["metrics"]["program_exit_code"], exit_code);
        assert_eq!(report["metrics"]["outputs"], json!([]));
        let (status, bundle) =
            request(&app, "GET", &format!("/api/jobs/{id}/replay"), Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(bundle["examples"][0]["events"][1]["kind"], kind);
        state.shutdown().await;
    }
}

#[tokio::test]
async fn queued_and_running_jobs_can_be_cancelled_without_starting_more_work() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::parse_from(["test"]);
    config.player_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("external/qsbit-sim/web");
    config.work_dir = directory.path().join("runs");
    config.max_running = 1;
    config.exporter = Some(mock_tool(directory.path(), "export", "sleep 30"));
    let state = Studio::new(config).await.unwrap();
    let app = router(state.clone());
    let (_, first) = request(&app, "POST", "/api/jobs", upload()).await;
    let first_id = first["id"].as_str().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if request(&app, "GET", &format!("/api/jobs/{first_id}"), Value::Null)
                .await
                .1["stage"]
                == "exporting"
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (_, second) = request(&app, "POST", "/api/jobs", upload()).await;
    let second_id = second["id"].as_str().unwrap();
    assert_eq!(second["stage"], "queued");
    assert_eq!(
        request(
            &app,
            "DELETE",
            &format!("/api/jobs/{first_id}"),
            Value::Null
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("/api/jobs/{second_id}/cancel"),
            Value::Null
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    let report = terminal_report(&app, second_id).await;
    assert_eq!(report["stage"], "cancelled");
    assert_eq!(report["failed_stage"], "queued");
    assert_eq!(report["stage_host_ms"], json!({}));
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("/api/jobs/{first_id}/cancel"),
            Value::Null
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    let report = terminal_report(&app, first_id).await;
    assert_eq!(report["stage"], "cancelled");
    assert_eq!(report["failed_stage"], "exporting");
    assert_eq!(report["replay_ready"], false);
    state.shutdown().await;
}

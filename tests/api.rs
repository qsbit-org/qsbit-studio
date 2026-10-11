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
    configure_runtime(&mut config, directory.path());
    config.work_dir = Some(directory.path().to_owned());
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
async fn player_assets_are_the_exact_discovered_files() {
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
            std::fs::read(state.player_root.join(filename)).unwrap()
        );
    }
    let (status, health) = request(&app, "GET", "/api/health", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["installation"]["revision"], "test-build");
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
async fn installation_discovery_rejects_missing_resources_and_unknown_interfaces() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::parse_from(["test"]);
    configure_runtime(&mut config, directory.path());
    config.simulator = "/bin/true".into();
    assert!(Studio::new(config.clone()).await.is_err());
    configure_runtime(&mut config, directory.path());
    let path = directory.path().join("installation.json");
    let mut info: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    info["player_api"] = 999.into();
    std::fs::write(&path, info.to_string()).unwrap();
    assert!(Studio::new(config.clone()).await.is_err());
    info["player_api"] = 1.into();
    std::fs::write(path, info.to_string()).unwrap();
    std::fs::remove_file(directory.path().join("web/trace-model.js")).unwrap();
    assert!(Studio::new(config).await.is_err());
}

#[tokio::test]
async fn core_only_simulator_rejects_stim_before_creating_a_job() {
    let (_directory, state, app) = setup(1).await;
    let mut body = upload();
    body["backend"] = "stim".into();
    let (status, response) = request(&app, "POST", "/api/jobs", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response["error"]["code"], "backend_unavailable");
    assert!(state.jobs.read().await.is_empty());
    state.shutdown().await;
}

fn configure_runtime(config: &mut Config, directory: &std::path::Path) {
    let web = directory.join("web");
    for (asset, _) in qsbit_studio::PLAYER_ASSETS {
        let path = web.join(asset);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("fixture {asset}")).unwrap();
    }
    let info = json!({"schema":1,"version":"test","revision":"test-build", "installed":true,
        "player_api":1,"web_root":web,"python_enabled":false,"python":null,"backend":null});
    std::fs::write(directory.join("installation.json"), info.to_string()).unwrap();
    config.compiler = "/bin/true".into();
    config.exporter = Some("/bin/false".into());
    config.simulator = mock_tool(directory, "simulator-default", "exit 1");
}

fn mock_tool(directory: &std::path::Path, name: &str, script: &str) -> PathBuf {
    let path = directory.join(name);
    let contents = format!(
        "#!/bin/sh\nset -e\nif [ \"${{1:-}}\" = --installation-info ]; then cat \"$(dirname -- \"$0\")/installation.json\"; exit; fi\n{script}\n"
    );
    // Write in a child: another parallel test's fork must not inherit a writable
    // descriptor to this executable, which would make exec fail with ETXTBSY.
    let status = std::process::Command::new("/bin/sh")
        .args([
            "-c",
            "printf '%s' \"$2\" > \"$1\" && chmod 755 \"$1\"",
            "write-fixture",
        ])
        .arg(&path)
        .arg(contents)
        .status()
        .unwrap();
    assert!(status.success());
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
        configure_runtime(&mut config, directory.path());
        config.work_dir = Some(directory.path().join("runs"));
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
    configure_runtime(&mut config, directory.path());
    config.work_dir = Some(directory.path().join("runs"));
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

#[tokio::test]
async fn target_selection_snapshots_configuration_and_rejects_unknown_ids() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("ideal.json");
    let second = directory.path().join("device.json");
    std::fs::write(&first, r#"{"schema":1,"name":"ideal"}"#).unwrap();
    let device = br#"{"schema":1,"name":"device","ports":4}"#;
    std::fs::write(&second, device).unwrap();
    let mut config = Config::parse_from(["test"]);
    configure_runtime(&mut config, directory.path());
    config.targets = vec![first, second.clone()];
    config.work_dir = Some(directory.path().join("runs"));
    config.exporter = Some("/bin/false".into());
    let state = Studio::new(config).await.unwrap();
    let app = router(state.clone());
    let (_, health) = request(&app, "GET", "/api/health", Value::Null).await;
    assert_eq!(health["default_target"], "ideal");
    assert_eq!(health["targets"], json!(["device", "ideal"]));
    // Edits after startup cannot change the configuration of queued jobs.
    std::fs::write(second, "{}").unwrap();
    let mut body = upload();
    body["target"] = "device".into();
    let (status, report) = request(&app, "POST", "/api/jobs", body.clone()).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(report["target"], "device");
    let id = report["id"].as_str().unwrap();
    let job = state.get(id.parse().unwrap()).await.unwrap();
    assert_eq!(
        std::fs::read(job.directory.join("target.json")).unwrap(),
        device
    );
    let saved: Value =
        serde_json::from_slice(&std::fs::read(job.directory.join("request.json")).unwrap())
            .unwrap();
    assert_eq!(saved["target"], "device");
    let (_, report) = request(&app, "POST", "/api/jobs", upload()).await;
    assert_eq!(report["target"], "ideal");
    body["target"] = "../unknown".into();
    assert_eq!(
        request(&app, "POST", "/api/jobs", body).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(state.jobs.read().await.len(), 2);
    state.shutdown().await;
}

#[test]
fn target_catalog_rejects_duplicate_ids_and_invalid_json() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target.json");
    std::fs::write(&path, "{}").unwrap();
    assert!(qsbit_studio::target::load_targets(&[path.clone(), path.clone()]).is_err());
    for invalid in ["[]", "invalid"] {
        std::fs::write(&path, invalid).unwrap();
        assert!(qsbit_studio::target::load_targets(std::slice::from_ref(&path)).is_err());
    }
}

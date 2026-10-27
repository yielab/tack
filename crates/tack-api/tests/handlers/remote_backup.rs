//! `POST /api/backup/remote/verify` and `POST /api/backup/remote/restore` against an
//! S3-compatible store that lives in the test: a wiremock server answering put, get and list
//! out of a map. A restore only stages `<db>.restore`; it never touches the live database.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tack_api::config::AppConfig;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const BUCKET: &str = "bkt";

type Objects = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

/// Keeps what is PUT and gives it back on GET; a GET of the bucket itself is the V2 listing.
#[derive(Clone)]
struct ObjectsResponder(Objects);

impl Respond for ObjectsResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let objects = self.0.lock().unwrap();
        let path = request.url.path().trim_start_matches('/');
        let key = path
            .strip_prefix(BUCKET)
            .unwrap_or(path)
            .trim_start_matches('/');
        match request.method.as_str() {
            "PUT" => {
                drop(objects);
                self.0
                    .lock()
                    .unwrap()
                    .insert(key.to_owned(), request.body.clone());
                ResponseTemplate::new(200).insert_header("ETag", "\"fake\"")
            }
            "DELETE" => {
                drop(objects);
                self.0.lock().unwrap().remove(key);
                ResponseTemplate::new(204)
            }
            _ if key.is_empty() => {
                let prefix = request
                    .url
                    .query_pairs()
                    .find(|(name, _)| name == "prefix")
                    .map(|(_, value)| value.into_owned())
                    .unwrap_or_default();
                let contents: String = objects
                    .iter()
                    .filter(|(key, _)| key.starts_with(&prefix))
                    .map(|(key, bytes)| {
                        format!(
                            "<Contents><Key>{key}</Key><LastModified>2026-01-01T00:00:00.000Z</LastModified><ETag>\"fake\"</ETag><Size>{}</Size><StorageClass>STANDARD</StorageClass></Contents>",
                            bytes.len()
                        )
                    })
                    .collect();
                ResponseTemplate::new(200).set_body_raw(
                    format!(
                        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>{BUCKET}</Name><Prefix>{prefix}</Prefix><IsTruncated>false</IsTruncated>{contents}</ListBucketResult>"
                    ),
                    "application/xml",
                )
            }
            _ => match objects.get(key) {
                Some(bytes) => ResponseTemplate::new(200)
                    .insert_header("ETag", "\"fake\"")
                    .insert_header("Last-Modified", "Thu, 01 Jan 2026 00:00:00 GMT")
                    .set_body_bytes(bytes.clone()),
                None => ResponseTemplate::new(404).set_body_raw(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Error><Code>NoSuchKey</Code><Message>missing</Message></Error>",
                    "application/xml",
                ),
            },
        }
    }
}

struct Remote {
    /// Held so the store keeps answering.
    _server: MockServer,
    objects: Objects,
    app: axum::Router,
    db_path: PathBuf,
    _dir: tempfile::TempDir,
}

impl Remote {
    /// An app on a file database whose backup destination is the in-test store.
    async fn start() -> Remote {
        let server = MockServer::start().await;
        let objects = Objects::default();
        Mock::given(method("PUT"))
            .and(path_regex(".*"))
            .respond_with(ObjectsResponder(objects.clone()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(".*"))
            .respond_with(ObjectsResponder(objects.clone()))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path_regex(".*"))
            .respond_with(ObjectsResponder(objects.clone()))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("temporary directory");
        let db_path = dir.path().join("tack.db");
        let (app, _) = common::test_app_with_file_db_and_config(
            &format!("sqlite:{}?mode=rwc", db_path.display()),
            AppConfig {
                backup_endpoint: Some(server.uri()),
                backup_bucket: Some(BUCKET.into()),
                backup_access_key: Some("access".into()),
                backup_secret_key: Some("secret".into()),
                storage_dir: dir.path().join("storage").display().to_string(),
                ..Default::default()
            },
        )
        .await;
        Remote {
            _server: server,
            objects,
            app,
            db_path,
            _dir: dir,
        }
    }

    async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        common::send(&self.app, "POST", uri, body, &[]).await
    }

    /// `POST /api/backup/remote`; returns the manifest.
    async fn back_up(&self) -> Value {
        let (status, manifest) = self.post("/api/backup/remote", json!({})).await;
        assert_eq!(status, StatusCode::OK, "{manifest}");
        manifest
    }

    fn staged_restore(&self) -> PathBuf {
        PathBuf::from(format!("{}.restore", self.db_path.display()))
    }

    /// Rewrite the sidecar manifest of `object_key` through `edit`.
    fn edit_sidecar(&self, object_key: &str, edit: impl FnOnce(&mut Value)) {
        let sidecar = format!("{object_key}.manifest.json");
        let mut objects = self.objects.lock().unwrap();
        let mut manifest: Value = serde_json::from_slice(&objects[&sidecar]).unwrap();
        edit(&mut manifest);
        objects.insert(sidecar, serde_json::to_vec(&manifest).unwrap());
    }
}

fn message(body: &Value) -> &str {
    body["error"]["message"].as_str().unwrap_or_default()
}

#[tokio::test]
async fn verify_and_restore_refuse_when_no_destination_is_configured() {
    let tmp = tempfile::tempdir().expect("temporary directory");
    let db_url = format!("sqlite:{}?mode=rwc", tmp.path().join("t.db").display());
    let (app, _) = common::test_app_with_file_db(&db_url).await;
    for uri in ["/api/backup/remote/verify", "/api/backup/remote/restore"] {
        let (status, body) = common::send(&app, "POST", uri, json!({}), &[]).await;
        assert_eq!(status, StatusCode::CONFLICT, "{uri}: {body}");
        assert!(
            message(&body).contains("remote backup is not configured"),
            "{body}"
        );
    }
    assert!(!tmp.path().join("t.db.restore").exists());
}

#[tokio::test]
async fn verify_and_restore_say_so_when_the_bucket_has_no_backup() {
    let remote = Remote::start().await;
    for uri in ["/api/backup/remote/verify", "/api/backup/remote/restore"] {
        let (status, body) = remote.post(uri, json!({})).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
        assert!(message(&body).contains("no remote backups found"), "{body}");
    }
    assert!(!remote.staged_restore().exists());
}

#[tokio::test]
async fn verify_passes_a_good_archive_and_restore_stages_its_database() {
    let remote = Remote::start().await;
    let pid = common::create_project(&remote.app, "Before the backup", "software").await;
    let manifest = remote.back_up().await;
    let key = manifest["object_key"].as_str().unwrap();

    let (status, listed) =
        common::send(&remote.app, "GET", "/api/backup/remote", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed[0]["object_key"], key);

    let (status, verdict) = remote.post("/api/backup/remote/verify", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{verdict}");
    assert_eq!(verdict["ok"], true, "{verdict}");
    assert_eq!(verdict["object_key"], key);
    assert_eq!(verdict["manifest"]["db_sha256"], manifest["db_sha256"]);
    assert!(!remote.staged_restore().exists(), "verify stages nothing");

    // Nothing has been written locally since the backup, so the restore needs no force.
    let (status, restored) = remote
        .post("/api/backup/remote/restore", json!({"key": key}))
        .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["staged"], true);
    assert_eq!(restored["restart_required"], true);
    assert_eq!(restored["object_key"], key);
    let staged = std::fs::read(remote.staged_restore()).expect("the restore is staged");
    assert!(
        staged.starts_with(b"SQLite format 3\x00"),
        "a SQLite file is staged"
    );
    assert!(
        staged
            .windows(b"Before the backup".len())
            .any(|w| w == b"Before the backup"),
        "the staged database holds the project {pid} saved at backup time"
    );
}

#[tokio::test]
async fn an_archive_whose_database_does_not_match_its_manifest_fails_verify_and_is_not_restored() {
    let remote = Remote::start().await;
    let manifest = remote.back_up().await;
    let key = manifest["object_key"].as_str().unwrap().to_owned();
    remote.edit_sidecar(&key, |m| m["db_sha256"] = json!("0".repeat(64)));

    let (status, verdict) = remote
        .post("/api/backup/remote/verify", json!({"key": key}))
        .await;
    assert_eq!(status, StatusCode::OK, "{verdict}");
    assert_eq!(verdict["ok"], false, "{verdict}");
    assert!(
        verdict["reason"]
            .as_str()
            .unwrap()
            .contains("integrity check failed"),
        "{verdict}"
    );

    let (status, body) = remote
        .post("/api/backup/remote/restore", json!({"key": key}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(message(&body).contains("integrity check failed"), "{body}");
    assert!(
        !remote.staged_restore().exists(),
        "an unverified archive is never staged"
    );
}

#[tokio::test]
async fn an_archive_from_a_newer_schema_fails_verify_and_is_not_restored() {
    let remote = Remote::start().await;
    let manifest = remote.back_up().await;
    let key = manifest["object_key"].as_str().unwrap().to_owned();
    remote.edit_sidecar(&key, |m| m["migration_version"] = json!(9999));

    let (_, verdict) = remote.post("/api/backup/remote/verify", json!({})).await;
    assert_eq!(verdict["ok"], false, "{verdict}");
    assert!(
        verdict["reason"]
            .as_str()
            .unwrap()
            .contains("ahead of the running binary")
    );

    let (status, body) = remote.post("/api/backup/remote/restore", json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        message(&body).contains("ahead of the running binary"),
        "{body}"
    );
    assert!(!remote.staged_restore().exists());
}

#[tokio::test]
async fn an_unreadable_archive_fails_verify() {
    let remote = Remote::start().await;
    let manifest = remote.back_up().await;
    let key = manifest["object_key"].as_str().unwrap().to_owned();
    remote
        .objects
        .lock()
        .unwrap()
        .insert(key.clone(), b"this is not a zstd archive".to_vec());

    let (status, verdict) = remote
        .post("/api/backup/remote/verify", json!({"key": key}))
        .await;
    assert_eq!(status, StatusCode::OK, "{verdict}");
    assert_eq!(verdict["ok"], false, "{verdict}");
    assert!(!remote.staged_restore().exists());
}

#[tokio::test]
async fn restoring_an_older_backup_over_newer_local_work_needs_force() {
    let remote = Remote::start().await;
    let first = remote.back_up().await;
    let second = remote.back_up().await;
    assert!(second["generation"].as_u64() > first["generation"].as_u64());
    let old_key = first["object_key"].as_str().unwrap();

    let (status, body) = remote
        .post("/api/backup/remote/restore", json!({"key": old_key}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        message(&body).contains("this device has newer work"),
        "{body}"
    );
    assert!(!remote.staged_restore().exists());

    let (status, body) = remote
        .post(
            "/api/backup/remote/restore",
            json!({"key": old_key, "force": true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["object_key"], old_key);
    assert!(remote.staged_restore().exists());
}

#[tokio::test]
async fn naming_a_key_the_bucket_does_not_hold_is_refused() {
    let remote = Remote::start().await;
    remote.back_up().await;
    let (status, body) = remote
        .post(
            "/api/backup/remote/verify",
            json!({"key": "tack/missing.tar.zst"}),
        )
        .await;
    assert!(
        status.is_server_error() || status.is_client_error(),
        "{status}: {body}"
    );
    assert_ne!(body["ok"], true, "{body}");
    let (status, _) = remote
        .post(
            "/api/backup/remote/restore",
            json!({"key": "tack/missing.tar.zst"}),
        )
        .await;
    assert_ne!(status, StatusCode::OK);
    assert!(!remote.staged_restore().exists());
}

#[tokio::test]
async fn an_archive_or_manifest_that_cannot_be_read_is_a_422_with_a_code_not_a_500() {
    let remote = Remote::start().await;
    let manifest = remote.back_up().await;
    let key = manifest["object_key"].as_str().unwrap().to_owned();

    // The archive is not zstd at all.
    remote
        .objects
        .lock()
        .unwrap()
        .insert(key.clone(), b"this is not a backup".to_vec());
    let (status, body) = remote
        .post("/api/backup/remote/restore", json!({"key": key}))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "unreadable_archive", "{body}");
    assert!(message(&body).contains("cannot be read"), "{body}");
    assert!(!remote.staged_restore().exists());

    // The sidecar manifest is not JSON.
    remote
        .objects
        .lock()
        .unwrap()
        .insert(format!("{key}.manifest.json"), b"{".to_vec());
    let (status, body) = remote
        .post("/api/backup/remote/restore", json!({"key": key}))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "unreadable_archive", "{body}");
}

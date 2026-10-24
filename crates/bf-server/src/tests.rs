//! PT: Testes do roteador com um pacote sintético numa pasta temporária.
//! EN: Router tests with a synthetic package in a temporary folder.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::app::{CONTENT_SECURITY_POLICY, router};
use crate::game::GameStore;

// PT: Cria `<root>/2.1.23` com dados, nomes, atlas e um manifesto com os sha256 corretos.
// EN: Creates `<root>/2.1.23` with data, names, atlas and a manifest with the right sha256s.
pub fn write_package(root: &Path) {
    let dir = root.join("2.1.23");
    fs::create_dir_all(dir.join("locale")).unwrap();
    fs::create_dir_all(dir.join("icons")).unwrap();
    fs::create_dir_all(dir.join("raw")).unwrap();
    let files: &[(&str, &[u8])] = &[
        ("gamedata.json", br#"{"format_version":3,"game_version":"2.1.23","items":{},"fluids":{},"recipes":{},"entities":{},"tiles":{}}"#),
        ("locale/en.json", br#"{"language":"en"}"#),
        ("locale/pt-BR.json", br#"{"language":"pt-BR"}"#),
        ("icons/atlas.json", br#"{"cell":64}"#),
        ("icons/atlas-0.png", b"\x89PNG fake"),
        ("raw/data-raw-dump.json", b"{}"),
    ];
    let mut hashes = serde_json::Map::new();
    for (path, bytes) in files {
        fs::write(dir.join(path), bytes).unwrap();
        hashes.insert(
            (*path).to_owned(),
            hex::encode(Sha256::digest(bytes)).into(),
        );
    }
    let manifest = serde_json::json!({
        "game_version": "2.1.23",
        "has_icons": true,
        "files": hashes,
    });
    fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
}

fn app(root: &Path) -> axum::Router {
    let store = GameStore::load(root).unwrap();
    router(Arc::new(store), None, &root.join("no-dist"))
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, body)
}

#[tokio::test]
async fn lists_versions_and_serves_files() {
    let root = tempfile::tempdir().unwrap();
    write_package(root.path());
    let app = app(root.path());

    let (status, _, body) = get(&app, "/api/v1/game/versions").await;
    assert_eq!(status, StatusCode::OK);
    let versions: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(versions[0]["version"], "2.1.23");
    assert_eq!(versions[0]["has_icons"], true);

    let (status, headers, body) = get(&app, "/api/v1/game/2.1.23/data").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with(br#"{"format_version":3"#));
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    assert!(headers.contains_key(header::ETAG));

    let (status, _, _) = get(&app, "/api/v1/game/2.1.23/locale/pt-BR").await;
    assert_eq!(status, StatusCode::OK);
    let (status, headers, _) = get(&app, "/api/v1/assets/icons/2.1.23/atlas-0.png").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "image/png");
}

#[tokio::test]
async fn answers_not_modified_for_matching_etag() {
    let root = tempfile::tempdir().unwrap();
    write_package(root.path());
    let app = app(root.path());
    let (_, headers, _) = get(&app, "/api/v1/game/2.1.23/data").await;
    let etag = headers[header::ETAG].clone();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/game/2.1.23/data")
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn refuses_traversal_and_unknown_files() {
    let root = tempfile::tempdir().unwrap();
    write_package(root.path());
    let app = app(root.path());

    for uri in [
        "/api/v1/assets/icons/2.1.23/..%2Fgamedata.json",
        "/api/v1/assets/icons/2.1.23/%2e%2e",
        "/api/v1/assets/icons/2.1.23/.hidden",
        "/api/v1/assets/icons/..%2F2.1.23/atlas.json",
        "/api/v1/assets/icons/2.1.23/missing.png",
        "/api/v1/game/2.1.23/locale/fr",
        "/api/v1/game/2.1.23/locale/..%2F..%2Fgamedata",
        "/api/v1/game/9.9.9/data",
        "/api/v1/game/latest/data",
        "/api/v1/game/2.1.23/raw",
        "/api/v1/nothing",
    ] {
        let (status, _, body) = get(&app, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(
            !String::from_utf8_lossy(&body).contains("format_version"),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn sends_security_headers() {
    let root = tempfile::tempdir().unwrap();
    write_package(root.path());
    let app = app(root.path());
    let (_, headers, _) = get(&app, "/api/v1/health").await;
    assert_eq!(
        headers[header::CONTENT_SECURITY_POLICY],
        CONTENT_SECURITY_POLICY
    );
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
    assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
}

#[test]
fn skips_tampered_packages() {
    let root = tempfile::tempdir().unwrap();
    write_package(root.path());
    fs::write(root.path().join("2.1.23/gamedata.json"), b"tampered").unwrap();
    let store = GameStore::load(root.path()).unwrap();
    assert!(store.packages.is_empty());
}

#[tokio::test]
async fn accepts_weak_and_listed_etags() {
    let root = tempfile::tempdir().unwrap();
    write_package(root.path());
    let app = app(root.path());
    let (_, headers, _) = get(&app, "/api/v1/game/2.1.23/data").await;
    let etag = headers[header::ETAG].to_str().unwrap().to_owned();
    assert!(etag.starts_with("W/\""));
    for value in [
        etag.clone(),
        format!("\"other\", {etag}"),
        etag.trim_start_matches("W/").to_owned(),
        "*".to_owned(),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/game/2.1.23/data")
                    .header(header::IF_NONE_MATCH, value.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{value}");
    }
}

#[test]
fn ignores_folders_that_are_not_versions() {
    let root = tempfile::tempdir().unwrap();
    write_package(root.path());
    fs::create_dir_all(root.path().join(".2.1.23.staging")).unwrap();
    fs::create_dir_all(root.path().join("notes")).unwrap();
    let store = GameStore::load(root.path()).unwrap();
    assert_eq!(store.packages.len(), 1);
}

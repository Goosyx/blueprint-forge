//! PT: Testes com Postgres real (`#[sqlx::test]` cria um banco novo por teste e aplica as
//!     migrations): login, isolamento entre usuários, cotas atômicas, deduplicação, sessões,
//!     força bruta, origem e limite de corpo.
//! EN: Tests with a real Postgres (`#[sqlx::test]` creates a fresh database per test and applies
//!     the migrations): login, user isolation, atomic quotas, deduplication, sessions, brute force,
//!     origin and body limit.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use bf_blueprint::{Blueprint, BlueprintString, Entity, Position, encode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::app::router;
use crate::auth::{password, routes::insert_user};
use crate::config::{Config, Quotas, Registration};
use crate::game::GameStore;
use crate::state::AppState;

const ORIGIN: &str = "http://localhost:8080";
const PASSWORD: &str = "correct horse battery staple";

struct Harness {
    app: axum::Router,
    db: PgPool,
    _assets: tempfile::TempDir,
}

fn config(registration: Registration, quotas: Quotas) -> Config {
    Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        database_url: String::new(),
        game_assets_dir: "unused".into(),
        web_dist: "unused".into(),
        allowed_origin: ORIGIN.into(),
        registration,
        max_users: 10,
        max_db_bytes: i64::MAX,
        quotas,
        // PT: Nos testes a "conexão" é 0.0.0.0, tratada como proxy, para simular IPs com `X-Forwarded-For`.
        // EN: In tests the "connection" is 0.0.0.0, treated as a proxy, to simulate IPs with `X-Forwarded-For`.
        trusted_proxy: Some("0.0.0.0".parse().unwrap()),
    }
}

fn harness(db: PgPool, registration: Registration, quotas: Quotas) -> Harness {
    let assets = tempfile::tempdir().unwrap();
    crate::tests::write_package(assets.path());
    let store = Arc::new(GameStore::load(assets.path()).unwrap());
    let dummy = password::hash_now("dummy").unwrap();
    let state = AppState::new(
        db.clone(),
        store.clone(),
        config(registration, quotas),
        dummy,
    );
    Harness {
        app: router(store, Some(state), &assets.path().join("no-dist")),
        db,
        _assets: assets,
    }
}

async fn add_user(db: &PgPool, email: &str) -> Uuid {
    let id = Uuid::now_v7();
    let hash = password::hash_now(PASSWORD).unwrap();
    let mut tx = db.begin().await.unwrap();
    insert_user(&mut tx, id, email, &hash, "pt-BR")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Option<String>, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ORIGIN, ORIGIN);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    let request = match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => request.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::to_owned);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, cookie, json)
}

async fn login(app: &axum::Router, email: &str) -> String {
    let (status, cookie, _) = send(
        app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({"email": email, "password": PASSWORD})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    cookie.unwrap()
}

fn blueprint_text(label: &str, entities: u64) -> String {
    let content = BlueprintString::Blueprint(Blueprint {
        label: Some(label.into()),
        entities: (1..=entities)
            .map(|n| {
                Entity::new(
                    n,
                    "wooden-chest",
                    Position {
                        x: n as f64 + 0.5,
                        y: 0.5,
                    },
                )
            })
            .collect(),
        ..Blueprint::default()
    });
    encode(&content).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn login_me_logout(db: PgPool) {
    let h = harness(db, Registration::Closed, Quotas::default());
    add_user(&h.db, "a@x.com").await;

    let (status, _, _) = send(
        &h.app,
        "POST",
        "/api/v1/auth/register",
        None,
        Some(json!({"email": "new@x.com", "password": PASSWORD})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cadastro fechado / registration closed"
    );

    let (status, _, _) = send(
        &h.app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({"email": "a@x.com", "password": "wrong password!!"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send(
        &h.app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({"email": "nobody@x.com", "password": PASSWORD})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "mesma resposta / same answer"
    );

    let cookie = login(&h.app, "A@X.com").await;
    let (status, _, me) = send(&h.app, "GET", "/api/v1/auth/me", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["email"], "a@x.com");

    let (status, _, _) = send(&h.app, "POST", "/api/v1/auth/logout", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&h.app, "GET", "/api/v1/auth/me", Some(&cookie), None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "logout invalida / logout invalidates"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn users_cannot_see_each_other(db: PgPool) {
    let h = harness(db, Registration::Closed, Quotas::default());
    add_user(&h.db, "a@x.com").await;
    add_user(&h.db, "b@x.com").await;
    let a = login(&h.app, "a@x.com").await;
    let b = login(&h.app, "b@x.com").await;

    let (status, _, created) = send(
        &h.app,
        "POST",
        "/api/v1/blueprints",
        Some(&a),
        Some(json!({"source": blueprint_text("de A", 3)})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(created["title"], "de A");
    assert_eq!(created["entity_count"], 3);

    for (method, body) in [
        ("GET", None),
        ("PATCH", Some(json!({"title": "x"}))),
        ("DELETE", None),
    ] {
        let (status, _, _) = send(
            &h.app,
            method,
            &format!("/api/v1/blueprints/{id}"),
            Some(&b),
            body,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
    }
    let (_, _, list) = send(&h.app, "GET", "/api/v1/blueprints", Some(&b), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);

    let (status, _, project) = send(
        &h.app,
        "POST",
        "/api/v1/projects",
        Some(&b),
        Some(json!({"name": "P"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project = project["id"].as_str().unwrap().to_owned();
    let (status, _, _) = send(
        &h.app,
        "POST",
        &format!("/api/v1/projects/{project}/blueprints"),
        Some(&b),
        Some(json!({"blueprint_id": id})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "blueprint de outro usuário / another user's blueprint"
    );

    let (status, _, detail) = send(
        &h.app,
        "GET",
        &format!("/api/v1/blueprints/{id}"),
        Some(&a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["analyses"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn quotas_hold_under_parallel_requests(db: PgPool) {
    let quotas = Quotas {
        blueprints: 5,
        ..Quotas::default()
    };
    let h = harness(db, Registration::Closed, quotas);
    let user = add_user(&h.db, "a@x.com").await;
    let cookie = login(&h.app, "a@x.com").await;

    // PT: 20 pedidos paralelos com textos diferentes: só 5 podem passar.
    // EN: 20 parallel requests with different texts: only 5 may pass.
    let mut tasks = Vec::new();
    for index in 0..20u64 {
        let app = h.app.clone();
        let cookie = cookie.clone();
        tasks.push(tokio::spawn(async move {
            send(
                &app,
                "POST",
                "/api/v1/blueprints",
                Some(&cookie),
                Some(json!({"source": blueprint_text(&format!("bp {index}"), index + 1)})),
            )
            .await
            .0
        }));
    }
    let mut created = 0;
    for task in tasks {
        if task.await.unwrap() == StatusCode::CREATED {
            created += 1;
        }
    }
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM blueprints WHERE user_id = $1")
        .bind(user)
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert!(created <= 5, "criadas / created: {created}");
    assert_eq!(stored, i64::from(created));
    let usage: i32 = sqlx::query_scalar("SELECT blueprints FROM user_usage WHERE user_id = $1")
        .bind(user)
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(
        i64::from(usage),
        stored,
        "uso bate com o banco / usage matches the database"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn dedup_and_delete_restore_quota(db: PgPool) {
    let h = harness(db, Registration::Closed, Quotas::default());
    let user = add_user(&h.db, "a@x.com").await;
    let cookie = login(&h.app, "a@x.com").await;
    let text = blueprint_text("mesma", 2);

    let (first, _, created) = send(
        &h.app,
        "POST",
        "/api/v1/blueprints",
        Some(&cookie),
        Some(json!({"source": text})),
    )
    .await;
    let (second, _, again) = send(
        &h.app,
        "POST",
        "/api/v1/blueprints",
        Some(&cookie),
        Some(json!({"source": text})),
    )
    .await;
    assert_eq!(first, StatusCode::CREATED);
    assert_eq!(second, StatusCode::OK);
    assert_eq!(created["id"], again["id"]);
    let usage: (i32, i64) =
        sqlx::query_as("SELECT blueprints, bytes FROM user_usage WHERE user_id = $1")
            .bind(user)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(usage.0, 1);
    assert!(usage.1 > 0);

    let id = created["id"].as_str().unwrap();
    let (status, _, _) = send(
        &h.app,
        "DELETE",
        &format!("/api/v1/blueprints/{id}"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let usage: (i32, i64) =
        sqlx::query_as("SELECT blueprints, bytes FROM user_usage WHERE user_id = $1")
            .bind(user)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert_eq!(usage, (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_blueprints_are_rejected_by_the_server(db: PgPool) {
    let h = harness(db, Registration::Closed, Quotas::default());
    add_user(&h.db, "a@x.com").await;
    let cookie = login(&h.app, "a@x.com").await;
    let (status, _, body) = send(
        &h.app,
        "POST",
        "/api/v1/blueprints",
        Some(&cookie),
        Some(json!({"source": "0<script>"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "blueprint.invalid_character");
}

#[sqlx::test(migrations = "../../migrations")]
async fn sessions_are_capped_and_password_change_ends_others(db: PgPool) {
    let quotas = Quotas {
        sessions: 3,
        ..Quotas::default()
    };
    let h = harness(db, Registration::Closed, quotas);
    let user = add_user(&h.db, "a@x.com").await;
    let mut cookies = Vec::new();
    for _ in 0..5 {
        cookies.push(login(&h.app, "a@x.com").await);
        // PT: Respeita o limitador de login (rajada de 5). EN: Respects the login limiter (burst of 5).
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions WHERE user_id = $1")
        .bind(user)
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(count, 3);
    let (status, _, _) = send(&h.app, "GET", "/api/v1/auth/me", Some(&cookies[0]), None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a mais antiga saiu / the oldest was removed"
    );

    let current = cookies.last().unwrap();
    let (status, _, _) = send(
        &h.app,
        "POST",
        "/api/v1/auth/password",
        Some(current),
        Some(json!({"current": PASSWORD, "new": "another long password"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&h.app, "GET", "/api/v1/auth/me", Some(&cookies[3]), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send(&h.app, "GET", "/api/v1/auth/me", Some(current), None).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn brute_force_is_locked_and_rate_limited(db: PgPool) {
    let h = harness(db, Registration::Closed, Quotas::default());
    let user = add_user(&h.db, "a@x.com").await;
    for _ in 0..5 {
        let (status, _, _) = send(
            &h.app,
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "a@x.com", "password": "wrong password!!"})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let locked: Option<time::OffsetDateTime> =
        sqlx::query_scalar("SELECT locked_until FROM users WHERE id = $1")
            .bind(user)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert!(locked.is_some(), "conta bloqueada / account locked");
    let (status, _, _) = send(
        &h.app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({"email": "a@x.com", "password": PASSWORD})),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrations = "../../migrations")]
async fn origin_and_body_limits(db: PgPool) {
    let h = harness(db, Registration::Closed, Quotas::default());
    add_user(&h.db, "a@x.com").await;
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::from(
            json!({"email": "a@x.com", "password": PASSWORD}).to_string(),
        ))
        .unwrap();
    let response = h.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let cookie = login(&h.app, "a@x.com").await;
    let huge = "A".repeat(crate::app::MAX_BODY_BYTES + 1);
    let (status, _, _) = send(
        &h.app,
        "POST",
        "/api/v1/blueprints",
        Some(&cookie),
        Some(json!({"source": huge})),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/blueprints")
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_ENCODING, "gzip")
        .body(Body::from(vec![0x1f, 0x8b, 0, 0]))
        .unwrap();
    let response = h.app.clone().oneshot(request).await.unwrap();
    assert!(
        response.status().is_client_error(),
        "gzip no corpo é recusado / gzip body is refused"
    );
}

async fn send_from(app: &axum::Router, ip: &str, uri: &str, body: Value) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::ORIGIN, ORIGIN)
        .header("x-forwarded-for", ip)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    app.clone().oneshot(request).await.unwrap().status()
}

#[sqlx::test(migrations = "../../migrations")]
async fn parallel_wrong_logins_still_lock_and_locked_looks_like_wrong(db: PgPool) {
    let h = harness(db, Registration::Closed, Quotas::default());
    let user = add_user(&h.db, "a@x.com").await;
    let mut tasks = Vec::new();
    for index in 0..20 {
        let app = h.app.clone();
        tasks.push(tokio::spawn(async move {
            send_from(
                &app,
                &format!("10.0.0.{index}"),
                "/api/v1/auth/login",
                json!({"email": "a@x.com", "password": "wrong password!!"}),
            )
            .await
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), StatusCode::UNAUTHORIZED);
    }
    let locked: Option<time::OffsetDateTime> =
        sqlx::query_scalar("SELECT locked_until FROM users WHERE id = $1")
            .bind(user)
            .fetch_one(&h.db)
            .await
            .unwrap();
    assert!(
        locked.is_some(),
        "paralelo também bloqueia / parallel also locks"
    );

    // PT: Senha certa com a conta bloqueada recebe o mesmo 401 de uma senha errada.
    // EN: The right password on a locked account gets the same 401 as a wrong one.
    let status = send_from(
        &h.app,
        "10.0.1.1",
        "/api/v1/auth/login",
        json!({"email": "a@x.com", "password": PASSWORD}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invite_mode_does_not_reveal_accounts(db: PgPool) {
    let h = harness(db, Registration::Invite, Quotas::default());
    add_user(&h.db, "a@x.com").await;
    let fake = "A".repeat(43);
    for (index, email) in ["a@x.com", "new@x.com"].iter().enumerate() {
        let status = send_from(
            &h.app,
            &format!("10.1.0.{index}"),
            "/api/v1/auth/register",
            json!({"email": email, "password": PASSWORD, "invite": fake}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{email}");
    }

    let (token, hash) = crate::auth::session::new_token().unwrap();
    sqlx::query(
        "INSERT INTO invites (token_hash, expires_at) VALUES ($1, now() + interval '1 day')",
    )
    .bind(hash)
    .execute(&h.db)
    .await
    .unwrap();
    let status = send_from(
        &h.app,
        "10.1.1.1",
        "/api/v1/auth/register",
        json!({"email": "new@x.com", "password": PASSWORD, "invite": token}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let status = send_from(
        &h.app,
        "10.1.1.2",
        "/api/v1/auth/register",
        json!({"email": "other@x.com", "password": PASSWORD, "invite": token}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "convite de uso único / single-use invite"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn user_cap_holds_under_parallel_registrations(db: PgPool) {
    let h = harness(db, Registration::Open, Quotas::default());
    let mut tasks = Vec::new();
    for index in 0..25 {
        let app = h.app.clone();
        tasks.push(tokio::spawn(async move {
            send_from(
                &app,
                &format!("10.2.0.{index}"),
                "/api/v1/auth/register",
                json!({"email": format!("u{index}@x.com"), "password": PASSWORD}),
            )
            .await
        }));
    }
    for task in tasks {
        let _ = task.await.unwrap();
    }
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&h.db)
        .await
        .unwrap();
    assert_eq!(users, 10, "teto de 10 / cap of 10");
}

#[sqlx::test(migrations = "../../migrations")]
async fn empty_label_and_huge_options_are_handled(db: PgPool) {
    let h = harness(db, Registration::Closed, Quotas::default());
    add_user(&h.db, "a@x.com").await;
    let cookie = login(&h.app, "a@x.com").await;
    let (status, _, created) = send(
        &h.app,
        "POST",
        "/api/v1/blueprints",
        Some(&cookie),
        Some(json!({"source": blueprint_text("", 1)})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"], "Blueprint");

    let id = created["id"].as_str().unwrap();
    let exported: Vec<String> = (0..1000).map(|i| format!("item-{i:0>90}")).collect();
    let (status, _, body) = send(
        &h.app,
        "POST",
        &format!("/api/v1/blueprints/{id}/analyses"),
        Some(&cookie),
        Some(json!({"options": {"exported": exported}})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "analysis.options_too_large");
}

#[sqlx::test(migrations = "../../migrations")]
async fn full_project_still_updates_existing_items(db: PgPool) {
    let quotas = Quotas {
        blueprints_per_project: 1,
        ..Quotas::default()
    };
    let h = harness(db, Registration::Closed, quotas);
    add_user(&h.db, "a@x.com").await;
    let cookie = login(&h.app, "a@x.com").await;
    let (_, _, first) = send(
        &h.app,
        "POST",
        "/api/v1/blueprints",
        Some(&cookie),
        Some(json!({"source": blueprint_text("1", 1)})),
    )
    .await;
    let (_, _, second) = send(
        &h.app,
        "POST",
        "/api/v1/blueprints",
        Some(&cookie),
        Some(json!({"source": blueprint_text("2", 2)})),
    )
    .await;
    let (_, _, project) = send(
        &h.app,
        "POST",
        "/api/v1/projects",
        Some(&cookie),
        Some(json!({"name": "P"})),
    )
    .await;
    let project = project["id"].as_str().unwrap();
    let uri = format!("/api/v1/projects/{project}/blueprints");
    let (status, _, _) = send(
        &h.app,
        "POST",
        &uri,
        Some(&cookie),
        Some(json!({"blueprint_id": first["id"]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(
        &h.app,
        "POST",
        &uri,
        Some(&cookie),
        Some(json!({"blueprint_id": first["id"], "note": "nova"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "atualizar item existente / update existing item"
    );
    let (status, _, _) = send(
        &h.app,
        "POST",
        &uri,
        Some(&cookie),
        Some(json!({"blueprint_id": second["id"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

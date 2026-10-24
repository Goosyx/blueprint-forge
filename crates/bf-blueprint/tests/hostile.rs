//! PT: Suíte hostil: entradas maliciosas ou quebradas devem ser recusadas com o código de erro
//!     certo, rápido e sem consumir memória além dos limites.
//! EN: Hostile suite: malicious or broken inputs must be rejected with the right error code,
//!     quickly and without using memory beyond the limits.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::io::Write;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bf_blueprint::{DecodeError, Limits, decode};
use flate2::Compression;
use flate2::write::ZlibEncoder;

// PT: Monta uma string a partir de bytes JSON arbitrários, sem passar pelo modelo.
// EN: Builds a string from arbitrary JSON bytes, bypassing the model.
fn wrap(json: &[u8]) -> String {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(json).unwrap();
    format!("0{}", STANDARD.encode(encoder.finish().unwrap()))
}

// PT: Decodifica, exige erro com o código esperado e que tenha levado menos de 2 s.
// EN: Decodes, requires an error with the expected code and that it took under 2 s.
fn expect_code(text: &str, code: &str) {
    expect_code_with(text, code, &Limits::default());
}

// PT: Igual, mas sem o limite de razão de compressão, para atingir a checagem seguinte.
// EN: Same, but without the compression ratio limit, to reach the next check.
fn expect_code_without_ratio(text: &str, code: &str) {
    let limits = Limits {
        max_compression_ratio: usize::MAX,
        ..Limits::default()
    };
    expect_code_with(text, code, &limits);
}

fn expect_code_with(text: &str, code: &str, limits: &Limits) {
    let started = Instant::now();
    let error = decode(text, limits).expect_err("deveria falhar / should fail");
    assert_eq!(error.code(), code, "erro / error: {error}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "lento demais / too slow: {:?}",
        started.elapsed()
    );
}

fn entity_json(number: u64, x: &str, y: &str) -> String {
    format!(r#"{{"entity_number":{number},"name":"wooden-chest","position":{{"x":{x},"y":{y}}}}}"#)
}

fn blueprint_json(entities: &str, extra: &str) -> String {
    format!(
        r#"{{"blueprint":{{"item":"blueprint","entities":[{entities}]{extra},"version":562954249895936}}}}"#
    )
}

#[test]
fn rejects_empty_and_garbage() {
    expect_code("", "blueprint.empty");
    expect_code("   \n", "blueprint.empty");
    expect_code("0", "blueprint.empty");
    expect_code("1abc", "blueprint.unsupported_version");
    expect_code("0abc def", "blueprint.invalid_character");
    expect_code("0<script>", "blueprint.invalid_character");
    expect_code("0abc", "blueprint.invalid_compression");
    expect_code("0a", "blueprint.invalid_base64");
}

#[test]
fn rejects_oversized_text_before_decoding() {
    let huge = format!("0{}", "A".repeat(Limits::default().max_encoded_len));
    expect_code(&huge, "blueprint.too_large");
}

#[test]
fn rejects_zlib_bomb() {
    // PT: 200 MB de espaços comprimem para poucos centenas de KB.
    // EN: 200 MB of spaces compress to a few hundred KB.
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    let chunk = vec![b' '; 1024 * 1024];
    for _ in 0..200 {
        encoder.write_all(&chunk).unwrap();
    }
    let text = format!("0{}", STANDARD.encode(encoder.finish().unwrap()));
    assert!(text.len() < Limits::default().max_encoded_len);
    expect_code(&text, "blueprint.decompressed_too_large");
}

#[test]
fn rejects_suspicious_compression_ratio() {
    let limits = Limits {
        max_compression_ratio: 10,
        ..Limits::default()
    };
    let json = blueprint_json(
        &entity_json(1, "0.5", "0.5"),
        &format!(r#","description":"{}""#, " ".repeat(4000)),
    );
    let error = decode(&wrap(json.as_bytes()), &limits).unwrap_err();
    assert_eq!(error, DecodeError::CompressionRatio);
}

#[test]
fn rejects_deeply_nested_json() {
    let nested = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    let json = format!(r#"{{"blueprint":{{"item":"blueprint","schedules":{nested}}}}}"#);
    expect_code(&wrap(json.as_bytes()), "blueprint.compression_ratio");
    expect_code_without_ratio(&wrap(json.as_bytes()), "blueprint.invalid_json");
}

#[test]
fn rejects_deeply_nested_books() {
    let mut node =
        serde_json::json!({"blueprint": {"item": "blueprint", "version": 562954249895936u64}});
    for _ in 0..20 {
        let mut entry = node.as_object().unwrap().clone();
        entry.insert("index".into(), 0.into());
        node = serde_json::json!({"blueprint_book": {
            "item": "blueprint-book",
            "blueprints": [entry],
            "version": 562954249895936u64
        }});
    }
    expect_code(
        &wrap(node.to_string().as_bytes()),
        "blueprint.book_too_deep",
    );
}

#[test]
fn rejects_too_many_entities() {
    let limits = Limits::default();
    let mut entities = Vec::with_capacity(limits.max_entities + 1);
    for number in 1..=(limits.max_entities as u64 + 1) {
        entities.push(entity_json(number, "0.5", "0.5"));
    }
    let json = blueprint_json(&entities.join(","), "");
    expect_code(&wrap(json.as_bytes()), "blueprint.limit_exceeded");
}

#[test]
fn rejects_far_apart_entities() {
    let entities = format!(
        "{},{}",
        entity_json(1, "-99999.5", "-99999.5"),
        entity_json(2, "99999.5", "99999.5")
    );
    expect_code(
        &wrap(blueprint_json(&entities, "").as_bytes()),
        "blueprint.area_too_large",
    );

    let out_of_range = entity_json(1, "1000000000", "0.5");
    expect_code(
        &wrap(blueprint_json(&out_of_range, "").as_bytes()),
        "blueprint.invalid_position",
    );
}

#[test]
fn rejects_non_finite_numbers() {
    let entity = entity_json(1, "1e309", "0.5");
    expect_code(
        &wrap(blueprint_json(&entity, "").as_bytes()),
        "blueprint.invalid_json",
    );
}

#[test]
fn rejects_bad_entity_numbers_and_wires() {
    let duplicate = format!(
        "{},{}",
        entity_json(1, "0.5", "0.5"),
        entity_json(1, "1.5", "0.5")
    );
    expect_code(
        &wrap(blueprint_json(&duplicate, "").as_bytes()),
        "blueprint.duplicate_entity_number",
    );

    expect_code(
        &wrap(blueprint_json(&entity_json(0, "0.5", "0.5"), "").as_bytes()),
        "blueprint.invalid_entity_number",
    );

    let wire = blueprint_json(&entity_json(1, "0.5", "0.5"), r#","wires":[[1,5,99,5]]"#);
    expect_code(&wrap(wire.as_bytes()), "blueprint.dangling_wire");

    let direction =
        r#"{"entity_number":1,"name":"inserter","position":{"x":0.5,"y":0.5},"direction":16}"#;
    expect_code(
        &wrap(blueprint_json(direction, "").as_bytes()),
        "blueprint.invalid_direction",
    );
}

#[test]
fn rejects_huge_texts() {
    let label = format!(r#","label":"{}""#, "x".repeat(10 * 1024 * 1024));
    let json = blueprint_json(&entity_json(1, "0.5", "0.5"), &label);
    let text = wrap(json.as_bytes());
    expect_code(&text, "blueprint.compression_ratio");
    expect_code_without_ratio(&text, "blueprint.text_too_long");
}

#[test]
fn rejects_invalid_utf8() {
    let mut json = blueprint_json(&entity_json(1, "0.5", "0.5"), r#","label":"AB""#).into_bytes();
    let position = json.iter().position(|b| *b == b'A').unwrap();
    json[position] = 0xFF;
    expect_code(&wrap(&json), "blueprint.invalid_utf8");
}

#[test]
fn rejects_other_objects() {
    expect_code(
        &wrap(br#"{"upgrade_planner":{}}"#),
        "blueprint.not_a_blueprint",
    );
    expect_code(&wrap(br#"[1,2,3]"#), "blueprint.invalid_json");
    expect_code(&wrap(br#"{}"#), "blueprint.not_a_blueprint");
}

#[test]
fn rejects_a_million_entities() {
    let mut entities = Vec::with_capacity(1_000_000);
    for number in 1..=1_000_000u64 {
        entities.push(entity_json(number, "0.5", "0.5"));
    }
    let json = blueprint_json(&entities.join(","), "");
    let text = wrap(json.as_bytes());
    let error = decode(&text, &Limits::default()).unwrap_err();
    assert!(
        matches!(
            error.code(),
            "blueprint.decompressed_too_large"
                | "blueprint.compression_ratio"
                | "blueprint.limit_exceeded"
        ),
        "{error}"
    );
}

#[test]
fn rejects_a_thousand_nested_books() {
    // PT: Montado como texto: um `serde_json::Value` com mil níveis estouraria a pilha só ao ser destruído.
    // EN: Built as text: a thousand-level `serde_json::Value` would overflow the stack just being dropped.
    let mut json = String::from(r#"{"blueprint":{"item":"blueprint","version":562954249895936}}"#);
    for _ in 0..1_000 {
        let inner = &json[1..json.len() - 1];
        json = format!(
            r#"{{"blueprint_book":{{"item":"blueprint-book","blueprints":[{{"index":0,{inner}}}]}}}}"#
        );
    }
    let text = wrap(json.as_bytes());
    let error = decode(&text, &Limits::default()).unwrap_err();
    assert!(
        matches!(
            error.code(),
            "blueprint.invalid_json" | "blueprint.compression_ratio"
        ),
        "{error}"
    );
    let limits = Limits {
        max_compression_ratio: usize::MAX,
        ..Limits::default()
    };
    assert_eq!(
        decode(&text, &limits).unwrap_err().code(),
        "blueprint.invalid_json"
    );
}

#[test]
fn error_messages_never_echo_user_text() {
    let evil = r#"{"entity_number":"<img src=x onerror=alert(1)> bf-limit:7","name":"x","position":{"x":0,"y":0}}"#;
    let error = decode(
        &wrap(blueprint_json(evil, "").as_bytes()),
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "blueprint.invalid_json");
    let message = error.to_string();
    assert!(!message.contains("img"), "{message}");
    assert!(!message.contains("bf-limit"), "{message}");
}

#[test]
fn rejects_invalid_wire_connectors_and_long_names() {
    let entities = format!(
        "{},{}",
        entity_json(1, "0.5", "0.5"),
        entity_json(2, "1.5", "0.5")
    );
    let wire = blueprint_json(&entities, r#","wires":[[1,9,2,5]]"#);
    expect_code(&wrap(wire.as_bytes()), "blueprint.invalid_wire");

    let long_recipe = format!(
        r#"{{"entity_number":1,"name":"assembling-machine-1","position":{{"x":1.5,"y":1.5}},"recipe":"{}"}}"#,
        "r".repeat(200)
    );
    expect_code(
        &wrap(blueprint_json(&long_recipe, "").as_bytes()),
        "blueprint.text_too_long",
    );
}

#[test]
fn rejects_too_many_unknown_fields() {
    let mut fields = String::new();
    for index in 0..40 {
        fields.push_str(&format!(r#","f{index}":1"#));
    }
    let entity = format!(
        r#"{{"entity_number":1,"name":"wooden-chest","position":{{"x":0.5,"y":0.5}}{fields}}}"#
    );
    expect_code(
        &wrap(blueprint_json(&entity, "").as_bytes()),
        "blueprint.limit_exceeded",
    );
}

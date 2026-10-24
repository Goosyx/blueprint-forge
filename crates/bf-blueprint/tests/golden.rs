//! PT: Blueprints de referência exportadas do jogo real (`validation/golden/*.txt`). Cada uma
//!     precisa ser lida sem aviso e voltar, campo por campo, exatamente como o jogo gravou.
//! EN: Reference blueprints exported from the real game (`validation/golden/*.txt`). Each one
//!     must be read without warnings and come back, field by field, exactly as the game wrote it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use base64::Engine;
use bf_blueprint::{Limits, decode, encode};
use serde_json::Value;

fn golden_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../validation/golden");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("validation/golden")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    files.sort();
    files
}

// PT: Lê o JSON cru da string, sem passar pelo modelo, para comparar com o que o modelo grava.
// EN: Reads the raw JSON from the string, bypassing the model, to compare with what the model writes.
fn raw_json(text: &str) -> Value {
    let body = text.trim().strip_prefix('0').expect("versão / version");
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(body)
        .unwrap();
    let mut json = String::new();
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .read_to_string(&mut json)
        .unwrap();
    serde_json::from_str(&json).unwrap()
}

// PT: Lista os caminhos JSON em que `a` e `b` diferem, para a falha dizer onde está o problema.
// EN: Lists the JSON paths where `a` and `b` differ, so the failure says where the problem is.
fn differences(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys().filter(|key| !x.contains_key(*key))) {
                match (x.get(key), y.get(key)) {
                    (Some(left), Some(right)) => {
                        differences(left, right, &format!("{path}/{key}"), out);
                    }
                    (left, right) => out.push(format!("{path}/{key}: {left:?} → {right:?}")),
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (index, (left, right)) in x.iter().zip(y).enumerate() {
                differences(left, right, &format!("{path}/{index}"), out);
            }
        }
        (Value::Number(x), Value::Number(y)) if x.as_f64() == y.as_f64() => {}
        _ if a == b => {}
        _ => out.push(format!("{path}: {a} → {b}")),
    }
}

#[test]
fn golden_blueprints_round_trip_exactly() {
    let files = golden_files();
    assert!(
        !files.is_empty(),
        "nenhuma blueprint de referência / no reference blueprint"
    );
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let text = fs::read_to_string(file).unwrap();

        let decoded = decode(&text, &Limits::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            decoded.warnings.is_empty(),
            "{name}: {:?}",
            decoded.warnings
        );

        let encoded = encode(&decoded.content).unwrap();
        let again = decode(&encoded, &Limits::default()).unwrap();
        assert_eq!(again.content, decoded.content, "{name}: modelo / model");

        let mut diffs = Vec::new();
        differences(&raw_json(&text), &raw_json(&encoded), "", &mut diffs);
        assert!(diffs.is_empty(), "{name}:\n{}", diffs.join("\n"));
    }
    println!(
        "{} blueprints de referência / reference blueprints",
        files.len()
    );
}

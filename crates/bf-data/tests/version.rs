//! PT: A versão do jogo no Rust e nos scripts precisa ser a mesma.
//! EN: The game version in Rust and in the scripts must be the same.

#![allow(clippy::unwrap_used)]

#[test]
fn scripts_use_the_same_game_version() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/factorio-version.sh");
    let text = std::fs::read_to_string(path).unwrap();
    let expected = format!("FACTORIO_VERSION=\"{}\"", bf_data::GAME_VERSION);
    assert!(text.lines().any(|line| line == expected), "{expected}");
}

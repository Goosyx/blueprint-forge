//! PT: Fuzzing da análise completa (codec, mundo, cálculo e programação linear) com os dados
//!     reais do jogo quando o pacote existe (`BF_GAME_ASSETS`), ou dados vazios.
//! EN: Fuzzing of the whole analysis (codec, world, calculation and linear programming) with the
//!     real game data when the package exists (`BF_GAME_ASSETS`), or empty data.
#![no_main]

use std::sync::OnceLock;

use bf_analysis::analyze;
use bf_blueprint::{Limits, encode_raw_json};
use bf_calc::AnalysisOptions;
use bf_data::GameData;
use libfuzzer_sys::fuzz_target;

fn data() -> &'static GameData {
    static DATA: OnceLock<GameData> = OnceLock::new();
    DATA.get_or_init(|| {
        let path = std::env::var("BF_GAME_ASSETS")
            .map(|dir| format!("{dir}/gamedata.json"))
            .unwrap_or_else(|_| "../game-assets/2.1.23/gamedata.json".to_owned());
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| GameData::from_json(&text).ok())
            .unwrap_or_else(|| {
                GameData::from_json(
                    r#"{"format_version":1,"game_version":"fuzz","items":{},"fluids":{},"recipes":{},"entities":{},"tiles":{}}"#,
                )
                .unwrap_or_else(|_| std::process::abort())
            })
    })
}

fuzz_target!(|input: &[u8]| {
    let Ok(json) = std::str::from_utf8(input) else { return };
    let Ok(text) = encode_raw_json(json) else { return };
    let options = AnalysisOptions::default();
    let selected = input.first().map_or(0, |b| usize::from(*b % 4));
    let _ = analyze(data(), &text, selected, &options, &Limits::default());
});

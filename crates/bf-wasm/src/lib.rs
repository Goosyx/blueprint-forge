//! PT: Fachada `wasm-bindgen` para o navegador. As fronteiras usam JSON em texto: entra a
//!     blueprint string, sai o resultado serializado. Erros viram um JSON `{code, message}`.
//! EN: `wasm-bindgen` facade for the browser. Boundaries use JSON text: the blueprint string
//!     goes in, the serialized result comes out. Errors become a `{code, message}` JSON.

use bf_analysis::{AnalysisError, analyze, simulate_ticks};
use bf_blueprint::Limits;
use bf_calc::AnalysisOptions;
use bf_data::GameData;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
}

// PT: Converte um erro em `JsError` cuja mensagem é o JSON `{code, message}`.
// EN: Converts an error into a `JsError` whose message is the `{code, message}` JSON.
fn js_error(code: &str, message: String) -> JsError {
    let body = serde_json::to_string(&ErrorBody { code, message })
        .unwrap_or_else(|_| format!("{{\"code\":\"{code}\"}}"));
    JsError::new(&body)
}

fn analysis_error(error: &AnalysisError) -> JsError {
    js_error(error.code(), error.to_string())
}

/// PT: Motor de análise com os dados do jogo já carregados.
/// EN: Analysis engine with the game data already loaded.
#[wasm_bindgen]
pub struct Engine {
    data: GameData,
    limits: Limits,
}

#[wasm_bindgen]
impl Engine {
    /// PT: Cria o motor a partir do conteúdo do `gamedata.json`.
    /// EN: Creates the engine from the contents of `gamedata.json`.
    #[wasm_bindgen(constructor)]
    pub fn new(game_data_json: &str) -> Result<Engine, JsError> {
        console_error_panic_hook::set_once();
        let data = GameData::from_json(game_data_json)
            .map_err(|error| js_error("game_data.invalid", error.to_string()))?;
        Ok(Engine {
            data,
            limits: Limits::default(),
        })
    }

    /// PT: Versão do jogo dos dados carregados.
    /// EN: Game version of the loaded data.
    #[wasm_bindgen(getter, js_name = gameVersion)]
    pub fn game_version(&self) -> String {
        self.data.game_version.clone()
    }

    /// PT: Analisa a blueprint string: `selected` é a posição da blueprint no livro (0 para uma
    ///     blueprint avulsa) e `options_json` as opções da análise em JSON. Devolve o resultado em JSON.
    /// EN: Analyzes the blueprint string: `selected` is the blueprint's position in the book (0 for
    ///     a standalone blueprint) and `options_json` the analysis options as JSON. Returns JSON.
    pub fn analyze(
        &self,
        text: &str,
        selected: u32,
        options_json: &str,
    ) -> Result<String, JsError> {
        let options = parse_options(options_json)?;
        let selected = usize::try_from(selected).unwrap_or(usize::MAX);
        let analysis = analyze(&self.data, text, selected, &options, &self.limits)
            .map_err(|e| analysis_error(&e))?;
        serde_json::to_string(&analysis).map_err(|error| js_error("internal", error.to_string()))
    }

    /// PT: Simulação tick a tick da blueprint, com `warmup_ticks` de aquecimento e
    ///     `measure_ticks` de medição (com teto). Devolve o relatório de fluxo em JSON. Para
    ///     cancelar, a interface encerra o worker.
    /// EN: Tick-by-tick simulation of the blueprint, with `warmup_ticks` of warm-up and
    ///     `measure_ticks` of measurement (capped). Returns the flow report as JSON. To cancel, the
    ///     interface terminates the worker.
    pub fn simulate(
        &self,
        text: &str,
        selected: u32,
        options_json: &str,
        warmup_ticks: u32,
        measure_ticks: u32,
    ) -> Result<String, JsError> {
        let options = parse_options(options_json)?;
        let ticks = bf_sim::tick::TickOptions {
            warmup_ticks: warmup_ticks.min(MAX_WARMUP_TICKS),
            measure_ticks: measure_ticks.clamp(60, MAX_MEASURE_TICKS),
            ..bf_sim::tick::TickOptions::default()
        };
        let selected = usize::try_from(selected).unwrap_or(usize::MAX);
        let flow = simulate_ticks(
            &self.data,
            text,
            selected,
            &options,
            &self.limits,
            &ticks,
            &mut |_| true,
        )
        .map_err(|e| analysis_error(&e))?;
        serde_json::to_string(&flow).map_err(|error| js_error("internal", error.to_string()))
    }
}

// PT: Lê as opções em JSON (vazio = padrão), com teto de tamanho.
// EN: Reads the JSON options (empty = default), with a size cap.
fn parse_options(options_json: &str) -> Result<AnalysisOptions, JsError> {
    if options_json.len() > MAX_OPTIONS_BYTES {
        return Err(js_error("analysis.options_too_large", String::new()));
    }
    if options_json.trim().is_empty() {
        return Ok(AnalysisOptions::default());
    }
    serde_json::from_str(options_json)
        .map_err(|_| js_error("analysis.options_invalid", String::new()))
}

/// PT: Tetos da simulação por tick: 1 h de aquecimento e 10 min de medição, em ticks.
/// EN: Tick simulation caps: 1 h of warm-up and 10 min of measurement, in ticks.
const MAX_WARMUP_TICKS: u32 = 216_000;
const MAX_MEASURE_TICKS: u32 = 36_000;

/// PT: Tamanho máximo do JSON de opções. EN: Maximum size of the options JSON.
const MAX_OPTIONS_BYTES: usize = 1024 * 1024;

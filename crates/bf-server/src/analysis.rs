//! PT: Análise no servidor: o resultado do navegador nunca é confiado. Cada análise roda numa
//!     thread de bloqueio, com no máximo uma por CPU no servidor e `ANALYSES_PER_USER` por usuário,
//!     espera limitada por vaga e tempo limite.
//! EN: Server-side analysis: the browser's result is never trusted. Each analysis runs on a
//!     blocking thread, with at most one per CPU on the server and `ANALYSES_PER_USER` per user,
//!     a bounded wait for a slot and a timeout.

use std::time::Duration;

use bf_analysis::{Analysis, TreeNode, analyze};
use bf_blueprint::Limits;
use bf_calc::AnalysisOptions;
use serde::Serialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, UserSlot};

const SLOT_WAIT: Duration = Duration::from_secs(10);
const ANALYSIS_TIMEOUT: Duration = Duration::from_secs(20);
/// PT: Versão do analisador gravada junto com cada resumo. EN: Analyzer version stored with each summary.
pub const ANALYZER_VERSION: &str = concat!("bf-", env!("CARGO_PKG_VERSION"));

/// PT: Resumo guardado no banco (os detalhes são recalculados sob demanda).
/// EN: Summary stored in the database (details are recalculated on demand).
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub is_book: bool,
    pub blueprint_count: usize,
    pub entity_count: usize,
    pub label: Option<String>,
    pub selected: usize,
    pub selected_entities: usize,
    pub outputs: Vec<(String, f64)>,
    pub power_average_w: f64,
    pub power_peak_w: f64,
    pub pollution_per_minute: f64,
    pub warnings: usize,
}

impl Summary {
    fn from(analysis: &Analysis) -> Self {
        let mut outputs: Vec<(String, f64)> = analysis
            .report
            .steady
            .iter()
            .map(|flow| {
                (
                    flow.name.clone(),
                    flow.produced_per_second - flow.consumed_per_second,
                )
            })
            .filter(|(_, net)| *net > 1e-9)
            .collect();
        outputs.sort_by(|a, b| b.1.total_cmp(&a.1));
        outputs.truncate(10);
        let label = match &analysis.tree {
            TreeNode::Book { label, .. } => label.clone(),
            _ => analysis.blueprints.first().and_then(|b| b.label.clone()),
        };
        Self {
            is_book: matches!(analysis.tree, TreeNode::Book { .. }),
            blueprint_count: analysis.blueprints.len(),
            entity_count: analysis.blueprints.iter().map(|b| b.entity_count).sum(),
            label: label.map(|l| l.chars().take(200).collect()),
            selected: analysis.selected,
            selected_entities: analysis
                .blueprints
                .get(analysis.selected)
                .map_or(0, |b| b.entity_count),
            outputs,
            power_average_w: analysis.report.power.average_w,
            power_peak_w: analysis.report.power.peak_w,
            pollution_per_minute: analysis.report.pollution_per_minute,
            warnings: analysis.warnings.len()
                + analysis.world.warnings.len()
                + analysis.report.warnings.len(),
        }
    }
}

/// PT: Resultado da análise no servidor: a análise completa e o resumo para o banco.
/// EN: Server analysis result: the complete analysis and the summary for the database.
pub struct ServerAnalysis {
    pub analysis: Analysis,
    pub summary: Summary,
    pub data_version: String,
}

/// PT: Roda a análise respeitando as vagas e o tempo limite.
/// EN: Runs the analysis respecting the slots and the timeout.
pub async fn run(
    state: &AppState,
    user: Uuid,
    source: String,
    selected: usize,
    options: AnalysisOptions,
) -> ApiResult<ServerAnalysis> {
    // PT: A vaga do usuário e a global vão para dentro da thread: só voltam quando ela termina,
    //     mesmo que a resposta já tenha expirado ou o cliente tenha desconectado.
    // EN: The user slot and the global one go inside the thread: they only return when it finishes,
    //     even if the response already timed out or the client disconnected.
    let user_slot = UserSlot::acquire(state, user)?;
    let permit = tokio::time::timeout(SLOT_WAIT, state.analysis_slots.clone().acquire_owned())
        .await
        .map_err(|_| ApiError::Unavailable("analysis.busy"))?
        .map_err(|_| ApiError::Internal)?;
    let (data_version, data) = state
        .store
        .latest_data()
        .ok_or(ApiError::Unavailable("game_data.missing"))?;

    // PT: A vaga só é liberada quando a thread termina, mesmo que a resposta já tenha expirado.
    // EN: The slot is only released when the thread finishes, even if the response already timed out.
    let task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _user_slot = user_slot;
        analyze(&data, &source, selected, &options, &Limits::default())
    });
    let analysis = tokio::time::timeout(ANALYSIS_TIMEOUT, task)
        .await
        .map_err(|_| ApiError::Unavailable("analysis.timeout"))?
        .map_err(|_| ApiError::Internal)?
        .map_err(|error| ApiError::BadRequest(error.code()))?;
    let summary = Summary::from(&analysis);
    Ok(ServerAnalysis {
        analysis,
        summary,
        data_version,
    })
}

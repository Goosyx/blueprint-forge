//! PT: Cálculo numérico de uma blueprint: efeitos de módulos e beacons, produção e consumo,
//!     regime estável por programação linear, energia, geração, poluição e lista de materiais.
//! EN: Numerical calculation of a blueprint: module and beacon effects, production and
//!     consumption, steady state through linear programming, power, generation, pollution and
//!     bill of materials.

pub mod bom;
pub mod budget;
pub mod effects;
pub mod flows;
pub mod generation;
pub mod lp;
pub mod machines;
pub mod power;

use std::collections::{BTreeMap, BTreeSet};

use bf_blueprint::Blueprint;
use bf_data::GameData;
use bf_world::World;
use serde::{Deserialize, Serialize};

pub use bom::BomLine;
pub use budget::{Budget, CalcError};
pub use flows::{ItemFlow, RecipeGroup};
pub use generation::GenerationReport;
pub use lp::{SteadyOptions, SteadyState};
pub use machines::{MachineReport, RecipeSource};
pub use power::PowerReport;

/// PT: Teto de receitas trocadas manualmente por análise.
/// EN: Cap on manually overridden recipes per analysis.
pub const MAX_RECIPE_OVERRIDES: usize = 10_000;

/// PT: Tamanho máximo das opções serializadas. EN: Maximum size of the serialized options.
pub const MAX_OPTIONS_BYTES: usize = 16 * 1024;

/// PT: Opções da análise escolhidas pelo usuário.
/// EN: Analysis options chosen by the user.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalysisOptions {
    /// PT: Número da entidade → receita escolhida (fornalhas, até a inferência da Fase 3).
    /// EN: Entity number → chosen recipe (furnaces, until the Phase 3 inference).
    pub recipe_overrides: BTreeMap<u64, String>,
    /// PT: Item → máximo de entrada externa por segundo. EN: Item → maximum external input per second.
    pub input_limits: BTreeMap<String, f64>,
    /// PT: Produtos finais sem saída externa. EN: Final products without an external outlet.
    pub blocked_outputs: BTreeSet<String>,
    /// PT: Intermediários que também saem da blueprint (por padrão a sobra entope e para a máquina).
    /// EN: Intermediates that also leave the blueprint (by default surplus clogs and stops the machine).
    pub exported: BTreeSet<String>,
    /// PT: Combustível das máquinas a queima (padrão: carvão). EN: Burner machine fuel (default: coal).
    pub fuel: Option<String>,
}

impl AnalysisOptions {
    /// PT: Confere tamanhos e valores antes de usar (vale para opções vindas da API).
    /// EN: Checks sizes and values before use (applies to options coming from the API).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.recipe_overrides.len() > MAX_RECIPE_OVERRIDES
            || self.input_limits.len() > 1_000
            || self.blocked_outputs.len() > 1_000
            || self.exported.len() > 1_000
        {
            return Err("analysis.options_too_large");
        }
        let long = |s: &str| s.len() > 100;
        if self.recipe_overrides.values().any(|s| long(s))
            || self.input_limits.keys().any(|s| long(s))
            || self.blocked_outputs.iter().any(|s| long(s))
            || self.exported.iter().any(|s| long(s))
            || self.fuel.as_deref().is_some_and(long)
        {
            return Err("analysis.options_invalid");
        }
        if self
            .input_limits
            .values()
            .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err("analysis.options_invalid");
        }
        // PT: Teto do tamanho serializado, conferido antes de gastar CPU (e abaixo do CHECK do banco).
        // EN: Cap on the serialized size, checked before spending CPU (and below the database CHECK).
        if serde_json::to_vec(self).map_or(true, |bytes| bytes.len() > MAX_OPTIONS_BYTES) {
            return Err("analysis.options_too_large");
        }
        Ok(())
    }
}

/// PT: Aviso do cálculo.
/// EN: Calculation warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum CalcWarning {
    /// PT: Máquina sem receita (fornalha sem receita escolhida, por exemplo).
    /// EN: Machine without a recipe (a furnace without a chosen recipe, for example).
    NoRecipe {
        entity_number: u64,
    },
    UnknownRecipe {
        entity_number: u64,
        recipe: String,
    },
    /// PT: A máquina não faz essa categoria de receita.
    /// EN: The machine does not make that recipe category.
    RecipeNotAllowed {
        entity_number: u64,
        recipe: String,
    },
}

/// PT: Relatório completo do cálculo de uma blueprint.
/// EN: Complete calculation report for one blueprint.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalcReport {
    pub machines: Vec<MachineReport>,
    pub groups: Vec<RecipeGroup>,
    /// PT: Balanço com todas as máquinas a 100%. EN: Balance with every machine at 100%.
    pub nominal: Vec<ItemFlow>,
    /// PT: Balanço no regime estável. EN: Balance in steady state.
    pub steady: Vec<ItemFlow>,
    pub steady_state: SteadyState,
    pub power: PowerReport,
    pub generation: GenerationReport,
    pub pollution_per_minute: f64,
    pub bill_of_materials: Vec<BomLine>,
    pub warnings: Vec<CalcWarning>,
}

/// PT: Calcula a blueprint. `world` precisa ter sido montado a partir de `blueprint`. Para com
///     `CalcError::BudgetExceeded` se o trabalho passar do orçamento padrão.
/// EN: Calculates the blueprint. `world` must have been built from `blueprint`. Stops with
///     `CalcError::BudgetExceeded` if the work exceeds the default budget.
pub fn calculate(
    data: &GameData,
    blueprint: &Blueprint,
    world: &World,
    options: &AnalysisOptions,
) -> Result<CalcReport, CalcError> {
    let mut budget = Budget::default();
    let fuel = options.fuel.as_deref().unwrap_or("coal");
    let mut warnings = Vec::new();
    let machines = machines::machines(data, blueprint, world, options, &mut warnings, &mut budget)?;
    let groups = flows::recipe_groups(data, &machines, fuel);

    let steady_state = lp::steady_state(
        &groups,
        &SteadyOptions {
            input_limits: options.input_limits.clone(),
            blocked_outputs: options.blocked_outputs.clone(),
            exported: options.exported.clone(),
        },
    );
    let full: BTreeMap<String, f64> = groups.iter().map(|g| (g.recipe.clone(), 1.0)).collect();
    let nominal = flows::balance(data, &groups, &full);
    let steady = flows::balance(data, &groups, &steady_state.utilization);

    let power = power::power(data, world, &machines, &steady_state.utilization);
    let generation = generation::generation(data, world, fuel, &mut budget)?;
    let crafting_pollution: f64 = groups
        .iter()
        .map(|g| {
            g.pollution_per_minute
                * steady_state
                    .utilization
                    .get(&g.recipe)
                    .copied()
                    .unwrap_or(1.0)
        })
        .sum();

    Ok(CalcReport {
        machines: machines.into_iter().map(|m| m.report).collect(),
        groups,
        nominal,
        steady,
        steady_state,
        power,
        pollution_per_minute: crafting_pollution
            + generation.pollution_per_minute
            + other_pollution(data, world),
        generation,
        bill_of_materials: bom::bill_of_materials(data, blueprint),
        warnings,
    })
}

// PT: Poluição das demais entidades que emitem (mineradoras, por exemplo), a 100%. Máquinas de
//     produção e caldeiras já foram contadas nos seus próprios cálculos.
// EN: Pollution of the other emitting entities (mining drills, for example), at 100%. Crafting
//     machines and boilers were already counted in their own calculations.
fn other_pollution(data: &GameData, world: &World) -> f64 {
    world
        .entities
        .iter()
        .filter_map(|placed| data.entities.get(&placed.name))
        .filter(|entity| {
            !matches!(
                entity.kind,
                bf_data::EntityKind::CraftingMachine { .. } | bf_data::EntityKind::Boiler { .. }
            )
        })
        .filter_map(|entity| entity.energy_source.as_ref())
        .filter_map(|source| source.emissions_per_minute.get("pollution"))
        .sum()
}

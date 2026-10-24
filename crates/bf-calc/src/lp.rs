//! PT: Regime estável por programação linear. Cada grupo de receita tem um uso `u ∈ [0, 1]`.
//!     Restrições: um item intermediário (produzido e consumido dentro da blueprint) não pode faltar
//!     nem sobrar, porque a sobra entope a saída e a máquina para; o usuário pode marcar
//!     intermediários como exportados. Produtos finais saem livres, a não ser que estejam marcados
//!     como "sem saída" (modela a refinaria parando com óleo cheio). Entradas externas respeitam os
//!     limites do usuário. Objetivo: maximizar o uso total.
//! EN: Steady state through linear programming. Each recipe group has a use `u ∈ [0, 1]`.
//!     Constraints: an intermediate item (produced and consumed inside the blueprint) can neither
//!     run short nor pile up, because surplus clogs the output and the machine stops; the user may
//!     mark intermediates as exported. Final products leave freely, unless marked as "no outlet"
//!     (models a refinery stopping when oil fills up). External inputs respect the user's limits.
//!     Objective: maximize total use.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use microlp::{ComparisonOp, OptimizationDirection, Problem};
use serde::Serialize;

use crate::flows::RecipeGroup;

/// PT: Tempo máximo do solver. EN: Maximum solver time.
const SOLVER_TIME_LIMIT: Duration = Duration::from_secs(2);
/// PT: Folga numérica nas restrições. EN: Numerical slack in the constraints.
const EPSILON: f64 = 1e-9;

/// PT: Resultado do regime estável.
/// EN: Steady-state result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SteadyState {
    /// PT: Receita → uso de 0 a 1. EN: Recipe → use from 0 to 1.
    pub utilization: BTreeMap<String, f64>,
    /// PT: Itens que limitam alguma receita (consumidos por inteiro). EN: Items limiting some recipe (fully consumed).
    pub bottlenecks: Vec<String>,
    /// PT: Se o solver não convergiu, o regime estável cai para 100% e isto fica `false`.
    /// EN: If the solver did not converge, the steady state falls back to 100% and this is `false`.
    pub solved: bool,
}

/// PT: Restrições escolhidas pelo usuário.
/// EN: Constraints chosen by the user.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SteadyOptions {
    /// PT: Item → máximo que pode entrar de fora por segundo. EN: Item → maximum that can come from outside per second.
    pub input_limits: BTreeMap<String, f64>,
    /// PT: Produtos finais sem saída (não podem sobrar). EN: Final products without an outlet (cannot pile up).
    pub blocked_outputs: BTreeSet<String>,
    /// PT: Intermediários que também saem da blueprint. EN: Intermediates that also leave the blueprint.
    pub exported: BTreeSet<String>,
}

/// PT: Resolve o regime estável dos grupos.
/// EN: Solves the steady state of the groups.
pub fn steady_state(groups: &[RecipeGroup], options: &SteadyOptions) -> SteadyState {
    let full: BTreeMap<String, f64> = groups.iter().map(|g| (g.recipe.clone(), 1.0)).collect();
    if groups.is_empty() {
        return SteadyState {
            utilization: full,
            bottlenecks: Vec::new(),
            solved: true,
        };
    }

    let mut problem = Problem::new(OptimizationDirection::Maximize);
    problem.set_time_limit(SOLVER_TIME_LIMIT);
    let vars: Vec<_> = groups
        .iter()
        .map(|_| problem.add_var(1.0, (0.0, 1.0)))
        .collect();

    // PT: Coeficiente líquido (produção − consumo) de cada item em cada grupo, já somado: uma
    //     receita que consome e produz o mesmo item (kovarex) vira um só termo com o saldo, e o
    //     item conta como produzido ou consumido pelo sinal desse saldo.
    // EN: Net coefficient (production − consumption) of each item in each group, already summed:
    //     a recipe that consumes and produces the same item (kovarex) becomes a single term with
    //     the balance, and the item counts as produced or consumed by that balance's sign.
    let mut net: BTreeMap<&str, BTreeMap<usize, f64>> = BTreeMap::new();
    for (index, group) in groups.iter().enumerate() {
        for (name, rate) in &group.produces {
            *net.entry(name).or_default().entry(index).or_insert(0.0) += *rate;
        }
        for (name, rate) in &group.consumes {
            *net.entry(name).or_default().entry(index).or_insert(0.0) -= *rate;
        }
    }
    let net: BTreeMap<&str, Vec<(usize, f64)>> = net
        .into_iter()
        .map(|(name, terms)| {
            let terms: Vec<(usize, f64)> =
                terms.into_iter().filter(|(_, c)| c.abs() > 1e-12).collect();
            (name, terms)
        })
        .collect();
    let produced: BTreeSet<&str> = net
        .iter()
        .filter(|(_, terms)| terms.iter().any(|(_, c)| *c > 0.0))
        .map(|(name, _)| *name)
        .collect();
    let consumed: BTreeSet<&str> = net
        .iter()
        .filter(|(_, terms)| terms.iter().any(|(_, c)| *c < 0.0))
        .map(|(name, _)| *name)
        .collect();

    let mut constrained = Vec::new();
    for (name, terms) in &net {
        let expression: Vec<_> = terms
            .iter()
            .filter_map(|(index, coefficient)| vars.get(*index).map(|var| (*var, *coefficient)))
            .collect();
        let is_produced = produced.contains(name);
        let is_consumed = consumed.contains(name);
        if is_produced && is_consumed {
            problem.add_constraint(expression.as_slice(), ComparisonOp::Ge, -EPSILON);
            if !options.exported.contains(*name) {
                problem.add_constraint(expression.as_slice(), ComparisonOp::Le, EPSILON);
            }
            constrained.push(*name);
        } else if is_produced && options.blocked_outputs.contains(*name) {
            problem.add_constraint(expression.as_slice(), ComparisonOp::Le, EPSILON);
        }
        if !is_produced
            && is_consumed
            && let Some(limit) = options.input_limits.get(*name)
        {
            let consumption: Vec<_> = expression.iter().map(|(var, c)| (*var, -c)).collect();
            problem.add_constraint(consumption.as_slice(), ComparisonOp::Le, limit.max(0.0));
            constrained.push(*name);
        }
    }

    // PT: Última defesa: um pânico dentro do solver vira "não resolvido" (no nativo; no WASM o
    //     pânico aborta e o worker é recriado).
    // EN: Last line of defense: a panic inside the solver becomes "not solved" (natively; in WASM
    //     the panic aborts and the worker is recreated).
    let solved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| problem.solve()));
    let Ok(Ok(solution)) = solved.map(|result| result.map(microlp::SolveOutcome::into_solution))
    else {
        return SteadyState {
            utilization: full,
            bottlenecks: Vec::new(),
            solved: false,
        };
    };
    let Ok(solution) = solution else {
        return SteadyState {
            utilization: full,
            bottlenecks: Vec::new(),
            solved: false,
        };
    };

    let utilization: BTreeMap<String, f64> = groups
        .iter()
        .zip(&vars)
        .map(|(group, var)| {
            (
                group.recipe.clone(),
                solution.var_value(*var).clamp(0.0, 1.0),
            )
        })
        .collect();

    // PT: Gargalo: item restrito cujo saldo ficou zerado enquanto algum consumidor não roda a 100%.
    // EN: Bottleneck: constrained item whose balance hit zero while some consumer is below 100%.
    let mut bottlenecks = Vec::new();
    for name in constrained {
        let Some(terms) = net.get(name) else { continue };
        let mut balance = 0.0;
        let mut starved_consumer = false;
        for (index, coefficient) in terms {
            let use_factor = groups
                .get(*index)
                .and_then(|g| utilization.get(&g.recipe))
                .copied()
                .unwrap_or(1.0);
            balance += coefficient * use_factor;
            if *coefficient < 0.0 && use_factor < 1.0 - 1e-6 {
                starved_consumer = true;
            }
        }
        let limit = options.input_limits.get(name).copied();
        let tight = match limit {
            Some(limit) => (-balance - limit).abs() < 1e-6,
            None => balance.abs() < 1e-6,
        };
        if tight && starved_consumer {
            bottlenecks.push(name.to_owned());
        }
    }

    SteadyState {
        utilization,
        bottlenecks,
        solved: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(recipe: &str, produces: &[(&str, f64)], consumes: &[(&str, f64)]) -> RecipeGroup {
        RecipeGroup {
            recipe: recipe.into(),
            machines: 1,
            produces: produces.iter().map(|(n, r)| ((*n).into(), *r)).collect(),
            consumes: consumes.iter().map(|(n, r)| ((*n).into(), *r)).collect(),
            power_w: 0.0,
            pollution_per_minute: 0.0,
        }
    }

    #[test]
    fn starved_consumer_slows_down() {
        // PT: Engrenagem produz 1/s; a ciência consome 2/s → a ciência roda a 50%.
        // EN: Gears make 1/s; science consumes 2/s → science runs at 50%.
        let groups = vec![
            group("gear", &[("gear", 1.0)], &[("plate", 2.0)]),
            group("science", &[("science", 0.5)], &[("gear", 2.0)]),
        ];
        let state = steady_state(&groups, &SteadyOptions::default());
        assert!(state.solved);
        assert!((state.utilization["gear"] - 1.0).abs() < 1e-6);
        assert!((state.utilization["science"] - 0.5).abs() < 1e-6);
        assert_eq!(state.bottlenecks, vec!["gear".to_owned()]);
    }

    #[test]
    fn surplus_intermediate_slows_its_producer() {
        // PT: Engrenagem faz 1/s, a ciência só usa 0,25/s: a engrenagem roda a 25%, a não ser que
        //     seja exportada.
        // EN: Gears make 1/s, science only uses 0.25/s: gears run at 25%, unless exported.
        let groups = vec![
            group("gear", &[("gear", 1.0)], &[("plate", 2.0)]),
            group("science", &[("science", 0.05)], &[("gear", 0.25)]),
        ];
        let state = steady_state(&groups, &SteadyOptions::default());
        assert!((state.utilization["gear"] - 0.25).abs() < 1e-6);
        assert!((state.utilization["science"] - 1.0).abs() < 1e-6);

        let options = SteadyOptions {
            exported: ["gear".to_owned()].into_iter().collect(),
            ..SteadyOptions::default()
        };
        let state = steady_state(&groups, &options);
        assert!((state.utilization["gear"] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn recipes_that_consume_their_own_product_do_not_panic() {
        // PT: Kovarex: consome 40 U-235 + 5 U-238 e produz 41 U-235 + 2 U-238 (saldo +1 e −3).
        // EN: Kovarex: consumes 40 U-235 + 5 U-238 and produces 41 U-235 + 2 U-238 (balance +1 and −3).
        let groups = vec![group(
            "kovarex",
            &[("uranium-235", 41.0), ("uranium-238", 2.0)],
            &[("uranium-235", 40.0), ("uranium-238", 5.0)],
        )];
        let state = steady_state(&groups, &SteadyOptions::default());
        assert!(state.solved);
        assert!((state.utilization["kovarex"] - 1.0).abs() < 1e-6);

        let liquefaction = vec![group(
            "coal-liquefaction",
            &[
                ("heavy-oil", 90.0),
                ("light-oil", 20.0),
                ("petroleum-gas", 10.0),
            ],
            &[("coal", 10.0), ("heavy-oil", 25.0), ("steam", 50.0)],
        )];
        let state = steady_state(&liquefaction, &SteadyOptions::default());
        assert!(state.solved);
        assert!((state.utilization["coal-liquefaction"] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn blocked_byproduct_stops_the_producer() {
        // PT: Refinaria faz óleo pesado sem consumidor e sem saída → para.
        // EN: Refinery makes heavy oil without a consumer or outlet → it stops.
        let groups = vec![group(
            "oil",
            &[("heavy-oil", 25.0), ("petroleum-gas", 55.0)],
            &[("crude-oil", 100.0)],
        )];
        let options = SteadyOptions {
            blocked_outputs: ["heavy-oil".to_owned()].into_iter().collect(),
            ..SteadyOptions::default()
        };
        let state = steady_state(&groups, &options);
        assert!(state.utilization["oil"] < 1e-6);
    }

    #[test]
    fn input_limit_caps_consumption() {
        let groups = vec![group("gear", &[("gear", 1.0)], &[("plate", 2.0)])];
        let options = SteadyOptions {
            input_limits: [("plate".to_owned(), 1.0)].into_iter().collect(),
            ..SteadyOptions::default()
        };
        let state = steady_state(&groups, &options);
        assert!((state.utilization["gear"] - 0.5).abs() < 1e-6);
        assert_eq!(state.bottlenecks, vec!["plate".to_owned()]);
    }
}

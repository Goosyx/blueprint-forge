//! PT: Efeitos de módulos e beacons sobre uma máquina, com as regras do 2.0: só os efeitos
//!     permitidos pela máquina contam, o beacon transmite `módulos × distribution_effectivity ×
//!     profile[n − 1]` (n = beacons que afetam a máquina) e os limites mínimos e máximos valem no fim.
//! EN: Module and beacon effects on a machine, with the 2.0 rules: only effects allowed by the
//!     machine count, a beacon transmits `modules × distribution_effectivity × profile[n − 1]`
//!     (n = beacons affecting the machine) and the minimum and maximum limits apply at the end.

use std::collections::BTreeMap;

use bf_data::{Effect, GameData};

/// PT: Piso de velocidade, consumo e poluição: −80%.
/// EN: Floor for speed, consumption and pollution: −80%.
pub const MIN_EFFECT: f64 = -0.8;

/// PT: Soma dos efeitos dos módulos (nome → quantidade), filtrando pelos efeitos permitidos.
///     Itens que não são módulos são ignorados.
/// EN: Sum of module effects (name → count), filtered by the allowed effects.
///     Items that are not modules are ignored.
pub fn modules_effect(
    data: &GameData,
    modules: &BTreeMap<String, u32>,
    allowed: &[String],
) -> Effect {
    let mut total = Effect::default();
    for (name, count) in modules {
        let Some(module) = data.items.get(name).and_then(|item| item.module.as_ref()) else {
            continue;
        };
        let count = f64::from(*count);
        add_scaled(&mut total, &module.effect, count);
    }
    filter_allowed(total, allowed)
}

/// PT: Soma `effect × factor` em `total`.
/// EN: Adds `effect × factor` into `total`.
pub fn add_scaled(total: &mut Effect, effect: &Effect, factor: f64) {
    total.speed += effect.speed * factor;
    total.productivity += effect.productivity * factor;
    total.consumption += effect.consumption * factor;
    total.pollution += effect.pollution * factor;
    total.quality += effect.quality * factor;
}

/// PT: Zera os efeitos que a lista `allowed` não inclui (`speed`, `productivity`...).
/// EN: Zeroes the effects that the `allowed` list does not include (`speed`, `productivity`...).
pub fn filter_allowed(effect: Effect, allowed: &[String]) -> Effect {
    let has = |name: &str| allowed.iter().any(|a| a == name);
    Effect {
        speed: if has("speed") { effect.speed } else { 0.0 },
        productivity: if has("productivity") {
            effect.productivity
        } else {
            0.0
        },
        consumption: if has("consumption") {
            effect.consumption
        } else {
            0.0
        },
        pollution: if has("pollution") {
            effect.pollution
        } else {
            0.0
        },
        quality: if has("quality") { effect.quality } else { 0.0 },
    }
}

/// PT: Fator do perfil do beacon para `count` beacons (o último valor vale para contagens maiores).
/// EN: Beacon profile factor for `count` beacons (the last value applies to larger counts).
pub fn profile_factor(profile: &[f64], count: usize) -> f64 {
    if count == 0 {
        return 0.0;
    }
    profile
        .get(count - 1)
        .or_else(|| profile.last())
        .copied()
        .unwrap_or(1.0)
}

/// PT: Aplica os limites do jogo: velocidade, consumo e poluição no mínimo −80%; produtividade
///     entre 0 e o máximo da receita; produtividade zerada se a receita não permite.
/// EN: Applies the game limits: speed, consumption and pollution at least −80%; productivity
///     between 0 and the recipe maximum; productivity zeroed if the recipe does not allow it.
pub fn clamp(effect: Effect, allow_productivity: bool, maximum_productivity: f64) -> Effect {
    Effect {
        speed: effect.speed.max(MIN_EFFECT),
        consumption: effect.consumption.max(MIN_EFFECT),
        pollution: effect.pollution.max(MIN_EFFECT),
        productivity: if allow_productivity {
            effect
                .productivity
                .clamp(0.0, maximum_productivity.max(0.0))
        } else {
            0.0
        },
        quality: effect.quality.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_uses_last_value_for_large_counts() {
        let profile = [1.0, 0.75, 0.6, 0.5];
        assert_eq!(profile_factor(&profile, 0), 0.0);
        assert_eq!(profile_factor(&profile, 1), 1.0);
        assert_eq!(profile_factor(&profile, 4), 0.5);
        assert_eq!(profile_factor(&profile, 12), 0.5);
        assert_eq!(profile_factor(&[], 3), 1.0);
    }

    #[test]
    fn clamps_effects() {
        let effect = Effect {
            speed: -2.0,
            productivity: 5.0,
            consumption: -1.5,
            pollution: -1.0,
            quality: -0.3,
        };
        let clamped = clamp(effect, true, 3.0);
        assert_eq!(clamped.speed, -0.8);
        assert_eq!(clamped.productivity, 3.0);
        assert_eq!(clamped.consumption, -0.8);
        assert_eq!(clamped.quality, 0.0);
        assert_eq!(clamp(effect, false, 3.0).productivity, 0.0);
    }

    #[test]
    fn filters_disallowed_effects() {
        let effect = Effect {
            speed: 0.5,
            productivity: 0.1,
            consumption: 0.7,
            pollution: 0.1,
            quality: 0.0,
        };
        let allowed = vec!["speed".to_owned(), "consumption".to_owned()];
        let filtered = filter_allowed(effect, &allowed);
        assert_eq!(filtered.speed, 0.5);
        assert_eq!(filtered.productivity, 0.0);
        assert_eq!(filtered.consumption, 0.7);
        assert_eq!(filtered.pollution, 0.0);
    }
}

//! PT: Cenário de validação (arquivo JSON) e a geração segura do `tests/current.lua` do mod.
//!     Nenhum texto livre entra no Lua: a blueprint passa pela pré-validação do codec (só
//!     alfabeto base64) e os demais campos são números ou nomes restritos a `[a-z0-9-]`.
//! EN: Validation scenario (JSON file) and safe generation of the mod's `tests/current.lua`.
//!     No free text goes into the Lua: the blueprint passes the codec precheck (base64 alphabet
//!     only) and the other fields are numbers or names restricted to `[a-z0-9-]`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use std::io::Read as _;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use bf_blueprint::{BlueprintString, Limits, decode, encode_raw_json, precheck};
use serde::Deserialize;

/// PT: Teto de ticks de um cenário (1 hora de jogo).
/// EN: Tick cap for one scenario (1 hour of game time).
pub const MAX_TICKS: u32 = 216_000;
/// PT: Teto de pontos de entrada e saída. EN: Cap on input and output points.
const MAX_POINTS: usize = 64;
/// PT: Teto de itens medidos. EN: Cap on measured items.
const MAX_MEASURED: usize = 32;

/// PT: Tipo de ponto: fonte infinita de itens ou sumidouro.
/// EN: Point kind: infinite item source or sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointKind {
    Source,
    Sink,
    #[serde(rename = "fluid_source")]
    FluidSource,
    #[serde(rename = "fluid_sink")]
    FluidSink,
}

/// PT: Papel da interface de energia: alimentar a rede ou ser uma carga (para medir geração).
/// EN: Role of the energy interface: feed the network or be a load (to measure generation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerMode {
    #[default]
    Supply,
    Load,
    /// PT: Para blueprints sem postes: um poste do mod de teste, sem colisão e com alcance de 64
    ///     tiles para cada lado, fica no centro da blueprint; os centros das entidades precisam
    ///     caber em 126 × 126 tiles (sobra 1 tile para a borda das máquinas).
    /// EN: For blueprints without poles: a test-mod pole, without collision and reaching 64 tiles
    ///     each way, sits at the blueprint's center; entity centers must fit in 126 × 126 tiles
    ///     (leaving 1 tile for the machines' edges).
    Cover,
}

/// PT: Baú infinito posicionado nas coordenadas da blueprint.
/// EN: Infinity chest placed at blueprint coordinates.
#[derive(Debug, Clone, Deserialize)]
pub struct Point {
    pub kind: PointKind,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub fluid: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
}

/// PT: Resultado esperado de um item.
/// EN: Expected result for one item.
#[derive(Debug, Clone, Deserialize)]
pub struct Expectation {
    pub produced_per_second: f64,
    /// PT: Tolerância relativa (0,01 = 1%). EN: Relative tolerance (0.01 = 1%).
    pub tolerance: f64,
}

/// PT: Pesquisas aplicadas antes do teste.
/// EN: Research applied before the test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Research {
    None,
    All,
}

/// PT: Cenário completo.
/// EN: Complete scenario.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub id: String,
    /// PT: Blueprint string pronta... EN: Ready blueprint string...
    #[serde(default)]
    pub blueprint: Option<String>,
    /// PT: ...ou o JSON da blueprint, comprimido pelo harness. EN: ...or the blueprint JSON, compressed by the harness.
    #[serde(default)]
    pub blueprint_json: Option<serde_json::Value>,
    /// PT: ...ou um arquivo com a string, relativo à pasta `validation/` (ex.: `golden/01-modulos.txt`).
    /// EN: ...or a file with the string, relative to the `validation/` folder (e.g. `golden/01-modulos.txt`).
    #[serde(default)]
    pub blueprint_file: Option<String>,
    pub research: Research,
    pub warmup_ticks: u32,
    pub measure_ticks: u32,
    pub measure: Vec<String>,
    #[serde(default)]
    pub points: Vec<Point>,
    #[serde(default)]
    pub expect: BTreeMap<String, Expectation>,
    /// PT: Compara o jogo com o cálculo do bf-calc (bônus, produção, energia e poluição).
    /// EN: Compares the game against the bf-calc calculation (bonuses, production, power and pollution).
    #[serde(default)]
    pub compare: bool,
    /// PT: Compara só os bônus de cada máquina (blueprints sem entradas, como as de referência de módulos).
    /// EN: Compares only each machine's bonuses (blueprints without inputs, like the module reference ones).
    #[serde(default)]
    pub compare_bonuses_only: bool,
    /// PT: Alimenta as esteiras marcadas por combinador constante e esvazia as pontas abertas.
    /// EN: Feeds the belts marked by a constant combinator and empties the open ends.
    #[serde(default)]
    pub auto_belts: bool,
    #[serde(default)]
    pub power: PowerMode,
    /// PT: Número da entidade → receita escolhida (fornalhas), usada no cálculo da comparação.
    /// EN: Entity number → chosen recipe (furnaces), used in the comparison calculation.
    #[serde(default)]
    pub recipe_overrides: BTreeMap<u64, String>,
    /// PT: Combustível colocado nas entidades a queima no início. EN: Fuel put into burner entities at the start.
    #[serde(default)]
    pub prefuel: Option<String>,
}

/// PT: Cenário conferido, com a blueprint string validada e a entidade âncora escolhida.
/// EN: Checked scenario, with the validated blueprint string and the chosen anchor entity.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub scenario: Scenario,
    pub blueprint: String,
    pub anchor_x: f64,
    pub anchor_y: f64,
}

/// PT: Nome restrito a letras minúsculas, dígitos e hífen (ids e nomes de item).
/// EN: Name restricted to lowercase letters, digits and hyphens (ids and item names).
pub fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// PT: Troca `blueprint_file` pelo conteúdo do arquivo. O caminho é relativo a `validation_dir`
///     (a pasta acima da dos cenários), não pode ser absoluto, subir pastas nem passar por link
///     simbólico, e a leitura para no tamanho máximo de uma blueprint colada.
/// EN: Replaces `blueprint_file` with the file's contents. The path is relative to `validation_dir`
///     (the folder above the scenarios' one), cannot be absolute, go up folders nor go through a
///     symbolic link, and reading stops at the maximum size of a pasted blueprint.
pub fn resolve_file(mut scenario: Scenario, validation_dir: &Path) -> Result<Scenario> {
    let Some(file) = scenario.blueprint_file.take() else {
        return Ok(scenario);
    };
    let relative = Path::new(&file);
    ensure!(
        scenario.blueprint.is_none() && scenario.blueprint_json.is_none(),
        "use só uma fonte de blueprint / use only one blueprint source"
    );
    ensure!(
        relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_))),
        "blueprint_file inválido / invalid blueprint_file: {file:?}"
    );
    let root = std::fs::canonicalize(validation_dir).with_context(|| {
        format!(
            "pasta ausente / missing folder: {}",
            validation_dir.display()
        )
    })?;
    // PT: Nenhuma parte do caminho pode ser link: assim `canonicalize` não leva para fora.
    // EN: No part of the path may be a link: that way `canonicalize` cannot lead outside.
    let mut current = root.clone();
    for part in relative.components() {
        current.push(part);
        let metadata = std::fs::symlink_metadata(&current)
            .with_context(|| format!("falha ao ler / failed to read {}", current.display()))?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "blueprint_file passa por link / goes through a link: {file:?}"
        );
    }
    ensure!(
        std::fs::canonicalize(&current)?.starts_with(&root) && current.is_file(),
        "blueprint_file fora da validação / outside validation: {file:?}"
    );
    let limit = Limits::default().max_encoded_len;
    let mut text = String::new();
    std::fs::File::open(&current)?
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_string(&mut text)
        .with_context(|| format!("falha ao ler / failed to read {}", current.display()))?;
    ensure!(
        text.len() <= limit,
        "blueprint_file grande demais / too large"
    );
    scenario.blueprint = Some(text);
    Ok(scenario)
}

/// PT: Confere o cenário inteiro e valida a blueprint com os limites do codec.
/// EN: Checks the whole scenario and validates the blueprint with the codec limits.
pub fn prepare(scenario: Scenario) -> Result<Prepared> {
    ensure!(
        is_safe_name(&scenario.id),
        "id inválido / invalid id: {:?}",
        scenario.id
    );
    ensure!(
        scenario.warmup_ticks <= MAX_TICKS && scenario.measure_ticks <= MAX_TICKS,
        "ticks acima do limite / ticks above limit"
    );
    ensure!(
        scenario.measure_ticks >= 60,
        "medição curta demais / measurement too short"
    );
    ensure!(
        scenario.measure.len() <= MAX_MEASURED,
        "itens medidos demais / too many measured items"
    );
    ensure!(
        scenario.points.len() <= MAX_POINTS,
        "pontos demais / too many points"
    );
    for name in scenario.measure.iter().chain(scenario.prefuel.iter()) {
        ensure!(is_safe_name(name), "nome inválido / invalid name: {name:?}");
    }
    // PT: Um cenário precisa conferir alguma coisa; sem isso ele passaria sem testar nada.
    // EN: A scenario must check something; otherwise it would pass without testing anything.
    ensure!(
        scenario.compare || !scenario.expect.is_empty(),
        "cenário sem compare nem expect / scenario without compare or expect"
    );
    ensure!(
        scenario.compare || !scenario.compare_bonuses_only,
        "compare_bonuses_only exige compare / requires compare"
    );
    for point in &scenario.points {
        ensure!(
            point.x.is_finite()
                && point.y.is_finite()
                && point.x.abs() < 1e4
                && point.y.abs() < 1e4,
            "posição inválida / invalid position"
        );
        ensure!(
            point.items.len() <= 32,
            "itens demais no ponto / too many items in point"
        );
        for item in point.items.iter().chain(point.fluid.iter()) {
            ensure!(is_safe_name(item), "nome inválido / invalid name: {item:?}");
        }
        let needs_fluid = matches!(point.kind, PointKind::FluidSource | PointKind::FluidSink);
        ensure!(
            needs_fluid == point.fluid.is_some(),
            "ponto de fluido sem fluido (ou o contrário) / fluid point without fluid (or the reverse)"
        );
        if let Some(temperature) = point.temperature {
            ensure!(
                temperature.is_finite() && (-300.0..=5000.0).contains(&temperature),
                "temperatura inválida / invalid temperature"
            );
        }
    }
    ensure!(
        scenario.recipe_overrides.len() <= 64,
        "receitas demais / too many recipes"
    );
    for recipe in scenario.recipe_overrides.values() {
        ensure!(
            is_safe_name(recipe),
            "receita inválida / invalid recipe: {recipe:?}"
        );
    }
    for (name, expectation) in &scenario.expect {
        ensure!(
            scenario.measure.contains(name),
            "expectativa sem medição / expectation without measurement: {name}"
        );
        ensure!(
            expectation.produced_per_second.is_finite() && expectation.tolerance.is_finite(),
            "expectativa inválida / invalid expectation"
        );
    }

    let blueprint = match (&scenario.blueprint, &scenario.blueprint_json) {
        (Some(text), None) => text.trim().to_owned(),
        (None, Some(json)) => encode_raw_json(&json.to_string())?,
        _ => bail!(
            "informe blueprint, blueprint_json ou blueprint_file / provide blueprint, blueprint_json or blueprint_file"
        ),
    };
    let limits = Limits::default();
    precheck(&blueprint, &limits).map_err(|e| anyhow::anyhow!("blueprint: {e}"))?;
    let decoded = decode(&blueprint, &limits).map_err(|e| anyhow::anyhow!("blueprint: {e}"))?;
    let BlueprintString::Blueprint(content) = decoded.content else {
        bail!(
            "o cenário precisa de uma blueprint, não de um livro / the scenario needs a blueprint, not a book"
        );
    };

    // PT: Referência para achar o deslocamento no jogo: o menor x e o menor y entre os centros das
    //     entidades. O mod calcula o mesmo mínimo entre os fantasmas colados, e a diferença é o
    //     deslocamento (funciona mesmo sem nenhuma entidade de nome único).
    // EN: Reference to find the in-game offset: the smallest x and smallest y among entity centers.
    //     The mod computes the same minimum among the pasted ghosts, and the difference is the
    //     offset (it works even without any uniquely-named entity).
    ensure!(
        !content.entities.is_empty(),
        "blueprint sem entidades / blueprint without entities"
    );
    let anchor_x = content
        .entities
        .iter()
        .map(|e| e.position.x)
        .fold(f64::INFINITY, f64::min);
    let anchor_y = content
        .entities
        .iter()
        .map(|e| e.position.y)
        .fold(f64::INFINITY, f64::min);

    Ok(Prepared {
        anchor_x,
        anchor_y,
        blueprint,
        scenario,
    })
}

/// PT: Gera o `tests/current.lua`. A blueprint vai numa string longa `[==[ ... ]==]`, que não pode
///     ser fechada por dentro porque o alfabeto base64 não tem `]`.
/// EN: Generates `tests/current.lua`. The blueprint goes into a `[==[ ... ]==]` long string,
///     which cannot be closed from inside because the base64 alphabet has no `]`.
pub fn to_lua(prepared: &Prepared) -> Result<String> {
    ensure!(
        !prepared.blueprint.contains(']'),
        "blueprint com ']' / blueprint with ']'"
    );
    let scenario = &prepared.scenario;
    let power = match scenario.power {
        PowerMode::Supply => "supply",
        PowerMode::Load => "load",
        PowerMode::Cover => "cover",
    };
    let research = match scenario.research {
        Research::None => "none",
        Research::All => "all",
    };

    let mut lua =
        String::from("-- PT: Gerado pelo bf-validate. EN: Generated by bf-validate.\nreturn {\n");
    writeln!(lua, "  id = \"{}\",", scenario.id)?;
    writeln!(lua, "  blueprint = [==[{}]==],", prepared.blueprint)?;
    writeln!(lua, "  research = \"{research}\",")?;
    writeln!(lua, "  power = \"{power}\",")?;
    writeln!(lua, "  auto_belts = {},", scenario.auto_belts)?;
    if let Some(fuel) = &scenario.prefuel {
        writeln!(lua, "  prefuel = \"{fuel}\",")?;
    }
    writeln!(lua, "  warmup_ticks = {},", scenario.warmup_ticks)?;
    writeln!(lua, "  measure_ticks = {},", scenario.measure_ticks)?;
    writeln!(
        lua,
        "  anchor = {{ x = {:?}, y = {:?} }},",
        prepared.anchor_x, prepared.anchor_y
    )?;
    let measure: Vec<String> = scenario
        .measure
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect();
    writeln!(lua, "  measure = {{ {} }},", measure.join(", "))?;
    lua.push_str("  points = {\n");
    for point in &scenario.points {
        let kind = match point.kind {
            PointKind::Source => "source",
            PointKind::Sink => "sink",
            PointKind::FluidSource => "fluid_source",
            PointKind::FluidSink => "fluid_sink",
        };
        let items: Vec<String> = point
            .items
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect();
        let fluid = point
            .fluid
            .as_ref()
            .map_or_else(|| "nil".to_owned(), |name| format!("\"{name}\""));
        let temperature = point
            .temperature
            .map_or_else(|| "nil".to_owned(), |t| format!("{t:?}"));
        writeln!(
            lua,
            "    {{ kind = \"{kind}\", x = {:?}, y = {:?}, items = {{ {} }}, fluid = {fluid}, temperature = {temperature} }},",
            point.x,
            point.y,
            items.join(", ")
        )?;
    }
    lua.push_str("  },\n}\n");
    Ok(lua)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scenario(blueprint: Option<&str>) -> Scenario {
        Scenario {
            id: "test".into(),
            blueprint: blueprint.map(str::to_owned),
            blueprint_json: blueprint.is_none().then(|| {
                json!({"blueprint": {"item": "blueprint", "version": 562954249895936u64, "entities": [
                    {"entity_number": 1, "name": "assembling-machine-1", "position": {"x": 1.5, "y": 1.5}}
                ]}})
            }),
            blueprint_file: None,
            research: Research::None,
            warmup_ticks: 60,
            measure_ticks: 600,
            measure: vec!["iron-gear-wheel".into()],
            points: Vec::new(),
            expect: BTreeMap::new(),
            compare: true,
            compare_bonuses_only: false,
            auto_belts: false,
            power: PowerMode::Supply,
            recipe_overrides: BTreeMap::new(),
            prefuel: None,
        }
    }

    #[test]
    fn blueprint_file_stays_inside_the_validation_folder() {
        // PT: Pasta própria: `validation/golden/ok.txt` dentro e `secret.txt` fora, ambos existindo,
        //     para a recusa vir da regra do caminho e não de arquivo ausente.
        // EN: Own folder: `validation/golden/ok.txt` inside and `secret.txt` outside, both existing,
        //     so the refusal comes from the path rule and not from a missing file.
        let base = std::env::temp_dir().join(format!("bf-validate-test-{}", std::process::id()));
        let validation = base.join("validation");
        std::fs::create_dir_all(validation.join("golden")).unwrap();
        std::fs::write(validation.join("golden/ok.txt"), "0abc").unwrap();
        std::fs::write(base.join("secret.txt"), "0secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(base.join("secret.txt"), validation.join("golden/link.txt"))
            .unwrap();

        let with_file = |file: &str| {
            let mut s = scenario(None);
            s.blueprint_json = None;
            s.blueprint_file = Some(file.into());
            resolve_file(s, &validation)
        };
        let ok = with_file("golden/ok.txt").unwrap();
        assert_eq!(ok.blueprint.as_deref(), Some("0abc"));
        let mut evil = vec![
            "../secret.txt",
            "golden/../../secret.txt",
            "./golden/ok.txt",
        ];
        let absolute = base.join("secret.txt").display().to_string();
        evil.push(&absolute);
        if cfg!(unix) {
            evil.push("golden/link.txt");
        }
        for file in evil {
            let error = with_file(file).unwrap_err().to_string();
            assert!(
                error.contains("inválido") || error.contains("link"),
                "{file:?}: {error}"
            );
        }
        let mut both = scenario(None);
        both.blueprint_file = Some("golden/ok.txt".into());
        assert!(resolve_file(both, &validation).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn generates_lua_with_a_long_string() {
        let prepared = prepare(scenario(None)).unwrap();
        let lua = to_lua(&prepared).unwrap();
        assert!(lua.contains("blueprint = [==[0"));
        assert!(lua.contains("anchor = { x = 1.5, y = 1.5 }"));
    }

    #[test]
    fn rejects_lua_injection_attempts() {
        for evil in [
            "0abc]]..os.exit()--",
            "0abc\"; os.exit() --",
            "0ab\ncd",
            "0abc]==]",
        ] {
            assert!(prepare(scenario(Some(evil))).is_err(), "{evil:?}");
        }
        let mut bad_id = scenario(None);
        bad_id.id = "x\"; os.exit() --".into();
        assert!(prepare(bad_id).is_err());
        let mut bad_item = scenario(None);
        bad_item.measure = vec!["iron\"..os.exit()..\"".into()];
        assert!(prepare(bad_item).is_err());
        let mut bad_fuel = scenario(None);
        bad_fuel.prefuel = Some("coal\", x = (function() end)() --".into());
        assert!(prepare(bad_fuel).is_err());
    }

    #[test]
    fn rejects_scenarios_that_check_nothing() {
        let mut nothing = scenario(None);
        nothing.compare = false;
        nothing.expect.clear();
        assert!(prepare(nothing).is_err());
        let mut bonuses = scenario(None);
        bonuses.compare = false;
        bonuses.compare_bonuses_only = true;
        assert!(prepare(bonuses).is_err());
    }

    #[test]
    fn rejects_books_and_absurd_values() {
        let mut long = scenario(None);
        long.measure_ticks = MAX_TICKS + 1;
        assert!(prepare(long).is_err());
        let mut far = scenario(None);
        far.points.push(Point {
            kind: PointKind::Source,
            x: f64::INFINITY,
            y: 0.0,
            items: Vec::new(),
            fluid: None,
            temperature: None,
        });
        assert!(prepare(far).is_err());
    }
}

//! PT: Dados do gráfico sankey: nós são itens e grupos de receita, ligações são fluxos por
//!     segundo no regime estável da simulação. O d3-sankey não aceita ciclos, então cada ligação
//!     que fecharia um ciclo (kovarex, por exemplo) vai para uma cópia do nó com `loop: true`.
//! EN: Sankey chart data: nodes are items and recipe groups, links are per-second flows in the
//!     simulation's steady state. d3-sankey does not accept cycles, so each link that would close
//!     a cycle (kovarex, for example) goes to a copy of the node with `loop: true`.

use std::collections::{BTreeMap, BTreeSet};

use bf_calc::CalcReport;
use bf_data::GameData;
use bf_sim::FlowReport;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Item,
    Recipe,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Node {
    pub kind: NodeKind,
    pub name: String,
    #[serde(rename = "loop")]
    pub is_loop: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Link {
    pub source: usize,
    pub target: usize,
    pub per_second: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Sankey {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
}

pub fn build(data: &GameData, report: &CalcReport, flow: &FlowReport) -> Sankey {
    // PT: Ciclos por segundo de cada receita, somando as máquinas. EN: Cycles per second of each recipe, summing machines.
    let mut crafts: BTreeMap<&str, f64> = BTreeMap::new();
    let mut productivity: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    let simulated_by_number: BTreeMap<u64, &bf_sim::MachineFlow> =
        flow.machines.iter().map(|m| (m.entity_number, m)).collect();
    for machine in &report.machines {
        let Some(simulated) = simulated_by_number.get(&machine.entity_number) else {
            continue;
        };
        if let Some(recipe) = machine.recipe.as_deref() {
            *crafts.entry(recipe).or_insert(0.0) += simulated.crafts_per_second;
            let entry = productivity.entry(recipe).or_insert((0.0, 0.0));
            entry.0 += machine.effect.productivity * simulated.crafts_per_second;
            entry.1 += simulated.crafts_per_second;
        }
    }
    let mut edges: Vec<(NodeKind, String, NodeKind, String, f64)> = Vec::new();
    for (recipe_name, rate) in &crafts {
        if *rate <= 1e-12 {
            continue;
        }
        let Some(recipe) = data.recipes.get(*recipe_name) else {
            continue;
        };
        let bonus = productivity.get(recipe_name).map_or(0.0, |(sum, weight)| {
            if *weight > 0.0 { sum / weight } else { 0.0 }
        });
        for ingredient in &recipe.ingredients {
            edges.push((
                NodeKind::Item,
                ingredient.name.clone(),
                NodeKind::Recipe,
                recipe.name.clone(),
                ingredient.amount * rate,
            ));
        }
        for product in &recipe.results {
            let expected = product.expected_amount();
            let productive = (expected - product.ignored_by_productivity).max(0.0);
            edges.push((
                NodeKind::Recipe,
                recipe.name.clone(),
                NodeKind::Item,
                product.name.clone(),
                (expected + productive * bonus) * rate,
            ));
        }
    }

    let mut sankey = Sankey::default();
    let mut index: BTreeMap<(bool, String, bool), usize> = BTreeMap::new();
    let mut node = |sankey: &mut Sankey, kind: NodeKind, name: &str, is_loop: bool| -> usize {
        let key = (kind == NodeKind::Recipe, name.to_owned(), is_loop);
        *index.entry(key).or_insert_with(|| {
            sankey.nodes.push(Node {
                kind,
                name: name.to_owned(),
                is_loop,
            });
            sankey.nodes.len() - 1
        })
    };
    // PT: Uma ligação fecha ciclo se o destino já alcança a origem pelas ligações aceitas.
    // EN: A link closes a cycle if the target already reaches the source through the accepted links.
    let mut accepted: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (from_kind, from, to_kind, to, per_second) in edges {
        let source = node(&mut sankey, from_kind, &from, false);
        let mut target = node(&mut sankey, to_kind.clone(), &to, false);
        if reaches(&accepted, target, source) || source == target {
            target = node(&mut sankey, to_kind, &to, true);
        } else {
            accepted.entry(source).or_default().insert(target);
        }
        sankey.links.push(Link {
            source,
            target,
            per_second,
        });
    }
    sankey
}

fn reaches(graph: &BTreeMap<usize, BTreeSet<usize>>, from: usize, to: usize) -> bool {
    let mut pending = vec![from];
    let mut seen = BTreeSet::from([from]);
    while let Some(node) = pending.pop() {
        if node == to {
            return true;
        }
        for &next in graph.get(&node).into_iter().flatten() {
            if seen.insert(next) {
                pending.push(next);
            }
        }
    }
    false
}

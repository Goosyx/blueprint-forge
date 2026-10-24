//! PT: Conjunto de receitas suportado. O projeto cresce por etapas: primeiro as ciências
//!     1 a 3 e seus intermediários, depois outros grupos já testados.
//! EN: Supported recipe set. The project grows in stages: first sciences 1 to 3 and their
//!     intermediates, then other groups once they are tested.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{GameData, Recipe};

/// PT: Insumos que o gerador recebe prontos de fora (não são fabricados dentro da blueprint).
/// EN: Inputs the generator receives ready-made from outside (not crafted inside the blueprint).
pub const BASIC_INPUTS: &[&str] = &[
    "iron-plate",
    "copper-plate",
    "steel-plate",
    "stone-brick",
    "coal",
    "stone",
    "water",
    "petroleum-gas",
];

/// PT: Pacotes de ciência da primeira etapa.
/// EN: Science packs of the first stage.
pub const SCIENCE_1_TO_3: &[&str] = &[
    "automation-science-pack",
    "logistic-science-pack",
    "chemical-science-pack",
];

/// PT: Receitas suportadas, indexadas pelo produto que cada uma fabrica.
/// EN: Supported recipes, indexed by the product each one makes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scope {
    /// PT: Produto → nome da receita escolhida para fabricá-lo.
    /// EN: Product → name of the recipe chosen to make it.
    pub recipe_for: BTreeMap<String, String>,
    /// PT: Insumos que entram de fora.
    /// EN: Inputs that come from outside.
    pub inputs: BTreeSet<String>,
}

/// PT: Erro ao montar o conjunto de receitas.
/// EN: Error while building the recipe set.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScopeError {
    #[error("nenhuma receita fabrica {0:?} / no recipe makes {0:?}")]
    NoRecipe(String),
}

impl Scope {
    /// PT: Fechamento das ciências 1 a 3: cada ingrediente é seguido até chegar num insumo básico.
    /// EN: Closure of sciences 1 to 3: each ingredient is followed until it reaches a basic input.
    pub fn science_1_to_3(data: &GameData) -> Result<Self, ScopeError> {
        Self::closure(data, SCIENCE_1_TO_3, BASIC_INPUTS)
    }

    /// PT: Monta o fechamento de `targets`, parando nos nomes de `inputs`.
    ///     Usa pilha explícita (sem recursão) e visita cada produto uma vez só.
    /// EN: Builds the closure of `targets`, stopping at the names in `inputs`.
    ///     Uses an explicit stack (no recursion) and visits each product only once.
    pub fn closure(data: &GameData, targets: &[&str], inputs: &[&str]) -> Result<Self, ScopeError> {
        let inputs: BTreeSet<String> = inputs.iter().map(|name| (*name).to_owned()).collect();
        let mut recipe_for = BTreeMap::new();
        let mut pending: Vec<String> = targets.iter().map(|name| (*name).to_owned()).collect();

        while let Some(product) = pending.pop() {
            if inputs.contains(&product) || recipe_for.contains_key(&product) {
                continue;
            }
            let recipe = main_recipe_for(data, &product)
                .ok_or_else(|| ScopeError::NoRecipe(product.clone()))?;
            for ingredient in &recipe.ingredients {
                pending.push(ingredient.name.clone());
            }
            recipe_for.insert(product, recipe.name.clone());
        }

        Ok(Self { recipe_for, inputs })
    }

    /// PT: Indica se a receita faz parte do conjunto suportado.
    /// EN: Tells whether the recipe belongs to the supported set.
    pub fn contains_recipe(&self, recipe: &str) -> bool {
        self.recipe_for.values().any(|name| name == recipe)
    }
}

/// PT: Receita "principal" de um produto: a de mesmo nome, se existir; senão a única receita
///     visível que tem o produto como resultado principal.
/// EN: A product's "main" recipe: the same-named one if it exists; otherwise the only visible
///     recipe that has the product as its main result.
pub fn main_recipe_for<'a>(data: &'a GameData, product: &str) -> Option<&'a Recipe> {
    if let Some(recipe) = data.recipes.get(product)
        && recipe.results.iter().any(|result| result.name == product)
    {
        return Some(recipe);
    }
    let mut candidates = data.recipes.values().filter(|recipe| {
        !recipe.hidden
            && match &recipe.main_product {
                Some(main) => main == product,
                None => {
                    recipe.results.len() == 1
                        && recipe.results.iter().any(|result| result.name == product)
                }
            }
    });
    let first = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    Some(first)
}

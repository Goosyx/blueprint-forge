//! PT: Dados do jogo normalizados: itens, fluidos, receitas e entidades, mais o conjunto
//!     de receitas suportado em cada fase. Não faz I/O de arquivo e compila para WASM.
//! EN: Normalized game data: items, fluids, recipes and entities, plus the set of recipes
//!     supported in each phase. It does no file I/O and compiles to WASM.

pub mod locale;
pub mod model;
pub mod scope;
pub mod units;

pub use locale::Locale;
pub use model::*;
pub use scope::Scope;

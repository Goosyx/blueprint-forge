//! PT: Monta o atlas de ícones a partir dos PNGs do `--dump-icon-sprites`. Cada ícone ocupa
//!     uma célula de 64 px; ícones idênticos (por exemplo, item e receita de mesmo nome)
//!     dividem a mesma célula.
//! EN: Builds the icon atlas from the `--dump-icon-sprites` PNGs. Each icon takes one 64 px
//!     cell; identical icons (for example, a same-named item and recipe) share one cell.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use bf_data::GameData;
use image::{GenericImage, RgbaImage, imageops};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// PT: Tamanho da célula em pixels.
/// EN: Cell size in pixels.
pub const CELL: u32 = 64;
/// PT: Células por linha e por coluna de cada página (2048 px).
/// EN: Cells per row and per column on each page (2048 px).
pub const CELLS_PER_SIDE: u32 = 32;

/// PT: Posição de um ícone: página, coluna e linha.
/// EN: Position of an icon: page, column and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Slot {
    pub page: u32,
    pub col: u32,
    pub row: u32,
}

/// PT: Conteúdo do `atlas.json`. As chaves de `icons` são `"<categoria>/<nome>"`.
/// EN: Contents of `atlas.json`. The `icons` keys are `"<category>/<name>"`.
#[derive(Debug, Serialize)]
pub struct Atlas {
    pub cell: u32,
    pub pages: Vec<String>,
    pub icons: BTreeMap<String, Slot>,
}

/// PT: Monta o atlas com os ícones de itens, fluidos, receitas, entidades e sinais virtuais
///     conhecidos, grava as páginas em `out_dir` e devolve o índice.
/// EN: Builds the atlas with the icons of known items, fluids, recipes, entities and virtual
///     signals, writes the pages into `out_dir` and returns the index.
pub fn build(
    script_output: &Path,
    data: &GameData,
    virtual_signals: &[String],
    out_dir: &Path,
) -> Result<Atlas> {
    let mut wanted: Vec<(String, &str)> = Vec::new();
    let categories: [(&str, Vec<&String>); 5] = [
        ("item", data.items.keys().collect()),
        ("fluid", data.fluids.keys().collect()),
        ("recipe", data.recipes.keys().collect()),
        ("entity", data.entities.keys().collect()),
        ("virtual-signal", virtual_signals.iter().collect()),
    ];
    for (category, names) in &categories {
        for name in names {
            wanted.push((name.to_string(), category));
        }
    }

    let mut pages: Vec<RgbaImage> = Vec::new();
    let mut by_hash: BTreeMap<[u8; 32], Slot> = BTreeMap::new();
    let mut icons = BTreeMap::new();
    let mut next = 0u32;

    for (name, category) in wanted {
        if !is_safe_name(&name) {
            bail!("nome de protótipo inesperado / unexpected prototype name {name:?}");
        }
        let path = script_output.join(category).join(format!("{name}.png"));
        if !path.is_file() {
            continue;
        }
        let image = image::open(&path)
            .with_context(|| format!("PNG inválido / invalid PNG {}", path.display()))?
            .to_rgba8();
        let image = fit_cell(&image);

        let hash: [u8; 32] = Sha256::digest(image.as_raw()).into();
        let slot = match by_hash.get(&hash) {
            Some(slot) => *slot,
            None => {
                let per_page = CELLS_PER_SIDE * CELLS_PER_SIDE;
                let slot = Slot {
                    page: next / per_page,
                    col: next % CELLS_PER_SIDE,
                    row: (next % per_page) / CELLS_PER_SIDE,
                };
                if slot.page as usize >= pages.len() {
                    let side = CELL * CELLS_PER_SIDE;
                    pages.push(RgbaImage::new(side, side));
                }
                let page = pages
                    .get_mut(slot.page as usize)
                    .context("página do atlas ausente / missing atlas page")?;
                page.copy_from(&image, slot.col * CELL, slot.row * CELL)?;
                by_hash.insert(hash, slot);
                next += 1;
                slot
            }
        };
        icons.insert(format!("{category}/{name}"), slot);
    }

    fs::create_dir_all(out_dir)?;
    let mut page_names = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let file = format!("atlas-{index}.png");
        page.save(out_dir.join(&file))?;
        page_names.push(file);
    }

    Ok(Atlas {
        cell: CELL,
        pages: page_names,
        icons,
    })
}

// PT: Alguns ícones vêm maiores (por exemplo, 128 px) ou não quadrados. Reduz mantendo a
//     proporção e centraliza numa célula transparente de 64×64.
// EN: Some icons come larger (for example, 128 px) or non-square. Scales them down keeping the
//     aspect ratio and centers them in a transparent 64×64 cell.
fn fit_cell(image: &RgbaImage) -> RgbaImage {
    if image.width() == CELL && image.height() == CELL {
        return image.clone();
    }
    let scale = f64::from(CELL) / f64::from(image.width().max(image.height()));
    let width = ((f64::from(image.width()) * scale).round() as u32).clamp(1, CELL);
    let height = ((f64::from(image.height()) * scale).round() as u32).clamp(1, CELL);
    let resized = imageops::resize(image, width, height, imageops::FilterType::Lanczos3);
    let mut cell = RgbaImage::new(CELL, CELL);
    imageops::overlay(
        &mut cell,
        &resized,
        i64::from((CELL - width) / 2),
        i64::from((CELL - height) / 2),
    );
    cell
}

/// PT: Nomes de protótipo do jogo base só usam letras ASCII, dígitos, hífen e sublinhado.
///     Qualquer outra coisa (barra, ponto, espaço...) é recusada antes de virar caminho de arquivo.
/// EN: Base-game prototype names only use ASCII letters, digits, hyphens and underscores.
///     Anything else (slash, dot, space...) is refused before it becomes a file path.
pub fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_names() {
        assert!(is_safe_name("iron-gear-wheel"));
        assert!(is_safe_name("signal-0"));
        assert!(!is_safe_name("../etc/passwd"));
        assert!(!is_safe_name("a/b"));
        assert!(!is_safe_name(""));
        assert!(is_safe_name("signal-A"));
        assert!(!is_safe_name("a.png"));
        assert!(!is_safe_name("a b"));
    }
}

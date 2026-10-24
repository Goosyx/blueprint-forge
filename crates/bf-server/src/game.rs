//! PT: Pacotes do jogo carregados na memória na inicialização. Cada arquivo é conferido contra o
//!     sha256 do `manifest.json`, e as rotas só servem arquivos dessa lista, procurados por nome
//!     exato num mapa: nenhum caminho vindo da requisição chega ao sistema de arquivos.
//! EN: Game packages loaded into memory at startup. Each file is checked against the sha256 in
//!     `manifest.json`, and routes only serve files from that list, looked up by exact name in a
//!     map: no path coming from the request ever reaches the file system.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, bail};
use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ApiError;

/// PT: Idiomas aceitos na rota de nomes.
/// EN: Languages accepted by the names route.
const LANGUAGES: &[&str] = &["en", "pt-BR"];

#[derive(Deserialize)]
struct Manifest {
    game_version: String,
    #[serde(default)]
    has_icons: bool,
    #[serde(default)]
    unvalidated: bool,
    files: BTreeMap<String, String>,
}

/// PT: Um arquivo do pacote, já na memória.
/// EN: One package file, already in memory.
#[derive(Debug, Clone)]
pub struct Asset {
    pub bytes: Bytes,
    pub sha256: String,
    pub content_type: &'static str,
}

/// PT: Um pacote (uma versão do jogo).
/// EN: One package (one game version).
#[derive(Debug, Clone)]
pub struct Package {
    pub version: String,
    /// PT: Dados do jogo já interpretados, para as análises no servidor.
    /// EN: Already parsed game data, for server-side analyses.
    pub game_data: Arc<bf_data::GameData>,
    pub has_icons: bool,
    pub unvalidated: bool,
    /// PT: Caminho relativo no pacote → arquivo. Só os arquivos servidos entram aqui.
    /// EN: Relative path in the package → file. Only served files are kept here.
    pub files: BTreeMap<String, Asset>,
}

/// PT: Todos os pacotes disponíveis, indexados pela versão.
/// EN: All available packages, indexed by version.
#[derive(Debug, Clone, Default)]
pub struct GameStore {
    pub packages: BTreeMap<String, Package>,
}

/// PT: Versão aceita na URL: `2.1.23` ou `2.1.23-space-age`.
/// EN: Version accepted in the URL: `2.1.23` or `2.1.23-space-age`.
pub fn is_valid_version(version: &str) -> bool {
    let core = version.strip_suffix("-space-age").unwrap_or(version);
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 5 && part.bytes().all(|b| b.is_ascii_digit())
        })
}

// PT: Arquivos do pacote que o servidor serve; `raw/` fica de fora.
// EN: Package files the server serves; `raw/` is left out.
fn is_served(path: &str) -> bool {
    path == "gamedata.json" || path.starts_with("locale/") || path.starts_with("icons/")
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else {
        "application/octet-stream"
    }
}

impl GameStore {
    /// PT: Carrega todos os pacotes de `root`. Pacote com arquivo adulterado é recusado.
    /// EN: Loads every package from `root`. A package with a tampered file is rejected.
    pub fn load(root: &Path) -> Result<Self> {
        let mut store = Self::default();
        if !root.is_dir() {
            tracing::warn!(path = %root.display(), "pasta de pacotes ausente / package folder missing");
            return Ok(store);
        }
        for entry in fs::read_dir(root)? {
            let dir = entry?.path();
            let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !dir.is_dir() || !is_valid_version(name) {
                continue;
            }
            // PT: Pacote ruim (adulterado, incompleto) é registrado e pulado; os outros continuam.
            // EN: A bad package (tampered, incomplete) is logged and skipped; the others carry on.
            let package = match load_package(&dir) {
                Ok(package) if package.version == name => package,
                Ok(_) => {
                    tracing::error!(
                        version = name,
                        "versão do manifesto difere da pasta / manifest version differs from folder"
                    );
                    continue;
                }
                Err(error) => {
                    tracing::error!(version = name, error = %error, "pacote recusado / package rejected");
                    continue;
                }
            };
            tracing::info!(version = %package.version, files = package.files.len(), "pacote carregado / package loaded");
            store.packages.insert(package.version.clone(), package);
        }
        Ok(store)
    }

    /// PT: Dados da versão mais nova (pela ordem numérica das partes da versão).
    /// EN: Data of the newest version (by numeric order of the version parts).
    pub fn latest_data(&self) -> Option<(String, Arc<bf_data::GameData>)> {
        let key = |version: &str| -> Vec<u32> {
            version
                .split(['.', '-'])
                .map(|part| part.parse().unwrap_or(0))
                .collect()
        };
        self.packages
            .values()
            .max_by(|a, b| key(&a.version).cmp(&key(&b.version)))
            .map(|package| (package.version.clone(), package.game_data.clone()))
    }

    fn file(&self, version: &str, path: &str) -> Option<&Asset> {
        if !is_valid_version(version) {
            return None;
        }
        self.packages.get(version)?.files.get(path)
    }
}

fn load_package(dir: &Path) -> Result<Package> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(dir.join("manifest.json"))?)?;
    let mut files = BTreeMap::new();
    for (path, expected) in &manifest.files {
        if !is_served(path) {
            continue;
        }
        // PT: O caminho vem do nosso manifesto, mas mesmo assim recusa `..` e caminhos absolutos.
        // EN: The path comes from our own manifest, but `..` and absolute paths are still refused.
        if path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        {
            bail!("caminho inválido no manifesto / invalid manifest path: {path:?}");
        }
        let bytes = fs::read(dir.join(path))?;
        let actual = hex::encode(Sha256::digest(&bytes));
        if &actual != expected {
            bail!("sha256 não confere / sha256 mismatch: {path}");
        }
        files.insert(
            path.clone(),
            Asset {
                bytes: Bytes::from(bytes),
                sha256: actual,
                content_type: content_type(path),
            },
        );
    }
    let game_data = files
        .get("gamedata.json")
        .map(|asset| std::str::from_utf8(&asset.bytes).map_err(anyhow::Error::from))
        .transpose()?
        .map(bf_data::GameData::from_json)
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("gamedata.json ausente / missing"))?;
    Ok(Package {
        game_data: Arc::new(game_data),
        version: manifest.game_version,
        has_icons: manifest.has_icons,
        unvalidated: manifest.unvalidated,
        files,
    })
}

/// PT: Item da lista de versões.
/// EN: Version list item.
#[derive(Serialize)]
pub struct VersionInfo {
    version: String,
    has_icons: bool,
    unvalidated: bool,
}

/// PT: `GET /api/v1/game/versions`.
pub async fn versions(State(store): State<Arc<GameStore>>) -> Json<Vec<VersionInfo>> {
    Json(
        store
            .packages
            .values()
            .rev()
            .map(|package| VersionInfo {
                version: package.version.clone(),
                has_icons: package.has_icons,
                unvalidated: package.unvalidated,
            })
            .collect(),
    )
}

/// PT: `GET /api/v1/game/{version}/data`.
pub async fn data(
    State(store): State<Arc<GameStore>>,
    UrlPath(version): UrlPath<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    serve(&store, &version, "gamedata.json", &headers)
}

/// PT: `GET /api/v1/game/{version}/locale/{lang}`.
pub async fn locale(
    State(store): State<Arc<GameStore>>,
    UrlPath((version, language)): UrlPath<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if !LANGUAGES.contains(&language.as_str()) {
        return Err(ApiError::NotFound);
    }
    serve(
        &store,
        &version,
        &format!("locale/{language}.json"),
        &headers,
    )
}

/// PT: `GET /api/v1/assets/icons/{version}/{file}`. Só nomes exatos da lista do manifesto.
/// EN: `GET /api/v1/assets/icons/{version}/{file}`. Only exact names from the manifest list.
pub async fn icon(
    State(store): State<Arc<GameStore>>,
    UrlPath((version, file)): UrlPath<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if file.contains('/') || file.contains('\\') || file.starts_with('.') {
        return Err(ApiError::NotFound);
    }
    serve(&store, &version, &format!("icons/{file}"), &headers)
}

// PT: Responde o arquivo com ETag fraca (sha256): a compressão muda os bytes, então a comparação
//     é fraca. `If-None-Match` aceita lista, prefixo `W/` e `*`; se casar, devolve 304 sem corpo.
// EN: Responds with the file and a weak ETag (sha256): compression changes the bytes, so the
//     comparison is weak. `If-None-Match` accepts a list, the `W/` prefix and `*`; on a match it
//     returns 304 without a body.
fn serve(
    store: &GameStore,
    version: &str,
    path: &str,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let asset = store.file(version, path).ok_or(ApiError::NotFound)?;
    let tag = format!("\"{}\"", asset.sha256);
    let etag_value = HeaderValue::from_str(&format!("W/{tag}")).map_err(|_| ApiError::Internal)?;
    let cache = HeaderValue::from_static("public, max-age=86400");

    let matches = headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|candidate| candidate == "*" || candidate.trim_start_matches("W/") == tag);
    if matches {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag_value), (header::CACHE_CONTROL, cache)],
        )
            .into_response());
    }
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static(asset.content_type),
            ),
            (header::ETAG, etag_value),
            (header::CACHE_CONTROL, cache),
        ],
        Body::from(asset.bytes.clone()),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_versions() {
        assert!(is_valid_version("2.1.23"));
        assert!(is_valid_version("2.1.23-space-age"));
        assert!(!is_valid_version("2.0"));
        assert!(!is_valid_version("../2.1.23"));
        assert!(!is_valid_version("2.1.23/.."));
        assert!(!is_valid_version("2.0.x"));
        assert!(!is_valid_version(""));
    }
}

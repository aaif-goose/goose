use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

const HF_API_BASE: &str = "https://huggingface.co/api/models";
const HF_DOWNLOAD_BASE: &str = "https://huggingface.co";

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HfModelInfo {
    pub repo_id: String,
    pub author: String,
    pub model_name: String,
    pub downloads: u64,
    pub gguf_files: Vec<HfGgufFile>,
}

/// A single downloadable GGUF file (used internally and for downloads).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HfGgufFile {
    pub filename: String,
    pub size_bytes: u64,
    pub quantization: String,
    pub download_url: String,
}

/// A quantization variant — groups sharded files into one logical entry.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HfQuantVariant {
    pub quantization: String,
    pub size_bytes: u64,
    pub filename: String,
    pub download_url: String,
    pub description: &'static str,
    pub quality_rank: u8,
}

#[derive(Debug, Deserialize)]
struct HfApiModel {
    id: Option<String>,
    author: Option<String>,
    downloads: Option<u64>,
    siblings: Option<Vec<HfApiSibling>>,
}

#[derive(Debug, Deserialize)]
struct HfApiSibling {
    rfilename: String,
    #[serde(default)]
    size: Option<u64>,
}

struct QuantInfo {
    description: &'static str,
    quality_rank: u8,
}

// quality_rank groups quants by bit-level so that all N-bit variants sort
// together. Within a group, higher rank = higher quality.
//
//   1-bit:  10–19      4-bit:  40–49      8-bit:  80–89
//   2-bit:  20–29      5-bit:  50–59      16-bit: 90–94
//   3-bit:  30–39      6-bit:  60–69      32-bit: 95–99
//
const QUANT_TABLE: &[(&str, &str, u8)] = &[
    // 1-bit
    ("TQ1_0", "Tiny, ternary quantization", 10),
    ("IQ1_S", "Extremely small, very low quality", 11),
    ("IQ1_M", "Extremely small, very low quality", 12),
    // 2-bit
    ("IQ2_XXS", "Very small, low quality", 20),
    ("IQ2_XS", "Very small, low quality", 21),
    ("IQ2_S", "Very small, low quality", 22),
    ("IQ2_M", "Very small, low quality", 23),
    ("Q2_K", "Small, low quality", 24),
    ("Q2_K_S", "Small, low quality", 24),
    ("Q2_K_L", "Small, low quality", 25),
    ("Q2_K_XL", "Small, low quality", 26),
    // 3-bit
    ("IQ3_XXS", "Very small, moderate quality loss", 30),
    ("IQ3_XS", "Small, moderate quality loss", 31),
    ("IQ3_S", "Small, moderate quality loss", 32),
    ("IQ3_M", "Small, moderate quality loss", 33),
    ("Q3_K_S", "Small, moderate quality loss", 34),
    ("Q3_K_M", "Small, balanced quality/size", 35),
    ("Q3_K_L", "Medium-small, decent quality", 36),
    ("Q3_K_XL", "Medium-small, decent quality", 37),
    // 4-bit
    ("IQ4_XS", "Medium, good quality", 40),
    ("IQ4_NL", "Medium, good quality", 41),
    ("Q4_0", "Medium, good quality", 42),
    ("Q4_1", "Medium, good quality", 43),
    ("Q4_K_S", "Medium, good quality/size balance", 44),
    (
        "Q4_K_M",
        "Medium, recommended balance of quality and size",
        45,
    ),
    ("Q4_K_L", "Medium, good quality", 46),
    ("Q4_K_XL", "Medium, good quality", 47),
    (
        "MXFP4_MOE",
        "Medium, mixed-precision 4-bit for MoE models",
        48,
    ),
    // 5-bit
    ("Q5_0", "Medium-large, high quality", 50),
    ("Q5_1", "Medium-large, high quality", 51),
    ("Q5_K_S", "Medium-large, high quality", 52),
    ("Q5_K_M", "Medium-large, very high quality", 53),
    ("Q5_K_XL", "Medium-large, very high quality", 54),
    // 6-bit
    ("Q6_K", "Large, near-lossless quality", 60),
    ("Q6_K_XL", "Large, near-lossless quality", 61),
    // 8-bit
    ("Q8_0", "Large, near-lossless quality", 80),
    ("Q8_K_XL", "Large, near-lossless quality", 81),
    // 16-bit
    ("F16", "Full size, original quality (16-bit)", 90),
    ("BF16", "Full size, original quality (bfloat16)", 91),
    // 32-bit
    ("F32", "Full size, original quality (32-bit)", 95),
];

fn quant_info(quant: &str) -> QuantInfo {
    QUANT_TABLE
        .iter()
        .find(|(name, _, _)| *name == quant)
        .map(|(_, description, quality_rank)| QuantInfo {
            description,
            quality_rank: *quality_rank,
        })
        .unwrap_or(QuantInfo {
            description: "",
            quality_rank: 45,
        })
}

pub fn parse_quantization_from_filename(filename: &str) -> String {
    parse_quantization(filename)
}

fn parse_quantization(filename: &str) -> String {
    // Strip directory prefix (e.g. "Q5_K_M/Model-Q5_K_M-00001-of-00002.gguf")
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    let stem = basename.trim_end_matches(".gguf");

    // Strip shard suffix like "-00001-of-00004"
    let stem = if let Some(pos) = stem.rfind("-of-") {
        stem.get(..pos)
            .and_then(|s| s.rsplit_once('-').map(|(prefix, _)| prefix))
            .unwrap_or(stem)
    } else {
        stem
    };

    // The quantization tag is typically the last hyphen-separated component
    // that looks like a quant identifier (starts with Q, IQ, F, BF, TQ, MXFP, etc.)
    // e.g. "Qwen3-Coder-Next-Q4_K_M" -> "Q4_K_M"
    //      "Model-UD-IQ1_M" -> "IQ1_M"
    if let Some((_, candidate)) = stem.rsplit_once('-') {
        if looks_like_quant(candidate) {
            return candidate.to_string();
        }
    }

    // Fallback: try dot separator (e.g. "model.Q4_K_M")
    if let Some((_, candidate)) = stem.rsplit_once('.') {
        if looks_like_quant(candidate) {
            return candidate.to_string();
        }
    }

    "unknown".to_string()
}

fn looks_like_quant(s: &str) -> bool {
    let upper = s.to_uppercase();
    upper.starts_with("Q")
        || upper.starts_with("IQ")
        || upper.starts_with("TQ")
        || upper.starts_with("MXFP")
        || upper == "F16"
        || upper == "F32"
        || upper == "BF16"
}

fn is_shard_file(filename: &str) -> bool {
    // Matches patterns like "-00001-of-00003.gguf"
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    let stem = basename.trim_end_matches(".gguf");
    if let Some(pos) = stem.rfind("-of-") {
        stem.get(..pos)
            .and_then(|before| before.rsplit('-').next())
            .map(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
    } else {
        false
    }
}

fn build_download_url(repo_id: &str, filename: &str) -> String {
    format!("{}/{}/resolve/main/{}", HF_DOWNLOAD_BASE, repo_id, filename)
}

/// Derive the expected model filename stem from a repo_id.
/// e.g. "unsloth/gemma-4-26B-A4B-it-GGUF" → "gemma-4-26b-a4b-it" (lowercased)
fn model_stem_from_repo(repo_id: &str) -> String {
    let repo_name = repo_id.rsplit('/').next().unwrap_or(repo_id);
    let stem = repo_name
        .strip_suffix("-GGUF")
        .or_else(|| repo_name.strip_suffix("-gguf"))
        .unwrap_or(repo_name);
    stem.to_lowercase()
}

/// Check whether a GGUF file belongs to the main model (vs auxiliary files like mmproj).
/// Matches files whose basename starts with the model stem derived from the repo name.
fn is_model_file(filename: &str, model_stem_lower: &str) -> bool {
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    basename.to_lowercase().starts_with(model_stem_lower)
}

/// Collect GGUF files into quantization variants.
/// Single-file quants use the file directly.
/// Sharded quants (multiple files for one quantization) aggregate sizes and use the
/// first shard filename as the representative — the download path must handle all shards.
fn group_into_variants(repo_id: &str, files: Vec<HfApiSibling>) -> Vec<HfQuantVariant> {
    use std::collections::HashMap;

    let stem = model_stem_from_repo(repo_id);

    let gguf_files: Vec<_> = files
        .into_iter()
        .filter(|s| {
            s.rfilename.ends_with(".gguf")
                && is_model_file(&s.rfilename, &stem)
                && parse_quantization(&s.rfilename) != "unknown"
        })
        .collect();

    // Separate single files from shards
    let mut single_files: Vec<&HfApiSibling> = Vec::new();
    let mut shard_groups: HashMap<String, Vec<&HfApiSibling>> = HashMap::new();

    for file in &gguf_files {
        if is_shard_file(&file.rfilename) {
            let quant = parse_quantization(&file.rfilename);
            shard_groups.entry(quant).or_default().push(file);
        } else {
            single_files.push(file);
        }
    }

    let mut variants: Vec<HfQuantVariant> = Vec::new();
    let mut seen_quants: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Add single-file variants
    for s in single_files {
        let quant = parse_quantization(&s.rfilename);
        seen_quants.insert(quant.clone());
        let info = quant_info(&quant);
        let download_url = build_download_url(repo_id, &s.rfilename);
        variants.push(HfQuantVariant {
            quantization: quant,
            size_bytes: s.size.unwrap_or(0),
            filename: s.rfilename.clone(),
            download_url,
            description: info.description,
            quality_rank: info.quality_rank,
        });
    }

    // Add sharded variants (only if no single-file variant exists for that quant)
    for (quant, shards) in &shard_groups {
        if seen_quants.contains(quant) {
            continue;
        }
        let total_size: u64 = shards.iter().map(|s| s.size.unwrap_or(0)).sum();
        // Use the first shard as representative filename
        let first_shard = shards
            .iter()
            .min_by_key(|s| &s.rfilename)
            .expect("shard group is non-empty");
        let info = quant_info(quant);
        let download_url = build_download_url(repo_id, &first_shard.rfilename);
        variants.push(HfQuantVariant {
            quantization: quant.clone(),
            size_bytes: total_size,
            filename: first_shard.rfilename.clone(),
            download_url,
            description: info.description,
            quality_rank: info.quality_rank,
        });
    }

    // Sort descending by quality_rank, then by size descending as tiebreaker
    variants.sort_by(|a, b| {
        b.quality_rank
            .cmp(&a.quality_rank)
            .then_with(|| b.size_bytes.cmp(&a.size_bytes))
    });
    variants
}

pub async fn search_gguf_models(query: &str, limit: usize) -> Result<Vec<HfModelInfo>> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}?search={}&filter=gguf&sort=downloads&direction=-1&limit={}",
        HF_API_BASE, query, limit
    );

    let response = client
        .get(&url)
        .header("User-Agent", "goose-ai-agent")
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("HuggingFace API returned status {}", response.status());
    }

    let models: Vec<HfApiModel> = response.json().await?;

    let results = models
        .into_iter()
        .filter_map(|m| {
            let repo_id = m.id?;
            let siblings = m.siblings.unwrap_or_default();

            // The search endpoint may not include `siblings`; parse whatever
            // is available. Files are fetched on-demand via `get_repo_gguf_variants`.
            let gguf_files: Vec<HfGgufFile> = siblings
                .into_iter()
                .filter(|s| s.rfilename.ends_with(".gguf"))
                .map(|s| {
                    let quantization = parse_quantization(&s.rfilename);
                    let download_url = build_download_url(&repo_id, &s.rfilename);
                    HfGgufFile {
                        filename: s.rfilename,
                        size_bytes: s.size.unwrap_or(0),
                        quantization,
                        download_url,
                    }
                })
                .collect();

            let author = m
                .author
                .unwrap_or_else(|| repo_id.split('/').next().unwrap_or_default().to_string());
            let model_name = repo_id
                .split('/')
                .next_back()
                .unwrap_or(&repo_id)
                .to_string();

            Some(HfModelInfo {
                repo_id,
                author,
                model_name,
                downloads: m.downloads.unwrap_or(0),
                gguf_files,
            })
        })
        .collect();

    Ok(results)
}

/// Fetch GGUF files for a repo and return them grouped by quantization.
pub async fn get_repo_gguf_variants(repo_id: &str) -> Result<Vec<HfQuantVariant>> {
    let client = reqwest::Client::new();
    let url = format!("{}/{}?blobs=true", HF_API_BASE, repo_id);

    let response = client
        .get(&url)
        .header("User-Agent", "goose-ai-agent")
        .send()
        .await?;

    if !response.status().is_success() {
        bail!(
            "HuggingFace API returned status {} for repo {}",
            response.status(),
            repo_id
        );
    }

    let model: HfApiModel = response.json().await?;
    let siblings = model.siblings.unwrap_or_default();

    Ok(group_into_variants(repo_id, siblings))
}

/// Fetch raw GGUF files (kept for resolve_model_spec).
pub async fn get_repo_gguf_files(repo_id: &str) -> Result<Vec<HfGgufFile>> {
    let client = reqwest::Client::new();
    let url = format!("{}/{}?blobs=true", HF_API_BASE, repo_id);

    let response = client
        .get(&url)
        .header("User-Agent", "goose-ai-agent")
        .send()
        .await?;

    if !response.status().is_success() {
        bail!(
            "HuggingFace API returned status {} for repo {}",
            response.status(),
            repo_id
        );
    }

    let model: HfApiModel = response.json().await?;
    let siblings = model.siblings.unwrap_or_default();

    let stem = model_stem_from_repo(repo_id);

    let files = siblings
        .into_iter()
        .filter(|s| s.rfilename.ends_with(".gguf"))
        .filter(|s| !is_shard_file(&s.rfilename))
        .filter(|s| is_model_file(&s.rfilename, &stem))
        .map(|s| {
            let quantization = parse_quantization(&s.rfilename);
            let download_url = build_download_url(repo_id, &s.rfilename);
            HfGgufFile {
                filename: s.rfilename,
                size_bytes: s.size.unwrap_or(0),
                quantization,
                download_url,
            }
        })
        .collect();

    Ok(files)
}

/// Parse a model spec like "bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M" into (repo_id, quantization).
pub fn parse_model_spec(spec: &str) -> Result<(String, String)> {
    let (repo_id, quant) = spec.rsplit_once(':').ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid model spec '{}': expected format 'user/repo:quantization'",
            spec
        )
    })?;

    if !repo_id.contains('/') {
        bail!("Invalid repo_id '{}': expected format 'user/repo'", repo_id);
    }

    Ok((repo_id.to_string(), quant.to_string()))
}

/// Resolve a model spec to a specific GGUF file from the repo.
pub async fn resolve_model_spec(spec: &str) -> Result<(String, HfGgufFile)> {
    let (repo_id, quant) = parse_model_spec(spec)?;
    let files = get_repo_gguf_files(&repo_id).await?;

    let file = files
        .into_iter()
        .find(|f| f.quantization.eq_ignore_ascii_case(&quant))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No GGUF file with quantization '{}' found in {}",
                quant,
                repo_id
            )
        })?;

    Ok((repo_id, file))
}

/// Recommend which quantization variant to use based on available memory.
pub fn recommend_variant(
    variants: &[HfQuantVariant],
    available_memory_bytes: u64,
) -> Option<usize> {
    // We need ~10-20% overhead beyond model size for inference context.
    // Pick the highest-quality variant that fits.
    let usable = (available_memory_bytes as f64 * 0.85) as u64;

    let mut best: Option<usize> = None;
    for (i, v) in variants.iter().enumerate() {
        if v.size_bytes <= usable {
            match best {
                Some(bi) if variants[bi].quality_rank < v.quality_rank => best = Some(i),
                None => best = Some(i),
                _ => {}
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quantization() {
        assert_eq!(parse_quantization("Model-Q4_K_M.gguf"), "Q4_K_M");
        assert_eq!(parse_quantization("Model-Q8_0.gguf"), "Q8_0");
        assert_eq!(parse_quantization("Model-IQ4_NL.gguf"), "IQ4_NL");
        assert_eq!(parse_quantization("Model-F16.gguf"), "F16");
        assert_eq!(parse_quantization("random-name.gguf"), "unknown");
    }

    #[test]
    fn test_parse_quantization_with_directory() {
        assert_eq!(
            parse_quantization("Q5_K_M/Model-Q5_K_M-00001-of-00002.gguf"),
            "Q5_K_M"
        );
    }

    #[test]
    fn test_parse_quantization_extended_tags() {
        assert_eq!(parse_quantization("Model-MXFP4_MOE.gguf"), "MXFP4_MOE");
        assert_eq!(parse_quantization("Model-UD-TQ1_0.gguf"), "TQ1_0");
        assert_eq!(parse_quantization("Model-Q2_K_L.gguf"), "Q2_K_L");
        assert_eq!(parse_quantization("Model-UD-Q4_K_XL.gguf"), "Q4_K_XL");
        assert_eq!(parse_quantization("Model-UD-IQ1_M.gguf"), "IQ1_M");
    }

    #[test]
    fn test_is_shard_file() {
        assert!(is_shard_file("Q5_K_M/Model-Q5_K_M-00001-of-00002.gguf"));
        assert!(is_shard_file("Model-BF16-00003-of-00004.gguf"));
        assert!(!is_shard_file("Model-Q4_K_M.gguf"));
    }

    #[test]
    fn test_parse_model_spec() {
        let (repo, quant) =
            parse_model_spec("bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M").unwrap();
        assert_eq!(repo, "bartowski/Llama-3.2-1B-Instruct-GGUF");
        assert_eq!(quant, "Q4_K_M");
    }

    #[test]
    fn test_parse_model_spec_invalid() {
        assert!(parse_model_spec("no-colon").is_err());
        assert!(parse_model_spec("noslash:Q4_K_M").is_err());
    }

    #[test]
    fn test_recommend_variant() {
        let variants = vec![
            HfQuantVariant {
                quantization: "Q2_K".into(),
                size_bytes: 2_000_000_000,
                filename: "m-Q2_K.gguf".into(),
                download_url: String::new(),
                description: "Small",
                quality_rank: 24,
            },
            HfQuantVariant {
                quantization: "Q4_K_M".into(),
                size_bytes: 4_000_000_000,
                filename: "m-Q4_K_M.gguf".into(),
                download_url: String::new(),
                description: "Medium",
                quality_rank: 45,
            },
            HfQuantVariant {
                quantization: "Q8_0".into(),
                size_bytes: 8_000_000_000,
                filename: "m-Q8_0.gguf".into(),
                download_url: String::new(),
                description: "Large",
                quality_rank: 80,
            },
        ];

        assert_eq!(recommend_variant(&variants, 5_000_000_000), Some(1));
        assert_eq!(recommend_variant(&variants, 10_000_000_000), Some(2));
        assert_eq!(recommend_variant(&variants, 1_000_000_000), None);
    }

    #[test]
    fn test_model_stem_from_repo() {
        assert_eq!(
            model_stem_from_repo("unsloth/gemma-4-26B-A4B-it-GGUF"),
            "gemma-4-26b-a4b-it"
        );
        assert_eq!(
            model_stem_from_repo("bartowski/Llama-3.2-3B-Instruct-GGUF"),
            "llama-3.2-3b-instruct"
        );
        assert_eq!(model_stem_from_repo("someone/SomeModel"), "somemodel");
    }

    #[test]
    fn test_is_model_file() {
        let stem = "gemma-3-27b-it";
        assert!(is_model_file("gemma-3-27b-it-Q4_K_M.gguf", stem));
        assert!(is_model_file(
            "BF16/gemma-3-27b-it-BF16-00001-of-00002.gguf",
            stem
        ));
        assert!(!is_model_file("mmproj-BF16.gguf", stem));
        assert!(!is_model_file("vision-encoder-Q4_K_M.gguf", stem));
    }

    #[test]
    fn test_group_into_variants_filters_auxiliary_files() {
        let files = vec![
            HfApiSibling {
                rfilename: "gemma-3-27b-it-Q4_K_M.gguf".into(),
                size: Some(4_000_000_000),
            },
            HfApiSibling {
                rfilename: "mmproj-BF16.gguf".into(),
                size: Some(800_000_000),
            },
        ];
        let variants = group_into_variants("unsloth/gemma-3-27b-it-GGUF", files);
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].quantization, "Q4_K_M");
    }

    #[test]
    fn test_group_into_variants_aggregates_shards() {
        let files = vec![
            HfApiSibling {
                rfilename: "BF16/gemma-3-27b-it-BF16-00001-of-00002.gguf".into(),
                size: Some(40_000_000_000),
            },
            HfApiSibling {
                rfilename: "BF16/gemma-3-27b-it-BF16-00002-of-00002.gguf".into(),
                size: Some(10_000_000_000),
            },
            HfApiSibling {
                rfilename: "gemma-3-27b-it-Q4_K_M.gguf".into(),
                size: Some(4_000_000_000),
            },
        ];
        let variants = group_into_variants("unsloth/gemma-3-27b-it-GGUF", files);
        assert_eq!(variants.len(), 2);
        // Sorted descending by quality_rank: BF16 (27) > Q4_K_M (19)
        assert_eq!(variants[0].quantization, "BF16");
        assert_eq!(variants[0].size_bytes, 50_000_000_000);
        assert_eq!(variants[1].quantization, "Q4_K_M");
    }

    #[test]
    fn test_group_into_variants_sorted_descending() {
        let files = vec![
            HfApiSibling {
                rfilename: "Model-IQ1_S.gguf".into(),
                size: Some(500_000_000),
            },
            HfApiSibling {
                rfilename: "Model-Q4_K_M.gguf".into(),
                size: Some(4_000_000_000),
            },
            HfApiSibling {
                rfilename: "Model-Q8_0.gguf".into(),
                size: Some(8_000_000_000),
            },
        ];
        let variants = group_into_variants("someone/Model-GGUF", files);
        assert_eq!(variants.len(), 3);
        assert_eq!(variants[0].quantization, "Q8_0");
        assert_eq!(variants[1].quantization, "Q4_K_M");
        assert_eq!(variants[2].quantization, "IQ1_S");
    }
}

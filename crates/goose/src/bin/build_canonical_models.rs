/// Build canonical models from models.dev API
///
/// This script fetches models from models.dev and converts them to canonical format.
/// By default, it also checks which models from top providers are properly mapped.
///
/// Usage:
///   cargo run --bin build_canonical_models              # Build and check (default)
///   cargo run --bin build_canonical_models --no-check   # Build only, skip checker
///
use anyhow::{Context, Result};
use clap::Parser;
use goose::providers::create_standard;
use goose_providers::canonical::ModelMapping;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderMetadata {
    pub id: String,
    pub display_name: String,
    pub npm: Option<String>,
    pub api: Option<String>,
    pub doc: Option<String>,
    pub env: Vec<String>,
    pub model_count: usize,
}

const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
const SEPARATOR: &str =
    "================================================================================";
const SUBSEPARATOR: &str =
    "--------------------------------------------------------------------------------";

fn is_compatible_provider(npm: &str) -> bool {
    npm.contains("openai") || npm.contains("anthropic") || npm.contains("ollama")
}

fn normalize_provider_name(provider: &str) -> &str {
    match provider {
        "llama" => "meta-llama",
        "xai" => "x-ai",
        "mistral" => "mistralai",
        _ => provider,
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Skip the canonical model checker (only build models)
    #[arg(long)]
    do_check: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
struct ProviderModelPair {
    provider: String,
    model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MappingEntry {
    provider: String,
    model: String,
    canonical: String,
    recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MappingReport {
    timestamp: String,
    unmapped_models: Vec<ProviderModelPair>,
    all_mappings: BTreeMap<String, Vec<ModelMapping>>,
    mapped_models: Vec<MappingEntry>,
    model_counts: BTreeMap<String, usize>,
    canonical_models_used: BTreeSet<String>,
}

impl MappingReport {
    fn new() -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            unmapped_models: Vec::new(),
            all_mappings: BTreeMap::new(),
            mapped_models: Vec::new(),
            model_counts: BTreeMap::new(),
            canonical_models_used: BTreeSet::new(),
        }
    }

    fn add_provider_results(
        &mut self,
        provider_name: &str,
        fetched_models: Vec<String>,
        mappings: Vec<ModelMapping>,
        recommended_models: Vec<String>,
    ) {
        let mapping_map: HashMap<&str, &str> = mappings
            .iter()
            .map(|m| (m.provider_model.as_str(), m.canonical_model.as_str()))
            .collect();

        let recommended_set: std::collections::HashSet<&str> =
            recommended_models.iter().map(|s| s.as_str()).collect();

        for model in &fetched_models {
            if !mapping_map.contains_key(model.as_str()) {
                self.unmapped_models.push(ProviderModelPair {
                    provider: provider_name.to_string(),
                    model: model.clone(),
                });
            }
        }

        for mapping in &mappings {
            self.canonical_models_used
                .insert(mapping.canonical_model.clone());
            self.mapped_models.push(MappingEntry {
                provider: provider_name.to_string(),
                model: mapping.provider_model.clone(),
                canonical: mapping.canonical_model.clone(),
                recommended: recommended_set.contains(mapping.provider_model.as_str()),
            });
        }

        self.all_mappings
            .insert(provider_name.to_string(), mappings);
        self.model_counts
            .insert(provider_name.to_string(), fetched_models.len());
    }

    fn print_summary(&self) {
        println!("\n{SEPARATOR}");
        println!("CANONICAL MODEL MAPPING REPORT");
        println!("{SEPARATOR}");
        println!("\nGenerated: {}\n", self.timestamp);

        println!("Models Checked Per Provider:");
        println!("{SUBSEPARATOR}");
        let mut providers: Vec<_> = self.model_counts.iter().collect();
        providers.sort_by_key(|(name, _)| *name);
        for (provider, count) in providers {
            let mapped = self
                .all_mappings
                .get(provider)
                .map(|m| m.len())
                .unwrap_or(0);
            let unmapped = count - mapped;
            println!(
                "  {:<20} Total: {:>3}  Mapped: {:>3}  Unmapped: {:>3}",
                provider, count, mapped, unmapped
            );
        }

        println!("\n{SEPARATOR}");
        println!("UNMAPPED MODELS ({})", self.unmapped_models.len());
        println!("{SEPARATOR}");

        if self.unmapped_models.is_empty() {
            println!("✓ All models are mapped to canonical models!");
        } else {
            let mut unmapped_by_provider: HashMap<&str, Vec<&str>> = HashMap::new();
            for pair in &self.unmapped_models {
                unmapped_by_provider
                    .entry(pair.provider.as_str())
                    .or_default()
                    .push(pair.model.as_str());
            }

            let mut providers: Vec<_> = unmapped_by_provider.keys().collect();
            providers.sort();

            for provider in providers {
                println!("\n{}:", provider);
                let mut models = unmapped_by_provider[provider].to_vec();
                models.sort();
                for model in models {
                    println!("  - {}", model);
                }
            }
        }

        println!("\n{SEPARATOR}");
        println!(
            "CANONICAL MODELS REFERENCED ({})",
            self.canonical_models_used.len()
        );
        println!("{SEPARATOR}");
        if self.canonical_models_used.is_empty() {
            println!("  (none yet)");
        } else {
            let mut canonical: Vec<_> = self.canonical_models_used.iter().collect();
            canonical.sort();
            for model in canonical {
                println!("  - {}", model);
            }
        }

        println!("\n{SEPARATOR}");
    }

    fn compare_with_previous(&self, previous: &MappingReport) {
        println!("\n{SEPARATOR}");
        println!("CHANGES SINCE PREVIOUS RUN");
        println!("{SEPARATOR}");

        let prev_map: HashMap<(&str, &str), &str> = previous
            .mapped_models
            .iter()
            .map(|e| {
                (
                    (e.provider.as_str(), e.model.as_str()),
                    e.canonical.as_str(),
                )
            })
            .collect();

        let curr_map: HashMap<(&str, &str), &str> = self
            .mapped_models
            .iter()
            .map(|e| {
                (
                    (e.provider.as_str(), e.model.as_str()),
                    e.canonical.as_str(),
                )
            })
            .collect();

        let mut changed_mappings = Vec::new();
        let mut added_mappings = Vec::new();
        let mut removed_mappings = Vec::new();

        for (&key @ (provider, model), &canonical) in &curr_map {
            match prev_map.get(&key) {
                Some(&prev_canonical) if prev_canonical != canonical => {
                    changed_mappings.push((provider, model, prev_canonical, canonical));
                }
                None => {
                    added_mappings.push((provider, model, canonical));
                }
                _ => {}
            }
        }

        for (&key @ (provider, model), &canonical) in &prev_map {
            if !curr_map.contains_key(&key) {
                removed_mappings.push((provider, model, canonical));
            }
        }

        if changed_mappings.is_empty() && added_mappings.is_empty() && removed_mappings.is_empty() {
            println!("\nNo changes in model mappings.");
        } else {
            if !changed_mappings.is_empty() {
                println!("\n⚠ Changed Mappings ({}):", changed_mappings.len());
                println!("  (Models that now map to a different canonical model)");
                for (provider, model, old_canonical, new_canonical) in changed_mappings {
                    println!("  {} / {}", provider, model);
                    println!("    WAS: {}", old_canonical);
                    println!("    NOW: {}", new_canonical);
                }
            }

            if !added_mappings.is_empty() {
                println!("\n✓ Added Mappings ({}):", added_mappings.len());
                println!("  (Models that gained a canonical mapping)");
                for (provider, model, canonical) in added_mappings {
                    println!("  {} / {} -> {}", provider, model, canonical);
                }
            }

            if !removed_mappings.is_empty() {
                println!("\n✗ Removed Mappings ({}):", removed_mappings.len());
                println!("  (Models that lost their canonical mapping)");
                for (provider, model, canonical) in removed_mappings {
                    println!("  {} / {} (was: {})", provider, model, canonical);
                }
            }
        }

        println!("\n{SEPARATOR}");
    }

    fn save_to_file(&self, path: &PathBuf) -> Result<()> {
        let mut report = self.clone();

        report.unmapped_models.sort_by(|a, b| {
            a.provider
                .cmp(&b.provider)
                .then_with(|| a.model.cmp(&b.model))
        });

        report.mapped_models.sort_by(|a, b| {
            a.provider
                .cmp(&b.provider)
                .then_with(|| a.model.cmp(&b.model))
        });

        for mappings in report.all_mappings.values_mut() {
            mappings.sort_by(|a, b| a.provider_model.cmp(&b.provider_model));
        }

        let json = serde_json::to_string_pretty(&report).context("Failed to serialize report")?;
        std::fs::write(path, json).context("Failed to write report file")?;
        Ok(())
    }

    fn load_from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("Failed to read report file")?;
        let report: MappingReport =
            serde_json::from_str(&content).context("Failed to parse report file")?;
        Ok(report)
    }
}

fn data_file_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../goose-provider-types/src/canonical/data")
        .join(filename)
}

async fn fetch_models_dev() -> Result<Value> {
    println!("Fetching models from models.dev API...");

    let client = reqwest::Client::new();
    let response = client
        .get(MODELS_DEV_API_URL)
        .header("User-Agent", "goose/canonical-builder")
        .send()
        .await
        .context("Failed to fetch from models.dev API")?;

    response
        .json()
        .await
        .context("Failed to parse models.dev response")
}

fn collect_provider_metadata(
    providers_obj: &serde_json::Map<String, Value>,
) -> Vec<ProviderMetadata> {
    let mut metadata_list = Vec::new();

    for (provider_id, provider_data) in providers_obj {
        let npm = match provider_data.get("npm").and_then(|v| v.as_str()) {
            Some(n) if !n.is_empty() => n,
            _ => continue,
        };

        if !is_compatible_provider(npm) {
            continue;
        }

        let api = provider_data
            .get("api")
            .and_then(|v| v.as_str())
            .map(String::from);

        if api.is_none() {
            continue;
        }

        let normalized_provider = normalize_provider_name(provider_id).to_string();
        let doc = provider_data
            .get("doc")
            .and_then(|v| v.as_str())
            .map(String::from);
        let env = provider_data
            .get("env")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let display_name = provider_data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(provider_id)
            .to_string();
        let model_count = provider_data
            .get("models")
            .and_then(|v| v.as_object())
            .map(|models| models.len())
            .unwrap_or(0);

        metadata_list.push(ProviderMetadata {
            id: normalized_provider,
            display_name,
            npm: Some(npm.to_string()),
            api,
            doc,
            env,
            model_count,
        });

        println!("  Added {} ({}) - {} models", provider_id, npm, model_count);
    }

    metadata_list.sort_by(|a, b| a.id.cmp(&b.id));
    metadata_list
}

async fn build_canonical_models() -> Result<()> {
    let json = fetch_models_dev().await?;
    let registry = goose_providers::canonical::models_dev::from_models_dev(&json.to_string())?;
    let output_path = data_file_path("canonical_models.json");
    registry.to_file(&output_path)?;
    println!(
        "Wrote {} models to {}",
        registry.count(),
        output_path.display()
    );

    let providers_obj = json
        .as_object()
        .context("Expected object in models.dev response")?;
    let metadata = collect_provider_metadata(providers_obj);
    std::fs::write(
        data_file_path("provider_metadata.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;
    Ok(())
}

async fn check_provider(
    provider_name: &str,
    _model_for_init: &str,
) -> Result<(Vec<String>, Vec<ModelMapping>, Vec<String>)> {
    println!("Checking provider: {}", provider_name);

    let provider = match create_standard(provider_name, Vec::new()).await {
        Ok(p) => p,
        Err(e) => {
            println!("  ⚠ Failed to create provider: {}", e);
            println!("  This is expected if credentials are not configured.");
            return Ok((Vec::new(), Vec::new(), Vec::new()));
        }
    };

    let fetched_models = match provider.fetch_supported_models().await {
        Ok(models) => {
            println!("  ✓ Fetched {} models", models.len());
            models
        }
        Err(e) => {
            println!("  ⚠ Failed to fetch models: {}", e);
            println!("  This is expected if credentials are not configured.");
            Vec::new()
        }
    };

    let recommended_models = match provider.fetch_recommended_models(false).await {
        Ok(models) => {
            println!("  ✓ Found {} recommended models", models.len());
            models
        }
        Err(e) => {
            println!("  ⚠ Failed to fetch recommended models: {}", e);
            Vec::new()
        }
    };

    let mut mappings = Vec::new();
    for model in &fetched_models {
        match provider.map_to_canonical_model(model).await {
            Ok(Some(canonical)) => {
                mappings.push(ModelMapping::new(model.clone(), canonical));
            }
            Ok(None) => {
                // No mapping found for this model
            }
            Err(e) => {
                println!("  ⚠ Failed to map model '{}': {}", model, e);
            }
        }
    }
    println!("  ✓ Found {} mappings", mappings.len());

    Ok((fetched_models, mappings, recommended_models))
}

async fn check_canonical_mappings() -> Result<()> {
    println!("\n{SEPARATOR}");
    println!("Canonical Model Checker");
    println!("Checking model mappings for top providers...\n");

    let providers = vec![
        ("anthropic", "claude-3-5-sonnet-20241022"),
        ("openai", "gpt-4"),
        ("openrouter", "anthropic/claude-3.5-sonnet"),
        ("google", "gemini-1.5-pro-002"),
        ("databricks", "claude-3-5-sonnet-20241022"),
        ("tetrate", "claude-3-5-sonnet-computer-use"),
        ("xai", "grok-code-fast-1"),
        ("azure_openai", "gpt-4o"),
        ("aws_bedrock", "anthropic.claude-3-5-sonnet-20241022-v2:0"),
        ("venice", "llama-3.3-70b"),
        ("gcp_vertex_ai", "gemini-1.5-pro-002"),
    ];

    let mut report = MappingReport::new();

    for (provider_name, default_model) in providers {
        let (fetched, mappings, recommended) = check_provider(provider_name, default_model).await?;
        report.add_provider_results(provider_name, fetched, mappings, recommended);
        println!();
    }

    report.print_summary();

    let output_path = data_file_path("canonical_mapping_report.json");

    if output_path.exists() {
        if let Ok(previous) = MappingReport::load_from_file(&output_path) {
            report.compare_with_previous(&previous);
        }
    }

    report.save_to_file(&output_path)?;
    println!("\n✓ Report saved to: {}", output_path.display());

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    build_canonical_models().await?;

    if args.do_check {
        check_canonical_mappings().await?;
    }

    Ok(())
}

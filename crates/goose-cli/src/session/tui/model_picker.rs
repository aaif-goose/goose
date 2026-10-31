#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ModelSource {
    Configured,
    Ollama,
    GooseLocal,
    Catalog,
}

impl ModelSource {
    fn label(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Ollama => "ollama",
            Self::GooseLocal => "goose local",
            Self::Catalog => "catalog",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModelAvailability {
    Configured,
    Installed,
    Downloadable,
}

impl ModelAvailability {
    fn label(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Installed => "installed",
            Self::Downloadable => "downloadable",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct MetadataValue<T> {
    pub value: T,
    pub provenance: MetadataProvenance,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum MetadataProvenance {
    Provider,
    Catalog,
    #[allow(dead_code)]
    Estimate,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ModelMetadata {
    pub context_length: Option<MetadataValue<u64>>,
    pub supports_tools: Option<MetadataValue<bool>>,
    pub supports_vision: Option<MetadataValue<bool>>,
    pub size_bytes: Option<MetadataValue<u64>>,
    pub performance: Option<MetadataValue<String>>,
    pub fit: Option<super::model_fit::ModelFitAssessment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ModelInventoryEntry {
    pub provider: String,
    pub model: String,
    pub source: ModelSource,
    pub availability: ModelAvailability,
    pub is_active: bool,
    pub is_default: bool,
    pub metadata: ModelMetadata,
    pub(super) download: Option<ModelDownloadSpec>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::session) struct ModelSelection {
    pub provider: String,
    pub model: String,
    pub(super) download: Option<ModelDownloadSpec>,
    pub(super) fit: Option<super::model_fit::ModelFitAssessment>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct ModelDownloadSpec {
    pub spec: String,
    pub backend_id: String,
    pub variant_id: String,
}

impl ModelSelection {
    pub(super) fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            download: None,
            fit: None,
        }
    }
}

pub(super) struct ModelPickerState {
    entries: Vec<ModelInventoryEntry>,
    visible_entries: Vec<ModelInventoryEntry>,
    query: String,
}

impl ModelPickerState {
    pub(super) fn new(
        input_entries: Vec<ModelInventoryEntry>,
        active: Option<ModelSelection>,
    ) -> Self {
        let mut entries: Vec<ModelInventoryEntry> = Vec::with_capacity(input_entries.len());

        for mut entry in input_entries {
            let key = (entry.provider.to_lowercase(), entry.model.to_lowercase());
            if let Some(existing) = entries.iter_mut().find(|existing| {
                (
                    existing.provider.to_lowercase(),
                    existing.model.to_lowercase(),
                ) == key
            }) {
                existing.is_active |= entry.is_active;
                existing.is_default |= entry.is_default;
                if entry.availability == ModelAvailability::Installed {
                    existing.availability = ModelAvailability::Installed;
                    existing.source = entry.source;
                }
                if existing.metadata.context_length.is_none() {
                    existing.metadata.context_length = entry.metadata.context_length.take();
                }
                if existing.metadata.supports_tools.is_none() {
                    existing.metadata.supports_tools = entry.metadata.supports_tools.take();
                }
                if existing.metadata.supports_vision.is_none() {
                    existing.metadata.supports_vision = entry.metadata.supports_vision.take();
                }
                if existing.metadata.size_bytes.is_none() {
                    existing.metadata.size_bytes = entry.metadata.size_bytes.take();
                }
                if existing.metadata.performance.is_none() {
                    existing.metadata.performance = entry.metadata.performance.take();
                }
                if existing.metadata.fit.is_none() {
                    existing.metadata.fit = entry.metadata.fit.take();
                }
                continue;
            }
            entry.is_active = active.as_ref().is_some_and(|selection| {
                selection.provider.eq_ignore_ascii_case(&entry.provider)
                    && selection.model.eq_ignore_ascii_case(&entry.model)
            });
            entries.push(entry);
        }

        let mut state = Self {
            visible_entries: entries.clone(),
            entries,
            query: String::new(),
        };
        state.apply_filter();
        state
    }

    pub(super) fn entries(&self) -> &[ModelInventoryEntry] {
        &self.entries
    }

    pub(super) fn visible_entries(&self) -> &[ModelInventoryEntry] {
        &self.visible_entries
    }

    pub(super) fn query(&self) -> &str {
        &self.query
    }

    pub(super) fn set_query(&mut self, query: &str) {
        self.query = query.trim().to_lowercase();
        self.apply_filter();
    }

    pub(super) fn select(&self, index: usize) -> Option<ModelSelection> {
        self.visible_entries.get(index).map(|entry| {
            let mut selection = ModelSelection::new(entry.provider.clone(), entry.model.clone());
            selection.download = entry.download.clone();
            selection.fit = entry.metadata.fit.clone();
            selection
        })
    }

    fn apply_filter(&mut self) {
        self.visible_entries = self
            .entries
            .iter()
            .filter(|entry| {
                self.query.is_empty()
                    || entry.provider.to_lowercase().contains(&self.query)
                    || entry.model.to_lowercase().contains(&self.query)
                    || entry.source.label().contains(&self.query)
                    || entry.availability.label().contains(&self.query)
            })
            .cloned()
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(provider: &str, model: &str, source: ModelSource) -> ModelInventoryEntry {
        ModelInventoryEntry {
            provider: provider.to_string(),
            model: model.to_string(),
            source,
            availability: ModelAvailability::Installed,
            is_active: false,
            is_default: false,
            metadata: ModelMetadata::default(),
            download: None,
        }
    }

    #[test]
    fn filters_provider_model_source_and_availability_without_case() {
        let mut ollama = entry("ollama", "qwen3:8b", ModelSource::Ollama);
        ollama.availability = ModelAvailability::Installed;
        let entries = vec![ollama, entry("openai", "gpt-5", ModelSource::Configured)];
        let mut picker = ModelPickerState::new(entries, None);

        picker.set_query("QWEN");
        assert_eq!(picker.visible_entries().len(), 1);
        picker.set_query("OLLAMA");
        assert_eq!(picker.visible_entries().len(), 1);
        picker.set_query("installed");
        assert_eq!(picker.visible_entries().len(), 2);
        picker.set_query("openai");
        assert_eq!(picker.visible_entries()[0].model, "gpt-5");
    }

    #[test]
    fn removes_duplicate_provider_model_entries_and_merges_flags() {
        let mut configured = entry("ollama", "qwen3:8b", ModelSource::Configured);
        configured.is_default = true;
        let mut installed = entry("ollama", "qwen3:8b", ModelSource::Ollama);
        installed.is_active = true;

        let picker = ModelPickerState::new(vec![configured, installed], None);
        assert_eq!(picker.entries().len(), 1);
        assert!(picker.entries()[0].is_active);
        assert!(picker.entries()[0].is_default);
    }

    #[test]
    fn marks_matching_active_model_and_selects_by_visible_index() {
        let picker = ModelPickerState::new(
            vec![entry("ollama", "qwen3:8b", ModelSource::Ollama)],
            Some(ModelSelection::new("ollama", "qwen3:8b")),
        );

        assert!(picker.visible_entries()[0].is_active);
        assert_eq!(
            picker.select(0),
            Some(ModelSelection::new("ollama", "qwen3:8b"))
        );
        assert_eq!(picker.select(1), None);
    }

    #[test]
    fn empty_query_shows_entries_and_unmatched_query_shows_none() {
        let mut picker =
            ModelPickerState::new(vec![entry("ollama", "qwen3:8b", ModelSource::Ollama)], None);
        assert_eq!(picker.visible_entries().len(), 1);

        picker.set_query("missing");
        assert!(picker.visible_entries().is_empty());
    }

    #[test]
    fn downloadable_selection_preserves_its_backend_and_variant() {
        let mut downloadable = entry("local", "Model · Q4", ModelSource::Catalog);
        downloadable.availability = ModelAvailability::Downloadable;
        downloadable.download = Some(ModelDownloadSpec {
            spec: "org/model-gguf".into(),
            backend_id: "llamacpp".into(),
            variant_id: "Q4_K_M".into(),
        });
        let picker = ModelPickerState::new(vec![downloadable], None);

        let selection = picker.select(0).unwrap();
        assert_eq!(selection.download.unwrap().variant_id, "Q4_K_M");
        assert_eq!(picker.select(1), None);
    }

    #[test]
    fn selection_preserves_model_fit_assessment() {
        let fit =
            super::super::model_fit::assess_model_fit(Some(6_000), Some(10_000), Some(true), false);
        let mut model = entry("local", "example/Q4_K_M", ModelSource::Catalog);
        model.metadata.fit = Some(fit.clone());

        let picker = ModelPickerState::new(vec![model], None);

        assert_eq!(picker.select(0).unwrap().fit, Some(fit));
    }
}

use std::collections::HashMap;
use std::future::Future;

use crate::canonical::maybe_get_canonical_model;
use crate::errors::ProviderError;
use crate::model::DEFAULT_CONTEXT_LIMIT;

#[derive(Debug, Clone, Default)]
pub struct ContextLimitResolver {
    provider_name: String,
    configured_limits: HashMap<String, usize>,
}

impl ContextLimitResolver {
    pub fn new(provider_name: impl Into<String>) -> Self {
        Self {
            provider_name: provider_name.into(),
            configured_limits: HashMap::new(),
        }
    }

    pub fn with_configured_limits(
        mut self,
        configured_limits: impl IntoIterator<Item = (String, usize)>,
    ) -> Self {
        self.configured_limits = configured_limits
            .into_iter()
            .filter(|(_, context_limit)| *context_limit > 0)
            .collect();
        self
    }

    fn configured_limit(&self, model: &str) -> Option<usize> {
        self.configured_limits.get(model).copied().or_else(|| {
            let mut matches = self
                .configured_limits
                .iter()
                .filter(|(configured_model, _)| configured_model.eq_ignore_ascii_case(model));
            let (_, limit) = matches.next()?;
            matches.next().is_none().then_some(*limit)
        })
    }

    pub fn resolve_local(&self, model: &str, override_limit: Option<usize>) -> usize {
        override_limit
            .or_else(|| self.configured_limit(model))
            .or_else(|| {
                maybe_get_canonical_model(&self.provider_name, model)
                    .map(|canonical| canonical.limit.context)
            })
            .unwrap_or(DEFAULT_CONTEXT_LIMIT)
    }

    pub async fn resolve<F, Fut>(
        &self,
        model: &str,
        override_limit: Option<usize>,
        discover: F,
    ) -> usize
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<usize>, ProviderError>>,
    {
        if let Some(limit) = override_limit {
            return limit;
        }

        if let Some(limit) = self.configured_limit(model) {
            return limit;
        }

        match discover().await {
            Ok(Some(limit)) if limit > 0 => return limit,
            Ok(Some(_) | None) => {}
            Err(error) => tracing::warn!(
                provider = self.provider_name,
                model,
                %error,
                "Context-limit discovery failed; falling back"
            ),
        }

        maybe_get_canonical_model(&self.provider_name, model)
            .map(|canonical| canonical.limit.context)
            .unwrap_or(DEFAULT_CONTEXT_LIMIT)
    }

    /// Resolve a limit reported by the provider itself (consumer override,
    /// configured entry, or live discovery). Returns `None` when none apply,
    /// so callers can prefer their own fallback over the canonical/default
    /// chain and reliably tell "the provider reports nothing" apart from a
    /// value that merely matches the global default.
    pub async fn resolve_provider_reported<F, Fut>(
        &self,
        model: &str,
        override_limit: Option<usize>,
        discover: F,
    ) -> Option<usize>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<usize>, ProviderError>>,
    {
        if let Some(limit) = override_limit {
            return Some(limit);
        }
        if let Some(limit) = self.configured_limit(model) {
            return Some(limit);
        }
        match discover().await {
            Ok(Some(limit)) if limit > 0 => Some(limit),
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(
                    provider = self.provider_name,
                    model,
                    %error,
                    "Context-limit discovery failed; no reported limit"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn applies_precedence() {
        let resolver = ContextLimitResolver::new("anthropic")
            .with_configured_limits([("claude-sonnet-4-5".to_string(), 64_000)]);

        assert_eq!(
            resolver
                .resolve("claude-sonnet-4-5", Some(32_000), || async {
                    Ok(Some(16_000))
                })
                .await,
            32_000
        );
        assert_eq!(
            resolver
                .resolve("claude-sonnet-4-5", None, || async { Ok(Some(16_000)) })
                .await,
            64_000
        );
    }

    #[test]
    fn configured_limits_match_case_insensitively() {
        let resolver = ContextLimitResolver::new("unknown-provider")
            .with_configured_limits([("MyModel".to_string(), 64_000)]);

        assert_eq!(resolver.resolve_local("mymodel", None), 64_000);
    }

    #[test]
    fn exact_match_wins_over_case_insensitive_matches() {
        let resolver = ContextLimitResolver::new("unknown-provider").with_configured_limits([
            ("MyModel".to_string(), 64_000),
            ("mymodel".to_string(), 32_000),
        ]);

        assert_eq!(resolver.resolve_local("MyModel", None), 64_000);
        assert_eq!(resolver.resolve_local("mymodel", None), 32_000);
        assert_eq!(
            resolver.resolve_local("MYMODEL", None),
            DEFAULT_CONTEXT_LIMIT
        );
    }

    #[test]
    fn ignores_zero_configured_limits() {
        let resolver = ContextLimitResolver::new("unknown-provider")
            .with_configured_limits([("configured".to_string(), 0)]);

        assert_eq!(
            resolver.resolve_local("configured", None),
            DEFAULT_CONTEXT_LIMIT
        );
    }

    #[test]
    fn local_resolution_skips_discovery() {
        let resolver = ContextLimitResolver::new("anthropic")
            .with_configured_limits([("configured".to_string(), 64_000)]);

        assert_eq!(resolver.resolve_local("configured", None), 64_000);
        assert_eq!(resolver.resolve_local("claude-sonnet-4-5", None), 1_000_000);
        assert_eq!(
            resolver.resolve_local("unknown", None),
            DEFAULT_CONTEXT_LIMIT
        );
    }

    #[tokio::test]
    async fn ignores_zero_discovered_limits() {
        let resolver = ContextLimitResolver::new("unknown-provider");

        assert_eq!(
            resolver
                .resolve("unknown-model", None, || async { Ok(Some(0)) })
                .await,
            DEFAULT_CONTEXT_LIMIT
        );
    }

    #[tokio::test]
    async fn uses_discovery_then_canonical_then_default() {
        let resolver = ContextLimitResolver::new("anthropic");
        assert_eq!(
            resolver
                .resolve("runtime-model", None, || async { Ok(Some(24_000)) })
                .await,
            24_000
        );
        assert_eq!(
            resolver
                .resolve("claude-sonnet-4-5", None, || async { Ok(None) })
                .await,
            1_000_000
        );
        assert_eq!(
            resolver
                .resolve("unknown-model", None, || async { Ok(None) })
                .await,
            DEFAULT_CONTEXT_LIMIT
        );
    }

    #[tokio::test]
    async fn reported_resolution_preserves_discovered_default_value() {
        let resolver = ContextLimitResolver::new("custom_lemonade");

        assert_eq!(
            resolver
                .resolve_provider_reported("gpt-5", None, || async {
                    Ok(Some(DEFAULT_CONTEXT_LIMIT))
                })
                .await,
            Some(DEFAULT_CONTEXT_LIMIT)
        );
        assert_eq!(
            resolver
                .resolve_provider_reported("gpt-5", None, || async { Ok(None) })
                .await,
            None
        );
        assert_eq!(
            resolver
                .resolve_provider_reported("gpt-5", None, || async { Ok(Some(0)) })
                .await,
            None
        );
        assert_eq!(
            resolver
                .resolve_provider_reported("gpt-5", Some(32_000), || async { Ok(None) })
                .await,
            Some(32_000)
        );
    }

    #[tokio::test]
    async fn reported_resolution_ignores_canonical_and_configured_fallbacks() {
        let resolver = ContextLimitResolver::new("anthropic")
            .with_configured_limits([("configured".to_string(), 64_000)]);

        assert_eq!(
            resolver
                .resolve_provider_reported("claude-sonnet-4-5", None, || async { Ok(None) })
                .await,
            None
        );
        assert_eq!(
            resolver
                .resolve_provider_reported("configured", None, || async { Ok(None) })
                .await,
            Some(64_000)
        );
        assert_eq!(
            resolver
                .resolve_provider_reported("unknown", None, || async { Ok(None) })
                .await,
            None
        );
    }
}

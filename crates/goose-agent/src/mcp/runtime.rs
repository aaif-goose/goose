use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, OnceCell};

/// Single-flight lazy startup. Failure is cached until the host replaces the slot.
pub struct Runtime<T> {
    value: OnceCell<T>,
}
impl<T> Default for Runtime<T> {
    fn default() -> Self {
        Self {
            value: OnceCell::new(),
        }
    }
}
impl<T> Runtime<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self) -> Option<&T> {
        self.value.get()
    }
    pub async fn get_or_init<F, Fut>(&self, init: F) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.value.get_or_init(init).await
    }
}

/// Scope-owned slots and starts that have not yet committed to the selected set.
/// Kept together so a concurrent resolve can reuse an in-progress enable.
pub struct Scopes<T> {
    pub selected: HashMap<String, indexmap::IndexMap<String, Arc<T>>>,
    pub enabling: HashMap<(String, String), Arc<T>>,
}
impl<T> Default for Scopes<T> {
    fn default() -> Self {
        Self {
            selected: HashMap::new(),
            enabling: HashMap::new(),
        }
    }
}

/// Scope ownership for `mcp::Manager`. GDK never persists the selection or inspects host
/// configuration.
pub struct Manager<T> {
    pub scopes: Mutex<Scopes<T>>,
}
impl<T> Manager<T> {
    pub fn new() -> Self {
        Self {
            scopes: Mutex::new(Scopes::default()),
        }
    }
    pub async fn release(&self, scope: &str) {
        let mut scopes = self.scopes.lock().await;
        scopes.selected.remove(scope);
        scopes.enabling.retain(|(id, _), _| id != scope);
    }
}

impl<T> Default for Manager<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Scopes<T> {
    /// Preserve compatible selected or in-flight slots while replacing a scope selection.
    /// Compatibility may inspect host policy, but ownership and ordering stay in the runtime.
    pub async fn reconcile<I, K, C, F, Fut>(
        &mut self,
        scope: &str,
        inputs: I,
        key: K,
        compatible: C,
        create: F,
    ) -> Vec<Arc<T>>
    where
        I: IntoIterator,
        K: Fn(&I::Item) -> String,
        C: Fn(Arc<T>, &I::Item) -> Fut,
        Fut: std::future::Future<Output = bool>,
        F: Fn(&I::Item) -> Arc<T>,
    {
        let mut current = self.selected.get(scope).cloned().unwrap_or_default();
        let mut selected = indexmap::IndexMap::new();
        for input in inputs {
            let key = key(&input);
            let pending = self
                .enabling
                .get(&(scope.to_string(), key.clone()))
                .cloned();
            let mut reusable = None;
            for slot in current.swap_remove(&key).into_iter().chain(pending) {
                if compatible(Arc::clone(&slot), &input).await {
                    reusable = Some(slot);
                    break;
                }
            }
            selected.insert(key, reusable.unwrap_or_else(|| create(&input)));
        }
        let slots = selected.values().cloned().collect();
        self.selected.insert(scope.to_string(), selected);
        slots
    }
}

/// A pinned scope selection held by a lease.
pub struct Selection<T> {
    pub scope: String,
    pub slots: Vec<Arc<T>>,
}
impl<T> Selection<T> {
    pub fn new(scope: impl Into<String>, slots: Vec<Arc<T>>) -> Self {
        Self {
            scope: scope.into(),
            slots,
        }
    }
}

impl<T> Clone for Selection<T> {
    fn clone(&self) -> Self {
        Self {
            scope: self.scope.clone(),
            slots: self.slots.clone(),
        }
    }
}

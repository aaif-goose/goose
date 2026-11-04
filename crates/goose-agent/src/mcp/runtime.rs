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
    pub fn ready(value: T) -> Self {
        Self {
            value: OnceCell::new_with(Some(value)),
        }
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

/// Generic ownership primitive for hosts with their own durable selection policy.
/// GDK never persists the selection or inspects host configuration.
pub struct ScopedRuntime<T> {
    pub scopes: Mutex<Scopes<T>>,
}
impl<T> ScopedRuntime<T> {
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

impl<T> Default for ScopedRuntime<T> {
    fn default() -> Self {
        Self::new()
    }
}

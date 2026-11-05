//! Relation values that track whether they have been loaded.

use std::fmt;
use std::ops::{Deref, DerefMut};

/// A relation value that may not have been loaded yet.
#[derive(Default)]
pub struct Deferred<T> {
    state: DeferredState<T>,
}

#[derive(Default)]
enum DeferredState<T> {
    #[default]
    Unloaded,
    Loaded(Box<T>),
}

impl<T> Deferred<T> {
    /// Creates an unloaded relation value.
    pub const fn unloaded() -> Self {
        Self {
            state: DeferredState::Unloaded,
        }
    }

    /// Creates a loaded relation value.
    pub fn loaded(value: T) -> Self {
        Self {
            state: DeferredState::Loaded(Box::new(value)),
        }
    }

    /// Returns whether a value has been loaded.
    pub fn is_loaded(&self) -> bool {
        matches!(self.state, DeferredState::Loaded(_))
    }

    /// Returns the loaded value.
    pub fn get(&self) -> Option<&T> {
        match &self.state {
            DeferredState::Unloaded => None,
            DeferredState::Loaded(value) => Some(value),
        }
    }

    /// Returns the loaded value mutably.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        match &mut self.state {
            DeferredState::Unloaded => None,
            DeferredState::Loaded(value) => Some(value),
        }
    }

    /// Marks the value as loaded.
    pub fn set(&mut self, value: T) {
        self.state = DeferredState::Loaded(Box::new(value));
    }

    /// Marks the value as unloaded.
    pub fn clear(&mut self) {
        self.state = DeferredState::Unloaded;
    }

    /// Returns the loaded value, panicking when unloaded.
    pub fn expect_loaded(&self) -> &T {
        self.get()
            .expect("relation is not loaded; use `.include(...)` or execute a relation query first")
    }

    /// Returns the loaded value mutably, panicking when unloaded.
    pub fn expect_loaded_mut(&mut self) -> &mut T {
        self.get_mut()
            .expect("relation is not loaded; use `.include(...)` or execute a relation query first")
    }
}

impl<T: Clone> Clone for Deferred<T> {
    fn clone(&self) -> Self {
        match &self.state {
            DeferredState::Unloaded => Self::unloaded(),
            DeferredState::Loaded(value) => Self::loaded((**value).clone()),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Deferred<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.state {
            DeferredState::Unloaded => f.write_str("Deferred::Unloaded"),
            DeferredState::Loaded(value) => f.debug_tuple("Deferred::Loaded").field(value).finish(),
        }
    }
}

impl<T> Deref for Deferred<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.expect_loaded()
    }
}

impl<T> DerefMut for Deferred<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.expect_loaded_mut()
    }
}

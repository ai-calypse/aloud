//! An in-memory cache for responses that cost a network round-trip: credential
//! scoping, single-flight revalidation, compare-and-set.
//!
//! Every method takes the lock itself and returns owned data — a caller holding
//! the lock across a compare and a later write lets a slow response overwrite a
//! newer one.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

/// Distinct search queries remembered before the map is dropped wholesale.
///
// ponytail: clear-all rather than LRU eviction — a session rarely reaches 32
// distinct searches, and the cost of being wrong is one refetch. Swap in an
// LRU if search ever becomes a primary navigation path.
const MAX_SEARCHES: usize = 32;

pub struct ResponseCache<T> {
    inner: Mutex<Inner<T>>,
}

struct Inner<T> {
    /// Which credential these entries were fetched with; a change drops the
    /// whole cache rather than serving one account's data to another. Hashed
    /// because a cache has no business keeping a second copy of a secret.
    credential: u64,
    primary: Option<Vec<T>>,
    searches: HashMap<String, Vec<T>>,
    /// Without this, every window focus starts another unbounded fan-out.
    revalidating: bool,
}

impl<T> Default for ResponseCache<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                credential: 0,
                primary: None,
                searches: HashMap::new(),
                revalidating: false,
            }),
        }
    }
}

fn hash(credential: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    credential.hash(&mut hasher);
    hasher.finish()
}

impl<T: Clone + PartialEq> ResponseCache<T> {
    /// The cached primary list, or None when cold or fetched under another key.
    pub fn primary(&self, credential: &str) -> Option<Vec<T>> {
        let mut inner = self.inner.lock().ok()?;
        inner.scope_to(hash(credential));
        inner.primary.clone()
    }

    pub fn store_primary(&self, credential: &str, value: Vec<T>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.scope_to(hash(credential));
            inner.primary = Some(value);
        }
    }

    pub fn search(&self, credential: &str, query: &str) -> Option<Vec<T>> {
        let mut inner = self.inner.lock().ok()?;
        inner.scope_to(hash(credential));
        inner.searches.get(query).cloned()
    }

    pub fn store_search(&self, credential: &str, query: String, value: Vec<T>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.scope_to(hash(credential));
            if inner.searches.len() >= MAX_SEARCHES {
                inner.searches.clear();
            }
            inner.searches.insert(query, value);
        }
    }

    /// Claims the right to revalidate, or returns false if someone already has
    /// it. The caller must reach [`Self::finish_revalidation`] on every path,
    /// including failure, or no further revalidation can start.
    pub fn begin_revalidation(&self) -> bool {
        match self.inner.lock() {
            Ok(mut inner) if !inner.revalidating => {
                inner.revalidating = true;
                true
            }
            _ => false,
        }
    }

    /// Releases the claim and stores `fresh` if it differs, returning whether to
    /// push. Compare and store under one lock, so a slower revalidation cannot
    /// land on a newer one. `None` (fetch failed) leaves the entry alone —
    /// showing the last known list beats blanking the pane.
    pub fn finish_revalidation(&self, credential: &str, fresh: Option<Vec<T>>) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        inner.revalidating = false;

        let Some(fresh) = fresh else {
            return false;
        };
        inner.scope_to(hash(credential));
        if inner.primary.as_ref() == Some(&fresh) {
            return false;
        }
        inner.primary = Some(fresh);
        true
    }

    /// Drops everything: disconnecting a service invalidates the connected list
    /// *and* any search result that mentioned it.
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.primary = None;
            inner.searches.clear();
        }
    }
}

impl<T> Inner<T> {
    /// Empties the cache if these entries belong to a different credential.
    fn scope_to(&mut self, credential: u64) {
        if self.credential == credential {
            return;
        }
        self.credential = credential;
        self.primary = None;
        self.searches.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> ResponseCache<String> {
        ResponseCache::default()
    }

    #[test]
    fn serves_what_was_stored() {
        let c = cache();
        c.store_primary("key-a", vec!["gmail".into()]);
        assert_eq!(c.primary("key-a"), Some(vec!["gmail".into()]));
    }

    /// Entries fetched with one Composio key must never reach a session
    /// authenticated as someone else.
    #[test]
    fn a_different_credential_sees_nothing() {
        let c = cache();
        c.store_primary("key-a", vec!["gmail".into()]);
        c.store_search("key-a", "slack".into(), vec!["slack".into()]);

        assert_eq!(c.primary("key-b"), None);
        assert_eq!(c.search("key-b", "slack"), None);
        // ...and switching back does not resurrect the old entries.
        assert_eq!(c.primary("key-a"), None);
    }

    /// Every window focus asks for one; unguarded they overlap.
    #[test]
    fn only_one_revalidation_runs_at_a_time() {
        let c = cache();
        assert!(c.begin_revalidation());
        assert!(!c.begin_revalidation());

        c.finish_revalidation("key-a", None);
        assert!(c.begin_revalidation(), "claim must be released on failure");
    }

    #[test]
    fn pushes_only_when_the_value_changed() {
        let c = cache();
        c.store_primary("key-a", vec!["gmail".into()]);

        assert!(!c.finish_revalidation("key-a", Some(vec!["gmail".into()])));
        assert!(c.finish_revalidation("key-a", Some(vec!["gmail".into(), "slack".into()])));
        assert_eq!(
            c.primary("key-a"),
            Some(vec!["gmail".into(), "slack".into()])
        );
    }

    /// A failed revalidation must not blank a pane that is showing good data.
    #[test]
    fn a_failed_revalidation_keeps_the_previous_value() {
        let c = cache();
        c.store_primary("key-a", vec!["gmail".into()]);
        assert!(!c.finish_revalidation("key-a", None));
        assert_eq!(c.primary("key-a"), Some(vec!["gmail".into()]));
    }

    #[test]
    fn clear_drops_searches_too() {
        let c = cache();
        c.store_primary("key-a", vec!["gmail".into()]);
        c.store_search("key-a", "gmail".into(), vec!["gmail".into()]);

        c.clear();
        assert_eq!(c.primary("key-a"), None);
        assert_eq!(c.search("key-a", "gmail"), None);
    }

    #[test]
    fn search_map_stays_bounded() {
        let c = cache();
        for i in 0..MAX_SEARCHES + 5 {
            c.store_search("key-a", format!("query-{i}"), vec![i.to_string()]);
        }
        let held = c.inner.lock().unwrap().searches.len();
        assert!(held <= MAX_SEARCHES, "unbounded growth: {held} entries");
    }
}

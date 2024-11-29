//! Thread-safe key/value cache.

use std::collections::hash_map::{Entry, HashMap};
use std::hash::Hash;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

/// Cache that remembers the result for each key.
#[derive(Debug)]
pub struct Cache<K, V> {
    // todo! This is an example cache type. Build your own cache type that satisfies the
    // specification for `get_or_insert_with`.
    inner: RwLock<HashMap<K, Arc<Mutex<Option<V>>>>>,
}

impl<K, V> Default for Cache<K, V> {
    fn default() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Cache<K, V> {
    /// Retrieve the value or insert a new one created by `f`.
    ///
    /// An invocation to this function should not block another invocation with a different key. For
    /// example, if a thread calls `get_or_insert_with(key1, f1)` and another thread calls
    /// `get_or_insert_with(key2, f2)` (`key1≠key2`, `key1,key2∉cache`) concurrently, `f1` and `f2`
    /// should run concurrently.
    ///
    /// On the other hand, since `f` may consume a lot of resource (= money), it's undesirable to
    /// duplicate the work. That is, `f` should be run only once for each key. Specifically, even
    /// for concurrent invocations of `get_or_insert_with(key, f)`, `f` is called only once per key.
    ///
    /// Hint: the [`Entry`] API may be useful in implementing this function.
    ///
    /// [`Entry`]: https://doc.rust-lang.org/stable/std/collections/hash_map/struct.HashMap.html#method.entry
    pub fn get_or_insert_with<F: FnOnce(K) -> V>(&self, key: K, f: F) -> V {
        let current_thread_id = thread::current().id();
        println!("thread_id: {:?} acquiring read lock", current_thread_id);
        let inner_read = self.inner.read().unwrap();
        if let Some(value) = inner_read.get(&key) {
            let vc = value.clone();
            drop(inner_read);
            let v = vc.lock().unwrap();
            if let Some(vv) = v.as_ref() {
                println!("thread_id: {:?} dropping read lock", current_thread_id);
                return vv.clone();
            }
        }
        else {
            drop(inner_read);
        }
        println!("thread_id: {:?} dropping read lock", current_thread_id);
        println!("thread_id: {:?} acquiring write lock", current_thread_id);
        let mut inner_write = self.inner.write().unwrap();
        if let Entry::Occupied(entry) = inner_write.entry(key.clone()) {
            let value_lock = entry.get().clone();
            let mut vl_guard = value_lock.lock().unwrap();
            if let Some(vv) = vl_guard.as_ref() {
                println!("thread_id: {:?} dropping write lock", current_thread_id);
                return vv.clone();
            }
        }
        let value_lock = Arc::new(Mutex::new(None));
        inner_write.insert(key.clone(), Arc::clone(&value_lock));
        let mut vl_guard = value_lock.lock().unwrap();
        drop(inner_write);
        println!("thread_id: {:?} dropping write lock", current_thread_id);
        let value = f(key.clone());
        *vl_guard = Some(value.clone());
        value
    }
}

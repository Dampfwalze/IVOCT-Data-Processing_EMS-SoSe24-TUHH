use core::fmt;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::Hash,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak},
};

#[derive(Clone)]
pub struct Cache(Arc<_Shared>);

pub struct Cached<T> {
    cache: Arc<_Shared>,
    entry: _CacheEntry,
    _phantom: std::marker::PhantomData<T>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey(usize);

#[derive(Clone)]
struct _CacheEntry(Arc<dyn Any + Sync + Send>);
struct _WeakCacheEntry(Weak<dyn Any + Sync + Send>);

struct _Shared(RwLock<HashMap<(CacheKey, TypeId), _WeakCacheEntry>>);

// MARK: Cache

impl Cache {
    pub fn new() -> Self {
        Self(Arc::new(_Shared(RwLock::new(HashMap::new()))))
    }

    pub fn get<T: Default + Send + Sync + 'static>(&self, key: impl Hash) -> Cached<T> {
        self.get_or_insert_with(key, T::default)
    }

    pub fn get_or_insert_with<T: Send + Sync + 'static>(
        &self,
        key: impl Hash,
        f: impl FnOnce() -> T,
    ) -> Cached<T> {
        let key = CacheKey::new(key);
        Cached {
            cache: self.0.clone(),
            entry: self.0.get_cache_or_insert_with(key, f),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let caches = self.0 .0.read().unwrap();
        f.debug_struct("Cache")
            .field(
                "Strong count",
                &caches.values().filter(|v| v.upgrade().is_some()).count(),
            )
            .field(
                "Weak count",
                &caches.values().filter(|v| v.upgrade().is_none()).count(),
            )
            .field("keys", &caches.keys().collect::<Vec<_>>())
            .finish()
    }
}

// MARK: Cached<T>

impl<T: Send + Sync + 'static> Cached<T> {
    pub fn read<'a>(&'a self) -> RwLockReadGuard<'a, T> {
        self.entry
            .downcast_ref::<T>()
            .expect("Entry should be of type T")
            .read()
            .unwrap()
    }

    pub fn write<'a>(&'a self) -> RwLockWriteGuard<'a, T> {
        self.entry
            .downcast_ref::<T>()
            .expect("Entry should be of type T")
            .write()
            .unwrap()
    }

    pub fn change_target(&mut self, key: impl Hash)
    where
        T: Default,
    {
        self.change_target_or_insert(key, T::default);
    }

    pub fn change_target_or_insert(&mut self, key: impl Hash, f: impl FnOnce() -> T) {
        let key = CacheKey::new(key);
        self.entry = self.cache.get_cache_or_insert_with(key, f);
    }
}

impl<T> Clone for Cached<T> {
    fn clone(&self) -> Self {
        Self {
            cache: self.cache.clone(),
            entry: self.entry.clone(),
            _phantom: std::marker::PhantomData,
        }
    }
}

// MARK: _Shared

impl _Shared {
    fn get_cache_or_insert_with<T: Send + Sync + 'static>(
        &self,
        key: CacheKey,
        f: impl FnOnce() -> T,
    ) -> _CacheEntry {
        if let Some(entry) = self.0.read().unwrap().get(&(key, TypeId::of::<T>())) {
            if let Some(entry) = entry.upgrade() {
                return entry;
            }
        }

        let entry = _CacheEntry::new(f());
        self.0
            .write()
            .unwrap()
            .insert((key, TypeId::of::<T>()), entry.weak());
        entry
    }
}

// MARK: CacheKey

impl CacheKey {
    pub fn new(key: impl Hash) -> Self {
        use std::hash::{DefaultHasher, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        Self(hasher.finish() as usize)
    }
}

// MARK: _CacheEntry

impl _CacheEntry {
    fn new<T: Send + Sync + 'static>(value: T) -> Self {
        Self(Arc::new(RwLock::new(value)))
    }

    fn downcast_ref<T: 'static>(&self) -> Option<&RwLock<T>> {
        self.0.downcast_ref()
    }

    fn weak(&self) -> _WeakCacheEntry {
        _WeakCacheEntry(Arc::downgrade(&self.0))
    }
}

impl _WeakCacheEntry {
    fn upgrade(&self) -> Option<_CacheEntry> {
        self.0.upgrade().map(_CacheEntry)
    }
}

// MARK: Tests

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_cache() {
        let cache = Cache::new();

        let key = CacheKey(0);

        let cached1 = cache.get::<u32>(key);
        let cached2 = cache.get_or_insert_with::<u64>(key, || 10);

        assert_eq!(*cached1.read(), 0);
        assert_eq!(*cached2.read(), 10);

        *cached1.write() = 1;

        assert_eq!(*cached1.read(), 1);
        assert_eq!(*cached2.read(), 10);

        let cached = cache.get::<u32>(key);

        assert_eq!(*cached.read(), 1);
        assert_eq!(*cached2.read(), 10);
    }

    #[test]
    fn test_cache_2() {
        let cache = Cache::new();

        let mut cached1 = cache.get_or_insert_with(123, || 0);
        let cached2 = cache.get_or_insert_with(123, || 1);

        assert_eq!(*cached1.read(), 0);
        assert_eq!(*cached2.read(), 0);

        *cached1.write() = 2;

        assert_eq!(*cached1.read(), 2);
        assert_eq!(*cached2.read(), 2);

        cached1.change_target(456);

        assert_eq!(*cached1.read(), 0);
        assert_eq!(*cached2.read(), 2);

        *cached2.write() = 3;

        assert_eq!(*cached1.read(), 0);
        assert_eq!(*cached2.read(), 3);

        drop(cached1);
        drop(cached2);

        assert!(cache
            .0
             .0
            .read()
            .unwrap()
            .values()
            .all(|entry| entry.upgrade().is_none()));
    }
}

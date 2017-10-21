//! Single-valued TTL cache

use std::time::{Duration, Instant};

/// A single-valued cache which the content will expire after a predefined duration.
pub struct Cache<T> {
    /// The instant which the cached value expires.
    expire: Instant,
    /// The stored value.
    value: Option<T>,
}

impl<T> Cache<T> {
    /// Creates a new empty cache.
    pub fn new() -> Self {
        Self {
            expire: Instant::now(),
            value: None,
        }
    }

    /// Retrieves the cached value.
    ///
    /// If the value has expired, `refresh()` will be called to obtain a new value, which will be
    /// valid for `duration` until its expiration again.
    pub fn get_or_refresh<E, F>(&mut self, duration: Duration, refresh: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if Instant::now() < self.expire {
            if let Some(ref value) = self.value {
                return Ok(value);
            }
        }

        self.value = Some(refresh()?);
        self.expire = Instant::now() + duration;
        match self.value {
            Some(ref value) => Ok(value),
            None => unreachable!(),
        }
    }
}

#[test]
fn test_get_or_refresh() {
    use std::thread::sleep;

    let mut cache = Cache::<u64>::new();
    let mut counter = 0;

    // Initially the cache was empty, so we should be able to insert 45 in it.
    {
        let result = cache.get_or_refresh(Duration::from_millis(500), || -> Result<u64, u64> {
            counter |= 1;
            Ok(45)
        });
        assert_eq!(result, Ok(&45));
        assert_eq!(counter, 1);
    }

    // Now the cache has a valid data, we should need to call the refresh function.
    {
        let result = cache.get_or_refresh(Duration::from_millis(500), || -> Result<u64, u64> {
            counter |= 2;
            Ok(75)
        });
        assert_eq!(result, Ok(&45));
        assert_eq!(counter, 1);
    }

    sleep(Duration::from_millis(600));

    // The cache should now be expired. The refresh function returns error though.
    {
        let result = cache.get_or_refresh(Duration::from_millis(500), || -> Result<u64, u64> {
            counter |= 4;
            Err(17)
        });
        assert_eq!(result, Err(17));
        assert_eq!(counter, 5);
    }

    // This time the refresh function returns Ok, so 49 can be inserted.
    {
        let result = cache.get_or_refresh(Duration::from_millis(500), || -> Result<u64, u64> {
            counter |= 8;
            Ok(49)
        });
        assert_eq!(result, Ok(&49));
        assert_eq!(counter, 13);
    }

    // Again the cache has valid data, no need to call the refresh function.
    {
        let result = cache.get_or_refresh(Duration::from_millis(500), || -> Result<u64, u64> {
            counter |= 16;
            Err(62)
        });
        assert_eq!(result, Ok(&49));
        assert_eq!(counter, 13);
    }

    sleep(Duration::from_millis(600));

    // Expired again, insert 31 this time.
    {
        let result = cache.get_or_refresh(Duration::from_millis(500), || -> Result<u64, u64> {
            counter |= 32;
            Ok(31)
        });
        assert_eq!(result, Ok(&31));
        assert_eq!(counter, 45);
    }
}

use std::{
    any::Any,
    error::Error,
    fmt,
    ops::{self, Deref},
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use zenoh_result::ZResult;

/// Zenoh configuration.
///
/// Most options are optional as a way to keep defaults flexible. Some of the options have different
/// default values depending on the rest of the configuration.
///
/// To construct a configuration, we advise that you use a configuration file (JSON, JSON5 and YAML
/// are currently supported, please use the proper extension for your format as the deserializer
/// will be picked according to it).
#[derive(Default, Debug, Clone)]
pub struct Config(pub(crate) zenoh_config::Config);

impl Config {
    pub fn from_env() -> ZResult<Self> {
        Ok(Config(zenoh_config::Config::from_env()?))
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> ZResult<Self> {
        Ok(Config(zenoh_config::Config::from_file(path)?))
    }

    pub fn insert_json5(&mut self, key: &str, value: &str) -> Result<(), InsertionError> {
        <zenoh_config::Config as validated_struct::ValidatedMap>::insert_json5(
            &mut self.0,
            key,
            value,
        )
        .map_err(InsertionError)
    }

    pub fn get<'a>(&'a self, key: &str) -> Result<&'a dyn Any, LookupError> {
        <zenoh_config::Config as validated_struct::ValidatedMap>::get(&self.0, key)
            .map_err(LookupError)
    }

    pub fn remove<K: AsRef<str>>(&mut self, key: K) -> ZResult<()> {
        self.0.remove(key)
    }
}

#[derive(Debug)]
pub struct InsertionError(validated_struct::InsertionError);

impl fmt::Display for InsertionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for InsertionError {}

#[derive(Debug)]
pub struct LookupError(validated_struct::GetError);

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for LookupError {}

#[zenoh_macros::unstable_config]
impl std::ops::Deref for Config {
    type Target = zenoh_config::Config;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[zenoh_macros::unstable_config]
impl std::ops::DerefMut for Config {
    fn deref_mut(&mut self) -> &mut <Self as std::ops::Deref>::Target {
        &mut self.0
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

pub type Notification = Arc<str>;

struct NotifierInner<T> {
    inner: Mutex<T>,
    subscribers: Mutex<Vec<flume::Sender<Notification>>>,
}
pub struct Notifier<T> {
    inner: Arc<NotifierInner<T>>,
}
impl<T> Clone for Notifier<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl Notifier<Config> {
    pub fn new(inner: Config) -> Self {
        Notifier {
            inner: Arc::new(NotifierInner {
                inner: Mutex::new(inner),
                subscribers: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn subscribe(&self) -> flume::Receiver<Notification> {
        let (tx, rx) = flume::unbounded();
        self.lock_subscribers().push(tx);
        rx
    }

    pub fn notify<K: AsRef<str>>(&self, key: K) {
        let key = key.as_ref();
        let key: Arc<str> = Arc::from(key);
        let mut marked = Vec::new();
        let mut subscribers = self.lock_subscribers();

        for (i, sub) in subscribers.iter().enumerate() {
            if sub.send(key.clone()).is_err() {
                marked.push(i)
            }
        }

        for i in marked.into_iter().rev() {
            subscribers.swap_remove(i);
        }
    }

    pub fn lock(&self) -> MutexGuard<Config> {
        self.lock_config()
    }

    fn lock_subscribers(&self) -> MutexGuard<Vec<flume::Sender<Notification>>> {
        self.inner
            .subscribers
            .lock()
            .expect("acquiring Notifier's subscribers Mutex should not fail")
    }

    fn lock_config(&self) -> MutexGuard<Config> {
        self.inner
            .inner
            .lock()
            .expect("acquiring Notifier's Config Mutex should not fail")
    }

    pub fn remove<K: AsRef<str>>(&self, key: K) -> ZResult<()> {
        self.lock_config().remove(key.as_ref())?;
        self.notify(key);
        Ok(())
    }

    pub fn insert_json5(&self, key: &str, value: &str) -> Result<(), InsertionError> {
        self.lock_config().insert_json5(key, value)
    }

    pub fn get<'a>(&'a self, key: &str) -> Result<LookupGuard<'a, Config>, LookupError> {
        let config = self.lock_config();
        // SAFETY: MutexGuard pins the mutex behind which the value is held.
        let subref = config.get(key.as_ref())? as *const _;
        Ok(LookupGuard {
            _guard: config,
            subref,
        })
    }
}

pub struct LookupGuard<'a, T> {
    _guard: MutexGuard<'a, T>,
    subref: *const dyn Any,
}

impl<'a, T> ops::Deref for LookupGuard<'a, T> {
    type Target = dyn Any;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.subref }
    }
}

impl<'a, T> AsRef<dyn Any> for LookupGuard<'a, T> {
    fn as_ref(&self) -> &dyn Any {
        self.deref()
    }
}

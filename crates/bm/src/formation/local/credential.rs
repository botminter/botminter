use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::formation::KeyValueCredentialStore;

#[cfg(target_os = "linux")]
use std::collections::HashMap;

fn load_tracked_keys(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(path)?;
    let keys: Vec<String> = serde_json::from_str(&data)?;
    Ok(keys)
}

fn save_tracked_keys(path: &Path, keys: &[String]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(keys)?;
    std::fs::write(path, data)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn connect_secret_service() -> Result<dbus_secret_service::SecretService> {
    dbus_secret_service::SecretService::connect(dbus_secret_service::EncryptionType::Plain).map_err(
        |e| {
            anyhow::anyhow!(
                "Cannot connect to Secret Service (D-Bus). \
                 Install a Secret Service provider (e.g., gnome-keyring) \
                 or use environment variable overrides instead. ({})",
                e
            )
        },
    )
}

#[cfg(target_os = "linux")]
fn get_or_create_collection<'a>(
    ss: &'a dbus_secret_service::SecretService,
    name: &str,
) -> Result<dbus_secret_service::Collection<'a>> {
    if let Ok(collections) = ss.get_all_collections() {
        for c in collections {
            if let Ok(label) = c.get_label() {
                if label == name {
                    return Ok(c);
                }
            }
        }
    }

    ss.create_collection(name, "")
        .map_err(|e| anyhow::anyhow!("Failed to create keyring collection '{}': {}", name, e))
}

#[cfg(target_os = "linux")]
fn dss_store(service: &str, key: &str, value: &str, collection_name: &str) -> Result<()> {
    let ss = connect_secret_service()?;
    let collection = get_or_create_collection(&ss, collection_name)?;
    collection
        .ensure_unlocked()
        .map_err(|e| anyhow::anyhow!("Failed to unlock collection '{}': {}", collection_name, e))?;

    let mut attrs = HashMap::new();
    attrs.insert("service", service);
    attrs.insert("username", key);

    collection
        .create_item(
            &format!("{} — {}", service, key),
            attrs,
            value.as_bytes(),
            true,
            "text/plain",
        )
        .map_err(|e| anyhow::anyhow!("Failed to store credential: {}", e))?;

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn dss_store(_service: &str, _key: &str, _value: &str, collection_name: &str) -> Result<()> {
    anyhow::bail!(
        "Custom keyring collections are only supported on Linux (requested '{}')",
        collection_name
    )
}

#[cfg(target_os = "linux")]
fn dss_retrieve(service: &str, key: &str, collection_name: &str) -> Result<Option<String>> {
    let ss = connect_secret_service()?;
    let collection = match get_or_create_collection(&ss, collection_name) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    if collection.is_locked().unwrap_or(true) {
        return Ok(None);
    }

    let mut attrs = HashMap::new();
    attrs.insert("service", service);
    attrs.insert("username", key);

    let items = collection
        .search_items(attrs)
        .map_err(|e| anyhow::anyhow!("Failed to search keyring: {}", e))?;

    if let Some(item) = items.first() {
        let secret = item
            .get_secret()
            .map_err(|e| anyhow::anyhow!("Failed to read secret: {}", e))?;
        Ok(Some(String::from_utf8_lossy(&secret).to_string()))
    } else {
        Ok(None)
    }
}

#[cfg(not(target_os = "linux"))]
fn dss_retrieve(_service: &str, _key: &str, _collection_name: &str) -> Result<Option<String>> {
    Ok(None)
}

#[cfg(target_os = "linux")]
fn dss_delete(service: &str, key: &str, collection_name: &str) -> Result<()> {
    let ss = connect_secret_service()?;
    let collection = match get_or_create_collection(&ss, collection_name) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let mut attrs = HashMap::new();
    attrs.insert("service", service);
    attrs.insert("username", key);

    if let Ok(items) = collection.search_items(attrs) {
        for item in items {
            let _ = item.delete();
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn dss_delete(_service: &str, _key: &str, _collection_name: &str) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn check_keyring_unlocked_for(collection_name: Option<&str>) -> Result<()> {
    let ss = connect_secret_service()?;

    let collection = if let Some(name) = collection_name {
        get_or_create_collection(&ss, name)?
    } else {
        ss.get_default_collection().map_err(|e| {
            anyhow::anyhow!(
                "No default keyring collection found. \
                 Run `seahorse` or `gnome-keyring-daemon` to create one. ({})",
                e
            )
        })?
    };

    let locked = collection
        .is_locked()
        .map_err(|e| anyhow::anyhow!("Cannot check keyring lock state: {}", e))?;

    if locked {
        anyhow::bail!(
            "System keyring is locked. Unlock it before storing credentials.\n\
             On GNOME: the keyring unlocks automatically on login.\n\
             On headless systems: run `gnome-keyring-daemon --unlock` or \
             use environment variable overrides instead."
        );
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn check_keyring_unlocked_for(collection_name: Option<&str>) -> Result<()> {
    if collection_name.is_some() {
        anyhow::bail!("Custom keyring collections are only supported on Linux")
    }
    Ok(())
}

pub struct LocalKeyValueCredentialStore {
    service: String,
    keys_path: PathBuf,
    collection: Option<String>,
}

impl LocalKeyValueCredentialStore {
    pub fn new(service: String, keys_path: PathBuf) -> Self {
        Self {
            service,
            keys_path,
            collection: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_collection(mut self, collection: Option<String>) -> Self {
        self.collection = collection;
        self
    }

    fn with_keyring_dbus<T, F: FnOnce() -> T>(&self, f: F) -> T {
        if let Ok(dbus) = std::env::var("BM_KEYRING_DBUS") {
            let original = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &dbus);
            let result = f();
            match original {
                Some(v) => std::env::set_var("DBUS_SESSION_BUS_ADDRESS", v),
                None => std::env::remove_var("DBUS_SESSION_BUS_ADDRESS"),
            }
            result
        } else {
            f()
        }
    }

    fn track_key(&self, key: &str) -> Result<()> {
        let mut keys = load_tracked_keys(&self.keys_path)?;
        if !keys.contains(&key.to_string()) {
            keys.push(key.to_string());
            keys.sort();
            save_tracked_keys(&self.keys_path, &keys)?;
        }
        Ok(())
    }

    fn untrack_key(&self, key: &str) -> Result<()> {
        let mut keys = load_tracked_keys(&self.keys_path)?;
        keys.retain(|k| k != key);
        save_tracked_keys(&self.keys_path, &keys)?;
        Ok(())
    }
}

impl KeyValueCredentialStore for LocalKeyValueCredentialStore {
    fn store(&self, key: &str, value: &str) -> Result<()> {
        self.with_keyring_dbus(|| {
            if let Some(ref collection) = self.collection {
                dss_store(&self.service, key, value, collection)?;
            } else {
                check_keyring_unlocked_for(None)?;
                let entry = keyring::Entry::new(&self.service, key).map_err(|e| {
                    anyhow::anyhow!("Failed to create keyring entry for key '{}'. ({})", key, e)
                })?;
                entry.set_password(value).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to store credential in system keyring for key '{}'. ({})",
                        key,
                        e
                    )
                })?;
            }

            self.track_key(key)?;
            Ok(())
        })
    }

    fn retrieve(&self, key: &str) -> Result<Option<String>> {
        self.with_keyring_dbus(|| {
            if let Some(ref collection) = self.collection {
                return dss_retrieve(&self.service, key, collection);
            }

            let entry = match keyring::Entry::new(&self.service, key) {
                Ok(entry) => entry,
                Err(_) => return Ok(None),
            };
            match entry.get_password() {
                Ok(password) => Ok(Some(password)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Ok(None),
            }
        })
    }

    fn remove(&self, key: &str) -> Result<()> {
        self.with_keyring_dbus(|| {
            if let Some(ref collection) = self.collection {
                dss_delete(&self.service, key, collection)?;
            } else if let Ok(entry) = keyring::Entry::new(&self.service, key) {
                match entry.delete_credential() {
                    Ok(()) => {}
                    Err(keyring::Error::NoEntry) => {}
                    Err(_) => {}
                }
            }

            self.untrack_key(key)?;
            Ok(())
        })
    }

    fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        let keys = load_tracked_keys(&self.keys_path)?;
        let mut filtered: Vec<String> =
            keys.into_iter().filter(|k| k.starts_with(prefix)).collect();
        filtered.sort();
        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_and_list_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("keys.json");

        save_tracked_keys(
            &keys_path,
            &["a/1".to_string(), "a/2".to_string(), "b/1".to_string()],
        )
        .unwrap();
        let keys = load_tracked_keys(&keys_path).unwrap();
        assert_eq!(keys, vec!["a/1", "a/2", "b/1"]);
    }

    #[test]
    fn load_tracked_keys_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("nonexistent.json");
        let keys = load_tracked_keys(&keys_path).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn save_tracked_keys_creates_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("nested").join("dir").join("keys.json");
        save_tracked_keys(&keys_path, &["key1".to_string()]).unwrap();
        let keys = load_tracked_keys(&keys_path).unwrap();
        assert_eq!(keys, vec!["key1"]);
    }

    #[test]
    fn track_key_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("keys.json");
        let store =
            LocalKeyValueCredentialStore::new("test-service".to_string(), keys_path.clone());

        store.track_key("my-key").unwrap();
        store.track_key("my-key").unwrap();
        let keys = load_tracked_keys(&keys_path).unwrap();
        assert_eq!(keys, vec!["my-key"]);
    }

    #[test]
    fn untrack_key_removes_from_file() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("keys.json");
        save_tracked_keys(
            &keys_path,
            &["a".to_string(), "b".to_string(), "c".to_string()],
        )
        .unwrap();

        let store =
            LocalKeyValueCredentialStore::new("test-service".to_string(), keys_path.clone());

        store.untrack_key("b").unwrap();
        let keys = load_tracked_keys(&keys_path).unwrap();
        assert_eq!(keys, vec!["a", "c"]);
    }

    #[test]
    fn untrack_key_nonexistent_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("keys.json");
        save_tracked_keys(&keys_path, &["a".to_string()]).unwrap();

        let store =
            LocalKeyValueCredentialStore::new("test-service".to_string(), keys_path.clone());

        store.untrack_key("nonexistent").unwrap();
        let keys = load_tracked_keys(&keys_path).unwrap();
        assert_eq!(keys, vec!["a"]);
    }

    #[test]
    fn list_keys_filters_by_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("keys.json");
        save_tracked_keys(
            &keys_path,
            &[
                "batman/app-id".to_string(),
                "superman/app-id".to_string(),
                "superman/private-key".to_string(),
            ],
        )
        .unwrap();

        let store = LocalKeyValueCredentialStore::new("test-service".to_string(), keys_path);

        let keys = store.list_keys("superman/").unwrap();
        assert_eq!(keys, vec!["superman/app-id", "superman/private-key"]);
    }

    #[test]
    fn list_keys_empty_prefix_returns_all() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("keys.json");
        save_tracked_keys(&keys_path, &["a".to_string(), "b".to_string()]).unwrap();

        let store = LocalKeyValueCredentialStore::new("test-service".to_string(), keys_path);

        let keys = store.list_keys("").unwrap();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    fn list_keys_no_match_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("keys.json");
        save_tracked_keys(&keys_path, &["superman/id".to_string()]).unwrap();

        let store = LocalKeyValueCredentialStore::new("test-service".to_string(), keys_path);

        let keys = store.list_keys("batman/").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn list_keys_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let keys_path = tmp.path().join("nonexistent.json");

        let store = LocalKeyValueCredentialStore::new("test-service".to_string(), keys_path);

        let keys = store.list_keys("").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn store_constructor_and_with_collection() {
        let store = LocalKeyValueCredentialStore::new(
            "botminter.team.matrix".to_string(),
            PathBuf::from("/tmp/keys.json"),
        );
        assert_eq!(store.service, "botminter.team.matrix");
        assert!(store.collection.is_none());

        let store = store.with_collection(Some("my-collection".to_string()));
        assert_eq!(store.collection, Some("my-collection".to_string()));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn custom_collection_store_fails_clearly_off_linux() {
        let tmp = tempfile::tempdir().unwrap();
        let store = LocalKeyValueCredentialStore::new(
            "botminter.team.github-app".to_string(),
            tmp.path().join("keys.json"),
        )
        .with_collection(Some("custom".to_string()));

        let err = store.store("member/app-id", "123").unwrap_err();
        assert!(err
            .to_string()
            .contains("Custom keyring collections are only supported on Linux"));
    }
}

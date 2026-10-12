//! Database passwords and web link keys, kept outside the `.tenun` file.
//!
//! Stored like `~/.pgpass`: one JSON file in the user's config folder, readable
//! only by the user (mode 0600 on Unix). The key is the connection itself
//! (`kind://user@host:port/db`), so two workspaces that use the same database
//! share one entry and a shared `.tenun` file never carries a password.

use crate::workspace::{DbConn, KeyPlace};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct Secrets {
    path: PathBuf,
}

impl Secrets {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The file the secrets are kept in. Tenun's other files of this computer sit beside it.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The default location: `<config dir>/tenun/secrets.json`.
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                if cfg!(windows) {
                    std::env::var_os("APPDATA").map(PathBuf::from)
                } else if cfg!(target_os = "macos") {
                    std::env::var_os("HOME")
                        .map(|h| PathBuf::from(h).join("Library/Application Support"))
                } else {
                    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
                }
            })
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("tenun").join("secrets.json")
    }

    pub fn key(kind: &str, c: &DbConn) -> String {
        format!(
            "{kind}://{}@{}:{}/{}",
            c.user,
            c.host,
            c.port.map(|p| p.to_string()).unwrap_or_default(),
            c.database
        )
    }

    /// The entry of a web link's key: the host it goes to, where it is sent and its name. Two
    /// links to one host that use the same key share one entry.
    pub fn link_key(host: &str, place: KeyPlace, name: &str) -> String {
        format!(
            "link://{}/{}/{name}",
            host.to_ascii_lowercase(),
            place.label()
        )
    }

    fn read(&self) -> BTreeMap<String, String> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.read().get(key).cloned()
    }

    pub fn set(&self, key: &str, password: &str) -> anyhow::Result<()> {
        let mut all = self.read();
        all.insert(key.to_string(), password.to_string());
        self.write(&all)
    }

    /// Deletes an entry. Nothing happens when there is none.
    pub fn remove(&self, key: &str) -> anyhow::Result<()> {
        let mut all = self.read();
        if all.remove(key).is_some() {
            self.write(&all)?;
        }
        Ok(())
    }

    fn write(&self, all: &BTreeMap<String, String>) -> anyhow::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(all)?;
        write_private(&self.path, text.as_bytes())
    }
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    // Opened without truncating: a file that exists with a wider mode is made private before
    // anything is written to it. `mode` applies only when the file is created here.
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.set_len(0)?;
    f.write_all(bytes)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    // On Windows the file sits in the user's own %APPDATA%, which other users cannot read.
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_private_mode() {
        let dir = std::env::temp_dir().join(format!("tenun-secrets-{}", std::process::id()));
        let s = Secrets::new(dir.join("secrets.json"));
        let c = DbConn {
            host: "h".into(),
            port: Some(5432),
            database: "d".into(),
            user: "u".into(),
            schema: None,
        };
        let k = Secrets::key("postgres", &c);
        assert_eq!(k, "postgres://u@h:5432/d");
        s.set(&k, "pw").unwrap();
        assert_eq!(s.get(&k).as_deref(), Some("pw"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("secrets.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        s.remove(&k).unwrap();
        assert_eq!(s.get(&k), None);
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_exists_with_a_wider_mode_is_made_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("tenun-secrets-wide-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("secrets.json");
        std::fs::write(&file, "{}").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let s = Secrets::new(file.clone());
        s.set("link://h/header/X", "v").unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(s.get("link://h/header/X").as_deref(), Some("v"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_link_key_is_named_by_host_place_and_name() {
        assert_eq!(
            Secrets::link_key("API.Example.com", KeyPlace::Header, "Authorization"),
            "link://api.example.com/header/Authorization"
        );
        assert_eq!(
            Secrets::link_key("h", KeyPlace::Query, "api_key"),
            "link://h/query/api_key"
        );
    }
}

#[cfg(test)]
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
pub fn read_json_config(path: &Path) -> io::Result<Value> {
    let mut value: Value = match read_optional(path)? {
        Some(text) => serde_json::from_str(&text).map_err(io::Error::other)?,
        None => json!({}),
    };
    let root = value.as_object_mut().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "Config root must be an object")
    })?;
    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "mcpServers must be an object",
        ));
    }
    Ok(value)
}

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(windows)]
fn replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(source: *const u16, target: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // Same-directory replacement; never remove the destination first.
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 0x1 | 0x8) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

pub fn write_config_atomic(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{}.{}.tmp", std::process::id(), id));
    let temporary = Temporary(PathBuf::from(name));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary.0)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);
    match fs::metadata(path) {
        Ok(meta) => {
            let mut backup = path.as_os_str().to_os_string();
            backup.push(format!(".{}.{}.bak", std::process::id(), id));
            let backup = PathBuf::from(backup);
            // Keep each backup; abort if backing up the existing file fails.
            let original = fs::read(path)?;
            let mut backup_file = options.open(&backup)?;
            backup_file.write_all(&original)?;
            backup_file.sync_all()?;
            fs::set_permissions(&temporary.0, meta.permissions())?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    replace(&temporary.0, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "mcp-safe-{}-{}.json",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }
    #[test]
    fn invalid_configs_are_rejected_without_changes() {
        for text in [
            "{broken",
            "[]",
            "null",
            r#"{"mcpServers":null}"#,
            r#"{"mcpServers":[]}"#,
        ] {
            let p = path();
            fs::write(&p, text).unwrap();
            assert!(read_json_config(&p).is_err());
            assert_eq!(fs::read_to_string(&p).unwrap(), text);
            fs::remove_file(p).unwrap();
        }
    }
    #[test]
    fn replacement_preserves_settings_and_backs_up_original() {
        let p = path();
        let original = r#"{"theme":"dark","mcpServers":{"old":{"command":"old"}}}"#;
        fs::write(&p, original).unwrap();
        let mut config = read_json_config(&p).unwrap();
        config["mcpServers"]["new"] = json!({"command":"new"});
        write_config_atomic(&p, &serde_json::to_string(&config).unwrap()).unwrap();
        let updated = read_json_config(&p).unwrap();
        assert_eq!(updated["theme"], "dark");
        assert_eq!(updated["mcpServers"]["old"]["command"], "old");
        let prefix = format!("{}.", p.file_name().unwrap().to_string_lossy());
        let backups: Vec<_> = fs::read_dir(p.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with(&prefix)
                    && e.path().extension().is_some_and(|x| x == "bak")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), original);
        fs::remove_file(backups[0].path()).unwrap();
        fs::remove_file(p).unwrap();
    }
    #[test]
    fn missing_config_is_created_but_read_errors_are_not_empty_configs() {
        let p = path();
        assert!(read_json_config(&p).unwrap()["mcpServers"].is_object());
        fs::create_dir(&p).unwrap();
        assert!(read_json_config(&p).is_err());
        fs::remove_dir(p).unwrap();
    }
}

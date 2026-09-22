//! Reading and writing `~/.claude/settings.json`.
//!
//! The file belongs to Claude Code, not to Summoner: it holds the user's
//! permissions, environment and their own hooks. Summoner only adds and
//! removes its own entries, so every edit here is a read-modify-write of
//! someone else's file, and the two failure modes that would lose its
//! contents are handled in one place.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

const BACKUP_SUFFIX: &str = ".summoner-bak";
const TEMP_SUFFIX: &str = ".summoner-tmp";

/// Path of the user's Claude Code settings file.
pub fn path() -> io::Result<PathBuf> {
    Ok(dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home dir"))?
        .join(".claude")
        .join("settings.json"))
}

/// Read the settings file. A missing file reads as empty settings.
///
/// A file that exists but does not parse as a JSON object is an error rather
/// than an empty map: Summoner writes back what it read, so treating an
/// unreadable file as empty would replace the user's settings with Summoner's
/// entries alone.
pub fn read(settings_path: &Path) -> io::Result<Map<String, Value>> {
    if !settings_path.exists() {
        return Ok(Map::new());
    }
    let content = fs::read_to_string(settings_path)?;
    let value: Value = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not valid JSON ({e}); left untouched", settings_path.display()),
        )
    })?;
    match value {
        Value::Object(map) => Ok(map),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} holds a JSON {}, not an object; left untouched",
                settings_path.display(),
                type_name(&other)
            ),
        )),
    }
}

/// Write the settings file, replacing it in a single rename so an interrupted
/// write leaves the original in place rather than a truncated file. The first
/// time Summoner edits a file it keeps the original alongside it as
/// `settings.json.summoner-bak`.
pub fn write(settings_path: &Path, settings: &Map<String, Value>) -> io::Result<()> {
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if settings_path.exists() {
        let backup = sibling(settings_path, BACKUP_SUFFIX)?;
        if !backup.exists() {
            fs::copy(settings_path, &backup)?;
        }
    }

    let content = serde_json::to_string_pretty(&Value::Object(settings.clone()))
        .map_err(io::Error::other)?;
    let temp = sibling(settings_path, TEMP_SUFFIX)?;
    fs::write(&temp, content)?;
    #[cfg(unix)]
    if let Ok(meta) = fs::metadata(settings_path) {
        let _ = fs::set_permissions(&temp, meta.permissions());
    }
    if let Err(e) = fs::rename(&temp, settings_path) {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    Ok(())
}

fn sibling(settings_path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let name = settings_path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "settings path has no file name")
    })?;
    let mut name = name.to_os_string();
    name.push(suffix);
    Ok(settings_path.with_file_name(name))
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

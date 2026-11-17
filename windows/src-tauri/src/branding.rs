// Display/voice identity is independent of the stable application identifier and hook paths.
use std::sync::{OnceLock, RwLock};
static NAME: OnceLock<RwLock<String>> = OnceLock::new();
pub fn clean(raw: &str) -> String {
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.chars().count() > 32 || !text.chars().any(char::is_alphanumeric)
        || !text.chars().all(|c| c.is_alphanumeric() || " ._-".contains(c)) { "Coucou".into() } else { text }
}
pub fn name() -> String { NAME.get_or_init(|| RwLock::new("Coucou".into())).read().unwrap().clone() }
pub fn set(raw: &str) { *NAME.get_or_init(|| RwLock::new("Coucou".into())).write().unwrap() = clean(raw); }
pub fn text(raw: String) -> String { raw.replace("Coucou", &name()) }
pub fn assistant() -> String { let n = name(); if n == "Coucou" { "Mochi".into() } else { n } }

#[cfg(windows)]
fn alias_path() -> std::path::PathBuf {
    let normalized = crate::voice::wake::normalize(&name());
    let slug = normalized.split_whitespace().collect::<Vec<_>>().join("-");
    let reserved = matches!(slug.as_str(), "con" | "prn" | "aux" | "nul") || (slug.len() == 4 && (slug.starts_with("com") || slug.starts_with("lpt")) && slug.ends_with(|c: char| c.is_ascii_digit()));
    let slug = if reserved { format!("app-{slug}") } else { slug };
    // A prefix for reserved names avoids Windows device names such as CON and NUL.
    crate::platform::config_dir().join("aliases").join(format!("{slug}.cmd"))
}
pub fn write_alias() -> Result<Option<String>, String> {
    #[cfg(windows)] {
        let path = alias_path();
        let parent = path.parent().ok_or("Invalid alias path")?;
        crate::platform::ensure_private_dir(parent).map_err(|e| e.to_string())?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().replace('%', "%%");
        std::fs::write(&path, format!("@echo off\r\nstart \"\" \"{exe}\" %*\r\n")).map_err(|e| e.to_string())?;
        return Ok(Some(path.to_string_lossy().into_owned()));
    }
    #[cfg(not(windows))] { Ok(None) }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn names_are_bounded_and_never_paths_or_shell_expressions() {
        assert_eq!(clean("  Jarvis  Azul "), "Jarvis Azul");
        for invalid in ["../Jarvis", "x&calc", "x%PATH%", "---", ""] { assert_eq!(clean(invalid), "Coucou"); }
        assert_eq!(clean(&"a".repeat(33)), "Coucou");
    }
}

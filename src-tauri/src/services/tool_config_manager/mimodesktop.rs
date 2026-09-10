//! Xiaomi MiMo Desktop beta: shared providers, separate Electron preferences.
//! Verified against desktop 26.909.91205; restart to reload the renderer/engine.

use super::{model_input_modalities_for, write_json_file, ApplyResult, ModelInfo};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const PROVIDER: &str = "echobird-desktop";
const CONFIG_NAMES: [&str; 3] = ["mimocode.jsonc", "mimocode.json", "config.json"];

fn config_dir() -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("MIMOCODE_HOME").filter(|v| !v.is_empty()) {
        let home = PathBuf::from(home);
        if !home.is_absolute() {
            return Err("MIMOCODE_HOME must be an absolute path".into());
        }
        return Ok(home.join("config"));
    }
    Ok(std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
        .join("mimocode"))
}

fn preferences_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_default()
        .join("Xiaomi MiMo")
        .join("preferences.json")
}

fn load_object(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read Xiaomi MiMo config {}: {e}", path.display()))?;
    // Match the desktop's JSONC syntax without silently repairing invalid JSON.
    let options = jsonc_parser::ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    };
    jsonc_parser::parse_to_serde_value::<Value>(&content, &options)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| format!("Cannot parse Xiaomi MiMo config: {}", path.display()))
}

fn apply_at(dir: &Path, prefs_path: &Path, info: &ModelInfo) -> Result<(), String> {
    let model = info.model.as_deref().unwrap_or("");
    if model.trim().is_empty() {
        return Err("Model ID is empty, cannot apply config".into());
    }
    let path = CONFIG_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())
        .unwrap_or_else(|| dir.join(CONFIG_NAMES[0]));
    // Validate both files before writing either; never replace malformed user data.
    let mut config = load_object(&path)?;
    let mut prefs = load_object(prefs_path)?;
    if let Some(provider) = config.get("provider") {
        if !provider.is_object() {
            return Err("Xiaomi MiMo provider config must be an object".into());
        }
    } else {
        config["provider"] = json!({});
    }
    if config.get("$schema").is_none() {
        config["$schema"] = json!("https://mimo.xiaomi.com/mimocode/config.json");
    }
    config["provider"][PROVIDER] = json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": "EchoBird Desktop",
        "options": {
            "baseURL": info.base_url.as_deref().unwrap_or("https://api.openai.com/v1").trim_end_matches('/'),
            "apiKey": info.api_key.as_deref().unwrap_or("")
        },
        "models": {
            model: {
                "name": info.name.as_deref().unwrap_or(model),
                "modalities": {"input": model_input_modalities_for(model), "output": ["text"]}
            }
        }
    });
    // Do not change model/small_model here: those also control the MiMo Code CLI.
    prefs["model"] = json!(format!("{PROVIDER}/{model}"));
    write_json_file(&path, &config)?;
    write_json_file(prefs_path, &prefs)
}

pub(super) fn apply(info: &ModelInfo) -> ApplyResult {
    match config_dir().and_then(|dir| apply_at(&dir, &preferences_path(), info)) {
        Ok(()) => ApplyResult {
            success: true,
            message: "Xiaomi MiMo default model configured. Fully quit and reopen the desktop app; existing tasks may keep their own model.".into(),
        },
        Err(message) => ApplyResult { success: false, message },
    }
}

fn read_at(dir: &Path, prefs_path: &Path) -> Option<ModelInfo> {
    fn merge(target: &mut Value, source: Value) {
        match (target, source) {
            (Value::Object(target), Value::Object(source)) => {
                for (key, value) in source {
                    merge(target.entry(key).or_insert(Value::Null), value);
                }
            }
            (target, source) => *target = source,
        }
    }

    let prefs = load_object(prefs_path).ok()?;
    let (provider_id, model) = prefs.get("model")?.as_str()?.split_once('/')?;
    // The engine merges global config.json -> mimocode.json -> mimocode.jsonc.
    // A higher-priority file may override options without repeating the models.
    let mut provider = json!({});
    for name in CONFIG_NAMES.iter().rev() {
        let config = load_object(&dir.join(name)).ok()?;
        if let Some(entry) = config.get("provider").and_then(|p| p.get(provider_id)) {
            merge(&mut provider, entry.clone());
        }
    }
    let entry = provider.get("models")?.get(model)?;
    serde_json::from_value(json!({
        "model": model,
        "name": entry.get("name").and_then(Value::as_str).unwrap_or(model),
        "baseUrl": provider.pointer("/options/baseURL"),
        "apiKey": provider.pointer("/options/apiKey"),
        "protocol": if provider.get("npm").and_then(Value::as_str) == Some("@ai-sdk/anthropic") { "anthropic" } else { "openai" }
    })).ok()
}

pub(super) fn read() -> Option<ModelInfo> {
    read_at(&config_dir().ok()?, &preferences_path())
}

fn restore_at(dir: &Path, prefs_path: &Path) -> Result<(), String> {
    let mut prefs = load_object(prefs_path)?;
    let mut configs = Vec::new();
    for name in CONFIG_NAMES {
        let path = dir.join(name);
        if path.exists() {
            configs.push((path.clone(), load_object(&path)?));
        }
    }
    // Clear only our selection before removing its provider. Other preferences,
    // custom providers and the CLI's selectors remain intact.
    if prefs
        .get("model")
        .and_then(Value::as_str)
        .is_some_and(|m| m.starts_with(&format!("{PROVIDER}/")))
    {
        prefs["model"] = json!("");
        write_json_file(prefs_path, &prefs)?;
    }
    for (path, mut config) in configs {
        if config
            .get_mut("provider")
            .and_then(Value::as_object_mut)
            .and_then(|p| p.remove(PROVIDER))
            .is_some()
        {
            write_json_file(&path, &config)?;
        }
    }
    Ok(())
}

pub(super) fn restore() -> ApplyResult {
    match config_dir().and_then(|dir| restore_at(&dir, &preferences_path())) {
        Ok(()) => ApplyResult {
            success: true,
            message: "Xiaomi MiMo desktop provider removed. Fully quit and reopen the desktop app."
                .into(),
        },
        Err(message) => ApplyResult {
            success: false,
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("echobird-mimodesktop-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn prefs(&self) -> PathBuf {
            self.0.join("desktop/preferences.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn model(id: &str) -> ModelInfo {
        serde_json::from_value(json!({"model": id, "name": "Test model", "baseUrl": "http://localhost:1234/v1/", "apiKey": "test-only"})).unwrap()
    }

    #[test]
    fn apply_and_switch_preserve_cli_and_desktop_settings() {
        let f = Fixture::new();
        let path = f.0.join("mimocode.jsonc");
        let original = json!({"model": "echobird/cli-model", "small_model": "personal/small", "provider": {"echobird": {"models": {"cli-model": {}}}, "personal": {"options": {"apiKey": "keep"}}}});
        write_json_file(&path, &original).unwrap();
        write_json_file(
            &f.prefs(),
            &json!({"model": "personal/old", "theme": "dark", "permByConvo": {"session": "ask"}}),
        )
        .unwrap();
        apply_at(&f.0, &f.prefs(), &model("vendor/model-one")).unwrap();
        assert_eq!(
            read_at(&f.0, &f.prefs()).unwrap().model.as_deref(),
            Some("vendor/model-one")
        );
        apply_at(&f.0, &f.prefs(), &model("model-two")).unwrap();
        let config = load_object(&path).unwrap();
        assert_eq!(config["model"], original["model"]);
        assert_eq!(config["small_model"], original["small_model"]);
        assert_eq!(
            config["provider"]["echobird"],
            original["provider"]["echobird"]
        );
        assert_eq!(
            config["provider"]["personal"],
            original["provider"]["personal"]
        );
        assert_eq!(
            config["provider"][PROVIDER]["models"]
                .as_object()
                .unwrap()
                .len(),
            1
        );
        let selected = read_at(&f.0, &f.prefs()).unwrap();
        assert_eq!(selected.model.as_deref(), Some("model-two"));
        assert_eq!(
            selected.base_url.as_deref(),
            Some("http://localhost:1234/v1")
        );
        let prefs = load_object(&f.prefs()).unwrap();
        assert_eq!(prefs["theme"], "dark");
        assert_eq!(prefs["permByConvo"]["session"], "ask");
        restore_at(&f.0, &f.prefs()).unwrap();
        let restored = load_object(&path).unwrap();
        assert_eq!(restored["provider"], original["provider"]);
        assert_eq!(restored["model"], original["model"]);
        assert_eq!(load_object(&f.prefs()).unwrap()["model"], "");
    }

    #[test]
    fn reads_manual_desktop_selection_without_global_model() {
        let f = Fixture::new();
        write_json_file(&f.0.join("mimocode.jsonc"), &json!({"provider": {"relay": {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": "https://example.com/v1", "apiKey": "test-only"}, "models": {"vendor/model": {"name": "Manual"}}}}})).unwrap();
        write_json_file(&f.prefs(), &json!({"model": "relay/vendor/model"})).unwrap();
        assert_eq!(
            read_at(&f.0, &f.prefs()).unwrap().name.as_deref(),
            Some("Manual")
        );
        restore_at(&f.0, &f.prefs()).unwrap();
        assert_eq!(
            load_object(&f.prefs()).unwrap()["model"],
            "relay/vendor/model"
        );
    }

    #[test]
    fn reads_provider_merged_across_global_configs() {
        let f = Fixture::new();
        write_json_file(&f.0.join("config.json"), &json!({"provider": {"relay": {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": "https://old.example/v1", "apiKey": "test-only"}}}})).unwrap();
        write_json_file(
            &f.0.join("mimocode.json"),
            &json!({"provider": {"relay": {"models": {"vendor/model": {"name": "Manual"}}}}}),
        )
        .unwrap();
        write_json_file(&f.0.join("mimocode.jsonc"), &json!({"provider": {"relay": {"options": {"baseURL": "https://override.example/v1"}}}})).unwrap();
        write_json_file(&f.prefs(), &json!({"model": "relay/vendor/model"})).unwrap();

        let selected = read_at(&f.0, &f.prefs()).unwrap();
        assert_eq!(selected.model.as_deref(), Some("vendor/model"));
        assert_eq!(selected.name.as_deref(), Some("Manual"));
        assert_eq!(
            selected.base_url.as_deref(),
            Some("https://override.example/v1")
        );
        assert_eq!(selected.api_key.as_deref(), Some("test-only"));
    }

    #[test]
    fn writes_existing_json_and_prefers_jsonc() {
        let f = Fixture::new();
        let json_path = f.0.join("mimocode.json");
        write_json_file(&json_path, &json!({})).unwrap();
        apply_at(&f.0, &f.prefs(), &model("one")).unwrap();
        assert!(!f.0.join("mimocode.jsonc").exists());
        let previous = fs::read(&json_path).unwrap();
        fs::write(
            f.0.join("mimocode.jsonc"),
            "{// comment\n\"theme\":\"keep\",}",
        )
        .unwrap();
        apply_at(&f.0, &f.prefs(), &model("two")).unwrap();
        assert_eq!(fs::read(&json_path).unwrap(), previous);
        assert_eq!(
            read_at(&f.0, &f.prefs()).unwrap().model.as_deref(),
            Some("two")
        );
        restore_at(&f.0, &f.prefs()).unwrap();
        for name in ["mimocode.json", "mimocode.jsonc"] {
            assert!(load_object(&f.0.join(name)).unwrap()["provider"]
                .get(PROVIDER)
                .is_none());
        }
    }

    #[test]
    fn invalid_files_and_empty_model_do_not_overwrite_settings() {
        for (config, prefs) in [
            ("{broken", "{}"),
            ("{}", "[]"),
            ("{\"provider\":42}", "{}"),
            ("{}", "{broken"),
            ("{\"a\":1 \"b\":2}", "{}"),
            ("{\"list\":[,]}", "{}"),
        ] {
            let f = Fixture::new();
            fs::create_dir_all(f.prefs().parent().unwrap()).unwrap();
            fs::write(f.0.join("mimocode.jsonc"), config).unwrap();
            fs::write(f.prefs(), prefs).unwrap();
            assert!(apply_at(&f.0, &f.prefs(), &model("one")).is_err());
            assert_eq!(
                fs::read_to_string(f.0.join("mimocode.jsonc")).unwrap(),
                config
            );
            assert_eq!(fs::read_to_string(f.prefs()).unwrap(), prefs);
        }
        let f = Fixture::new();
        assert!(apply_at(&f.0, &f.prefs(), &model(" ")).is_err());
        assert!(!f.prefs().exists());
        restore_at(&f.0, &f.prefs()).unwrap();
        assert!(!f.prefs().exists());
    }
}

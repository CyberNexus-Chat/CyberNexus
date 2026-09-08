//! OMP uses models.yml providers and config.yml modelRoles, independently of Pi.
//! Schema: https://github.com/can1357/oh-my-pi/blob/main/docs/models.md

use super::{write_yaml_file, yaml_get, yaml_map, yaml_str, ApplyResult, ModelInfo};
use serde_yaml_ng::{Mapping, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn agent_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".omp/agent")
}

fn has_legacy_db_settings(dir: &Path) -> Result<bool, String> {
    let path = dir.join("agent.db");
    if !path.exists() {
        return Ok(false);
    }
    let inspect = || -> rusqlite::Result<bool> {
        let db = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let has_table: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'settings')",
            [],
            |row| row.get(0),
        )?;
        if !has_table {
            return Ok(false);
        }
        // Both legacy schemas (JSON blob and per-key rows) need OMP's migration.
        db.query_row("SELECT EXISTS(SELECT 1 FROM settings)", [], |row| {
            row.get(0)
        })
    };
    inspect().map_err(|e| {
        format!(
            "Failed to inspect OMP legacy settings in {}: {e}",
            path.display()
        )
    })
}

// OMP prefers .yml over .yaml. Do not create a .yml that masks an existing
// .yaml, or bypass OMP's migration of legacy JSON settings and credentials.
fn load_config(dir: &Path, stem: &str) -> Result<(PathBuf, Value), String> {
    let yml = dir.join(format!("{stem}.yml"));
    let yaml = dir.join(format!("{stem}.yaml"));
    let path = if yml.exists() || !yaml.exists() {
        yml
    } else {
        yaml
    };
    if !path.exists() {
        let legacy = if stem == "config" { "settings" } else { stem };
        if dir.join(format!("{legacy}.json")).exists()
            || (stem == "config" && has_legacy_db_settings(dir)?)
        {
            return Err("Start OMP once to migrate its legacy config, then retry.".into());
        }
        return Ok((path, yaml_map()));
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let value: Value = serde_yaml_ng::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;
    if value.is_null() {
        return Ok((path, yaml_map()));
    }
    if !value.is_mapping() {
        return Err(format!("OMP config must be a mapping: {}", path.display()));
    }
    Ok((path, value))
}

fn child_map<'a>(config: &'a mut Value, key: &str) -> Result<&'a mut Mapping, String> {
    config
        .as_mapping_mut()
        .expect("load_config validates the root")
        .entry(yaml_str(key))
        .or_insert_with(yaml_map)
        .as_mapping_mut()
        .ok_or_else(|| format!("OMP {key} must be a mapping"))
}

pub(super) fn apply(model: &ModelInfo) -> ApplyResult {
    let result = apply_at(&agent_dir(), model);
    ApplyResult {
        success: result.is_ok(),
        message: result
            .map(|()| "Model configured for Oh My Pi. Restart omp to apply.".to_string())
            .unwrap_or_else(|e| e),
    }
}

fn apply_at(dir: &Path, model: &ModelInfo) -> Result<(), String> {
    let model_id = model
        .model
        .as_deref()
        .or(model.name.as_deref())
        .filter(|id| !id.trim().is_empty())
        .ok_or("Model ID is empty")?;
    let is_anthropic = model.protocol.as_deref() == Some("anthropic");
    let endpoint = if is_anthropic {
        model.anthropic_url.as_deref().or(model.base_url.as_deref())
    } else {
        model.base_url.as_deref()
    }
    .map(str::trim)
    .filter(|url| !url.is_empty())
    .ok_or("Base URL is empty")?
    .trim_end_matches('/');
    let api = if is_anthropic {
        "anthropic-messages"
    } else {
        "openai-completions"
    };
    let api_key = model.api_key.as_deref().unwrap_or("");
    let mut provider = serde_json::json!({
        "baseUrl": endpoint,
        "api": api,
        "models": [{"id": model_id, "name": model.name.as_deref().unwrap_or(model_id)}]
    });
    if api_key.is_empty() {
        provider["auth"] = "none".into();
    } else {
        provider["apiKey"] = api_key.into();
    }

    // Parse both files before writing either; invalid user YAML is never replaced.
    let (models_path, mut models) = load_config(dir, "models")?;
    let (config_path, mut config) = load_config(dir, "config")?;
    child_map(&mut models, "providers")?.insert(
        yaml_str("echobird"),
        serde_yaml_ng::to_value(provider).map_err(|e| e.to_string())?,
    );
    let selector = format!("echobird/{model_id}");
    let roles = child_map(&mut config, "modelRoles")?;
    // Replacing our provider removes its old models. Retarget roles that would
    // otherwise dangle, retaining their optional OMP thinking-level suffix.
    for value in roles.values_mut() {
        if let Some(old) = value.as_str().filter(|v| v.starts_with("echobird/")) {
            let thinking = old
                .rsplit_once(':')
                .map(|(_, level)| level)
                .filter(|level| {
                    matches!(
                        *level,
                        "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                    )
                });
            *value = yaml_str(
                &thinking.map_or_else(|| selector.clone(), |level| format!("{selector}:{level}")),
            );
        }
    }
    roles.insert(yaml_str("default"), yaml_str(&selector));
    write_yaml_file(&models_path, &models)?;
    write_yaml_file(&config_path, &config)
}

pub(super) fn read() -> Option<ModelInfo> {
    read_at(&agent_dir())
}

fn read_at(dir: &Path) -> Option<ModelInfo> {
    let (_, config) = load_config(dir, "config").ok()?;
    let selector = yaml_get(yaml_get(&config, "modelRoles")?, "default")?.as_str()?;
    let model_id = selector
        .strip_prefix("echobird/")
        .filter(|id| !id.is_empty())?;
    let (_, models) = load_config(dir, "models").ok()?;
    let provider = yaml_get(yaml_get(&models, "providers")?, "echobird")?;
    let model = yaml_get(provider, "models")?
        .as_sequence()?
        .iter()
        .find(|entry| yaml_get(entry, "id").and_then(Value::as_str) == Some(model_id))?;
    let is_anthropic = yaml_get(provider, "api")?.as_str()? == "anthropic-messages";
    let endpoint = yaml_get(provider, "baseUrl")?.as_str()?.to_string();
    Some(ModelInfo {
        name: Some(
            yaml_get(model, "name")
                .and_then(Value::as_str)
                .unwrap_or(model_id)
                .into(),
        ),
        model: Some(model_id.into()),
        base_url: Some(endpoint.clone()),
        anthropic_url: is_anthropic.then_some(endpoint),
        api_key: yaml_get(provider, "apiKey")
            .and_then(Value::as_str)
            .map(String::from),
        protocol: Some(if is_anthropic { "anthropic" } else { "openai" }.into()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

pub(super) fn restore() -> ApplyResult {
    let result = restore_at(&agent_dir());
    ApplyResult {
        success: result.is_ok(),
        message: result
            .map(|()| {
                "Oh My Pi restored — EchoBird's provider and model role selections removed."
                    .to_string()
            })
            .unwrap_or_else(|e| e),
    }
}

fn restore_at(dir: &Path) -> Result<(), String> {
    let (models_path, mut models) = load_config(dir, "models")?;
    let (config_path, mut config) = load_config(dir, "config")?;
    let mut models_changed = false;
    if yaml_get(&models, "providers").is_some() {
        models_changed = child_map(&mut models, "providers")?
            .remove(yaml_str("echobird"))
            .is_some();
    }
    let mut config_changed = false;
    if yaml_get(&config, "modelRoles").is_some() {
        child_map(&mut config, "modelRoles")?.retain(|_, value| {
            let owned = value.as_str().is_some_and(|v| v.starts_with("echobird/"));
            config_changed |= owned;
            !owned
        });
    }
    if config_changed {
        write_yaml_file(&config_path, &config)?;
    }
    if models_changed {
        write_yaml_file(&models_path, &models)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("echobird-omp-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, text: &str) {
            fs::write(self.0.join(name), text).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn model() -> ModelInfo {
        serde_json::from_value(serde_json::json!({
            "name": "Example Model",
            "model": "vendor/model-1",
            "baseUrl": "https://example.com/v1/",
            "apiKey": "test:key#with-quotes\"",
            "protocol": "openai"
        }))
        .unwrap()
    }

    #[test]
    fn apply_round_trips_yaml_and_preserves_other_providers_and_roles() {
        let fixture = Fixture::new();
        fixture.write(
            "models.yaml",
            "providers:\n  personal:\n    apiKey: PERSONAL_KEY\n",
        );
        fixture.write(
            "config.yaml",
            "theme:\n  dark: dracula\nmodelRoles:\n  smol: personal/small\n",
        );
        let info = model();
        apply_at(&fixture.0, &info).unwrap();
        assert!(!fixture.0.join("models.yml").exists());
        assert!(!fixture.0.join("config.yml").exists());
        let (_, models) = load_config(&fixture.0, "models").unwrap();
        assert_eq!(
            models["providers"]["personal"]["apiKey"].as_str(),
            Some("PERSONAL_KEY")
        );
        let (_, config) = load_config(&fixture.0, "config").unwrap();
        assert_eq!(config["theme"]["dark"].as_str(), Some("dracula"));
        assert_eq!(
            config["modelRoles"]["smol"].as_str(),
            Some("personal/small")
        );
        assert_eq!(
            config["modelRoles"]["default"].as_str(),
            Some("echobird/vendor/model-1")
        );
        let loaded = read_at(&fixture.0).unwrap();
        assert_eq!(loaded.model, info.model);
        assert_eq!(loaded.name, info.name);
        assert_eq!(loaded.api_key, info.api_key);
        assert_eq!(loaded.base_url.as_deref(), Some("https://example.com/v1"));
        assert_eq!(loaded.protocol.as_deref(), Some("openai"));
        assert_eq!(loaded.anthropic_url, None);
    }

    #[test]
    fn protocol_switch_and_keyless_models_replace_only_echobird() {
        let fixture = Fixture::new();
        let mut info = model();
        apply_at(&fixture.0, &info).unwrap();
        info.protocol = Some("anthropic".into());
        info.anthropic_url = Some("https://example.com/anthropic/v1/".into());
        apply_at(&fixture.0, &info).unwrap();
        let loaded = read_at(&fixture.0).unwrap();
        assert_eq!(loaded.protocol.as_deref(), Some("anthropic"));
        assert_eq!(
            loaded.anthropic_url.as_deref(),
            Some("https://example.com/anthropic/v1")
        );
        info.protocol = Some("openai".into());
        info.base_url = Some("http://127.0.0.1:8080/v1".into());
        info.api_key = None;
        apply_at(&fixture.0, &info).unwrap();
        let (_, models) = load_config(&fixture.0, "models").unwrap();
        let provider = &models["providers"]["echobird"];
        assert_eq!(provider["api"].as_str(), Some("openai-completions"));
        assert_eq!(provider["auth"].as_str(), Some("none"));
        assert!(yaml_get(provider, "apiKey").is_none());
        assert_eq!(read_at(&fixture.0).unwrap().api_key, None);
    }

    #[test]
    fn switching_models_updates_owned_roles_and_preserves_thinking() {
        let fixture = Fixture::new();
        let mut info = model();
        apply_at(&fixture.0, &info).unwrap();
        fixture.write("config.yml", "modelRoles:\n  default: echobird/vendor/model-1\n  smol: echobird/vendor/model-1:high\n  slow: personal/main\n  vision: '@smol'\n");
        info.model = Some("vendor/model-2".into());
        apply_at(&fixture.0, &info).unwrap();
        let (_, config) = load_config(&fixture.0, "config").unwrap();
        assert_eq!(
            config["modelRoles"]["default"].as_str(),
            Some("echobird/vendor/model-2")
        );
        assert_eq!(
            config["modelRoles"]["smol"].as_str(),
            Some("echobird/vendor/model-2:high")
        );
        assert_eq!(config["modelRoles"]["slow"].as_str(), Some("personal/main"));
        assert_eq!(config["modelRoles"]["vision"].as_str(), Some("@smol"));
        assert_eq!(read_at(&fixture.0).unwrap().model, info.model);
    }

    #[test]
    fn legacy_database_settings_are_not_masked_by_new_yaml() {
        for schema in [
            "CREATE TABLE settings (id INTEGER, data TEXT); INSERT INTO settings VALUES (1, '{\"shellPath\":\"bash\"}');",
            "CREATE TABLE settings (key TEXT, value TEXT); INSERT INTO settings VALUES ('shellPath', '\"bash\"');",
        ] {
            let fixture = Fixture::new();
            let path = fixture.0.join("agent.db");
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch(schema).unwrap();
            drop(db);
            let before = fs::read(&path).unwrap();
            assert!(apply_at(&fixture.0, &model()).unwrap_err().contains("migrate"));
            assert!(!fixture.0.join("config.yml").exists());
            assert!(!fixture.0.join("models.yml").exists());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn database_without_legacy_settings_allows_model_configuration() {
        let fixture = Fixture::new();
        let db = rusqlite::Connection::open(fixture.0.join("agent.db")).unwrap();
        db.execute_batch("CREATE TABLE settings (key TEXT, value TEXT);")
            .unwrap();
        drop(db);
        apply_at(&fixture.0, &model()).unwrap();
        assert!(read_at(&fixture.0).is_some());
    }

    #[test]
    fn restore_preserves_user_default_and_removes_only_owned_roles() {
        let fixture = Fixture::new();
        apply_at(&fixture.0, &model()).unwrap();
        fixture.write("config.yml", "theme:\n  dark: dracula\nmodelRoles:\n  default: personal/main\n  smol: echobird/vendor/model-1\n  slow: personal/big\n");
        fixture.write("models.yml", "providers:\n  personal:\n    apiKey: PERSONAL_KEY\n  echobird:\n    apiKey: test-key\n");
        fixture.write("auth.json", "{\"user-credential\":\"untouched\"}");
        let auth = fs::read(fixture.0.join("auth.json")).unwrap();
        restore_at(&fixture.0).unwrap();
        let (_, models) = load_config(&fixture.0, "models").unwrap();
        assert!(yaml_get(&models["providers"], "echobird").is_none());
        assert_eq!(
            models["providers"]["personal"]["apiKey"].as_str(),
            Some("PERSONAL_KEY")
        );
        let (_, config) = load_config(&fixture.0, "config").unwrap();
        assert_eq!(
            config["modelRoles"]["default"].as_str(),
            Some("personal/main")
        );
        assert_eq!(config["modelRoles"]["slow"].as_str(), Some("personal/big"));
        assert!(yaml_get(&config["modelRoles"], "smol").is_none());
        assert_eq!(config["theme"]["dark"].as_str(), Some("dracula"));
        assert_eq!(fs::read(fixture.0.join("auth.json")).unwrap(), auth);
        assert!(read_at(&fixture.0).is_none());
    }

    #[test]
    fn invalid_config_does_not_modify_either_file() {
        for invalid in ["modelRoles: [", "modelRoles: invalid", "- invalid root"] {
            let fixture = Fixture::new();
            fixture.write(
                "models.yml",
                "providers:\n  personal:\n    apiKey: preserved\n",
            );
            fixture.write("config.yml", invalid);
            let models = fs::read(fixture.0.join("models.yml")).unwrap();
            assert!(apply_at(&fixture.0, &model()).is_err());
            assert!(restore_at(&fixture.0).is_err());
            assert_eq!(fs::read(fixture.0.join("models.yml")).unwrap(), models);
            assert_eq!(
                fs::read_to_string(fixture.0.join("config.yml")).unwrap(),
                invalid
            );
        }
    }

    #[test]
    fn legacy_settings_are_not_masked_by_new_yaml() {
        let fixture = Fixture::new();
        fixture.write("settings.json", "{\"shellPath\":\"bash\"}");
        assert!(apply_at(&fixture.0, &model())
            .unwrap_err()
            .contains("migrate"));
        assert!(!fixture.0.join("config.yml").exists());
        assert!(!fixture.0.join("models.yml").exists());
        assert_eq!(
            fs::read_to_string(fixture.0.join("settings.json")).unwrap(),
            "{\"shellPath\":\"bash\"}"
        );
    }

    #[test]
    fn yml_takes_precedence_and_empty_model_is_rejected() {
        let fixture = Fixture::new();
        fixture.write("config.yml", "theme:\n  dark: selected\n");
        fixture.write("config.yaml", "theme:\n  dark: ignored\n");
        apply_at(&fixture.0, &model()).unwrap();
        let (_, config) = load_config(&fixture.0, "config").unwrap();
        assert_eq!(config["theme"]["dark"].as_str(), Some("selected"));
        assert_eq!(
            fs::read_to_string(fixture.0.join("config.yaml")).unwrap(),
            "theme:\n  dark: ignored\n"
        );
        let before = fs::read(fixture.0.join("models.yml")).unwrap();
        let mut invalid = model();
        invalid.model = Some(" ".into());
        assert!(apply_at(&fixture.0, &invalid).is_err());
        assert_eq!(fs::read(fixture.0.join("models.yml")).unwrap(), before);
    }
}

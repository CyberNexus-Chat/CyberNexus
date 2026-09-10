//! Model configuration for dsh.

use super::{
    read_yaml_file, write_yaml_file, yaml_as_map_mut, yaml_child_map, yaml_get, yaml_map, yaml_str,
    ApplyResult, ModelInfo,
};
use std::path::PathBuf;

// ════════════════════════════════════════════════════════════════
//  Type 3e: DeepSeek Harness (dsh) — DeepSeek's open-source agent
//  harness (developer preview). Config is split across TWO YAML files:
//    ~/.dsh/settings.yaml      llm-pi-ai.providers.<id> + agent-default-model
//    ~/.dsh/.credentials.yaml  <apiKeyEnv>: sk-...
//  Dual-protocol via the pi-ai adapter (anthropic-messages | openai-completions).
//  Same "write an echobird provider route + active selector" mechanism as
//  OpenScience; the route key (Provider ID) is a fixed lowercase constant.
//  dsh hot-reloads settings.yaml, but EchoBird still restarts the tracked
//  instance on start so a model switch + restart is deterministic.
// ════════════════════════════════════════════════════════════════

const DSH_API_KEY_ENV: &str = "ECHOBIRD_API_KEY";

fn dsh_config_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".dsh")
}

fn dsh_settings_path() -> PathBuf {
    dsh_config_dir().join("settings.yaml")
}

fn dsh_credentials_path() -> PathBuf {
    dsh_config_dir().join(".credentials.yaml")
}

pub(super) fn apply_dsh(model_info: &ModelInfo) -> ApplyResult {
    let model_id = model_info
        .model
        .as_deref()
        .or(model_info.name.as_deref())
        .unwrap_or("");
    if model_id.is_empty() {
        return ApplyResult {
            success: false,
            message: "Model ID is empty, cannot apply config".to_string(),
        };
    }

    // pi-ai protocol per wire: Anthropic → anthropic-messages, else
    // openai-completions. `endpoint` is always set after this block (mirror
    // apply_openscience: the anthropic arm early-returns on an empty URL).
    let is_anthropic = model_info.protocol.as_deref() == Some("anthropic");
    let api = if is_anthropic {
        "anthropic-messages"
    } else {
        "openai-completions"
    };
    let endpoint = if is_anthropic {
        match model_info
            .anthropic_url
            .as_deref()
            .or(model_info.base_url.as_deref())
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(|u| u.trim_end_matches('/').to_string())
        {
            Some(u) => u,
            None => {
                return ApplyResult {
                    success: false,
                    message: "Base URL is empty. Pick a model first.".to_string(),
                };
            }
        }
    } else {
        model_info
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/')
            .to_string()
    };

    // Local llama-server / vllm proxies need no real key (mirror openscience).
    let raw_api_key = model_info.api_key.as_deref().unwrap_or("");
    let is_local_provider = endpoint.contains("127.0.0.1") || endpoint.contains("localhost");
    let api_key = if !raw_api_key.is_empty() {
        raw_api_key.to_string()
    } else if is_local_provider {
        "local-no-auth".to_string()
    } else {
        return ApplyResult {
            success: false,
            message: "API Key is empty, cannot apply DeepSeek Harness config.".to_string(),
        };
    };

    let display_name = model_info.name.as_deref().unwrap_or(model_id);

    // ── settings.yaml: llm-pi-ai.providers.echobird + agent-default-model ──
    let settings_path = dsh_settings_path();
    let mut settings = read_yaml_file(&settings_path).unwrap_or_else(yaml_map);
    {
        let settings_map = yaml_as_map_mut(&mut settings);
        {
            let llm = yaml_child_map(settings_map, "llm-pi-ai");
            let providers = yaml_child_map(llm, "providers");

            let mut route = serde_yaml_ng::Mapping::new();
            route.insert(yaml_str("displayName"), yaml_str("EchoBird"));
            route.insert(yaml_str("apiKeyEnv"), yaml_str(DSH_API_KEY_ENV));
            route.insert(yaml_str("api"), yaml_str(api));
            route.insert(yaml_str("baseURL"), yaml_str(&endpoint));
            let mut model = serde_yaml_ng::Mapping::new();
            model.insert(yaml_str("id"), yaml_str(model_id));
            model.insert(yaml_str("name"), yaml_str(display_name));
            route.insert(
                yaml_str("models"),
                serde_yaml_ng::Value::Sequence(vec![serde_yaml_ng::Value::Mapping(model)]),
            );
            providers.insert(yaml_str("echobird"), serde_yaml_ng::Value::Mapping(route));
        }

        // Active selector — new sessions default to this route/model.
        let mut selector = serde_yaml_ng::Mapping::new();
        selector.insert(yaml_str("provider"), yaml_str("echobird"));
        selector.insert(yaml_str("model"), yaml_str(model_id));
        settings_map.insert(
            yaml_str("agent-default-model"),
            serde_yaml_ng::Value::Mapping(selector),
        );
    }

    // ── .credentials.yaml: ECHOBIRD_API_KEY ──
    let creds_path = dsh_credentials_path();
    let mut creds = read_yaml_file(&creds_path).unwrap_or_else(yaml_map);
    yaml_as_map_mut(&mut creds).insert(yaml_str(DSH_API_KEY_ENV), yaml_str(&api_key));

    if let Err(e) = write_yaml_file(&settings_path, &settings) {
        return ApplyResult {
            success: false,
            message: e,
        };
    }
    if let Err(e) = write_yaml_file(&creds_path, &creds) {
        return ApplyResult {
            success: false,
            message: e,
        };
    }

    log::info!(
        "[ToolConfigManager] DeepSeek Harness config written to {:?} + {:?}",
        settings_path,
        creds_path
    );
    ApplyResult {
        success: true,
        message: format!(
            "Model \"{}\" configured for DeepSeek Harness (echobird/{}) — start or restart `dsh web` to pick it up.",
            display_name, model_id
        ),
    }
}

pub(super) fn read_dsh() -> Option<ModelInfo> {
    let settings = read_yaml_file(&dsh_settings_path())?;
    let selector = yaml_get(&settings, "agent-default-model")?;
    if yaml_get(selector, "provider")?.as_str()? != "echobird" {
        // Not our provider - show as unconfigured so the user can apply.
        return None;
    }
    let model_id = yaml_get(selector, "model")?.as_str()?;

    let route = yaml_get(
        yaml_get(yaml_get(&settings, "llm-pi-ai")?, "providers")?,
        "echobird",
    )?;
    let api = yaml_get(route, "api")
        .and_then(|v| v.as_str())
        .unwrap_or("openai-completions");
    let protocol = if api == "anthropic-messages" {
        "anthropic"
    } else {
        "openai"
    };
    let base_url = yaml_get(route, "baseURL")
        .and_then(|v| v.as_str())
        .map(String::from);
    // Display name: read the models list for the entry matching `model_id`.
    let name = yaml_get(route, "models")
        .and_then(|m| m.as_sequence())
        .and_then(|seq| {
            seq.iter()
                .find(|e| yaml_get(e, "id").and_then(|v| v.as_str()) == Some(model_id))
        })
        .and_then(|e| yaml_get(e, "name").and_then(|v| v.as_str()))
        .map(String::from)
        .unwrap_or_else(|| model_id.to_string());

    let anthropic_url = if protocol == "anthropic" {
        base_url.clone()
    } else {
        None
    };

    Some(ModelInfo {
        name: Some(name),
        model: Some(model_id.to_string()),
        base_url,
        api_key: None,
        anthropic_url,
        protocol: Some(protocol.to_string()),
        display_model: None,
        relay_mode: None,
        responses_passthrough: None,
        web_search: None,
        one_m_context: None,
    })
}

pub(super) fn restore_dsh_to_official() -> ApplyResult {
    // settings.yaml: drop the echobird route + the agent-default-model selector.
    let settings_path = dsh_settings_path();
    let mut updated_settings = false;
    if settings_path.exists() {
        let mut settings = match read_yaml_file(&settings_path) {
            Some(s) => s,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!(
                        "Failed to parse DeepSeek Harness config: {}",
                        settings_path.display()
                    ),
                };
            }
        };
        let settings_map = yaml_as_map_mut(&mut settings);
        if let Some(llm) = settings_map
            .get_mut(yaml_str("llm-pi-ai"))
            .and_then(|v| v.as_mapping_mut())
        {
            if let Some(providers) = llm
                .get_mut(yaml_str("providers"))
                .and_then(|v| v.as_mapping_mut())
            {
                if providers.remove(yaml_str("echobird")).is_some() {
                    updated_settings = true;
                }
            }
        }
        let is_echobird = settings_map
            .get(yaml_str("agent-default-model"))
            .and_then(|v| v.as_mapping())
            .and_then(|m| m.get(yaml_str("provider")))
            .and_then(|v| v.as_str())
            == Some("echobird");
        if is_echobird {
            settings_map.remove(yaml_str("agent-default-model"));
            updated_settings = true;
        }
        if updated_settings {
            if let Err(e) = write_yaml_file(&settings_path, &settings) {
                return ApplyResult {
                    success: false,
                    message: e,
                };
            }
        }
    }

    // .credentials.yaml: drop ECHOBIRD_API_KEY.
    let creds_path = dsh_credentials_path();
    let mut updated_creds = false;
    if creds_path.exists() {
        let mut creds = match read_yaml_file(&creds_path) {
            Some(c) => c,
            None => {
                return ApplyResult {
                    success: false,
                    message: format!(
                        "Failed to parse DeepSeek Harness credentials: {}",
                        creds_path.display()
                    ),
                };
            }
        };
        if yaml_as_map_mut(&mut creds)
            .remove(yaml_str(DSH_API_KEY_ENV))
            .is_some()
        {
            updated_creds = true;
        }
        if updated_creds {
            if let Err(e) = write_yaml_file(&creds_path, &creds) {
                return ApplyResult {
                    success: false,
                    message: e,
                };
            }
        }
    }

    if updated_settings || updated_creds {
        ApplyResult {
            success: true,
            message: "DeepSeek Harness restored - Echobird provider removed.".to_string(),
        }
    } else {
        ApplyResult {
            success: true,
            message: "DeepSeek Harness already at defaults - no config to update.".to_string(),
        }
    }
}

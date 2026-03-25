use anyhow::Context;
use codex_config::CONFIG_TOML_FILE;
use std::path::Path;
use toml::Value as TomlValue;
use toml_edit::Array;
use toml_edit::Item as TomlEditItem;
use toml_edit::value;

use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;

const INTERNAL_PROFILE_NAME: &str = "internal";
pub const DEFAULT_INTERNAL_PROFILE_MODEL: &str = "gpt-5.4-2026-03-05";
const AZURE_PROVIDER_ID: &str = "azure";
const AZURE_API_VERSION: &str = "2025-04-01-preview";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapInternalProfileResult {
    pub made_internal_default: bool,
}

pub fn bootstrap_internal_profile(
    codex_home: &Path,
    ak: &str,
    azure_base_url: &str,
    model: &str,
) -> anyhow::Result<BootstrapInternalProfileResult> {
    let ak = ak.trim();
    if ak.is_empty() {
        anyhow::bail!("internal installer requires a non-empty ak");
    }
    let azure_base_url = azure_base_url.trim();
    if azure_base_url.is_empty() {
        anyhow::bail!("internal installer requires a non-empty azure base URL");
    }
    let model = model.trim();
    if model.is_empty() {
        anyhow::bail!("internal installer requires a non-empty model");
    }

    let config_path = codex_home.join(CONFIG_TOML_FILE);
    let serialized = match std::fs::read_to_string(&config_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.into()),
    };
    let existing = if serialized.is_empty() {
        TomlValue::Table(Default::default())
    } else {
        toml::from_str::<TomlValue>(&serialized)
            .with_context(|| format!("failed to parse config at {}", config_path.display()))?
    };

    let made_internal_default = !has_non_empty_string(&existing, &["profile"]);
    let mut edits = installer_owned_edits(ak, azure_base_url, model);

    if made_internal_default {
        edits.push(set_path(&["profile"], value(INTERNAL_PROFILE_NAME)));
    }

    edits.extend(missing_global_default_edits(&existing));

    ConfigEditsBuilder::new(codex_home)
        .with_edits(edits)
        .apply_blocking()?;

    Ok(BootstrapInternalProfileResult {
        made_internal_default,
    })
}

fn installer_owned_edits(ak: &str, azure_base_url: &str, model: &str) -> Vec<ConfigEdit> {
    let mut edits = vec![
        set_path(&["profiles", INTERNAL_PROFILE_NAME, "model"], value(model)),
        set_path(
            &["profiles", INTERNAL_PROFILE_NAME, "model_provider"],
            value(AZURE_PROVIDER_ID),
        ),
        set_path(
            &["profiles", INTERNAL_PROFILE_NAME, "sandbox_mode"],
            value("danger-full-access"),
        ),
        set_path(
            &["profiles", INTERNAL_PROFILE_NAME, "approval_policy"],
            value("on-request"),
        ),
        set_path(
            &["profiles", INTERNAL_PROFILE_NAME, "model_reasoning_effort"],
            value("xhigh"),
        ),
        set_path(
            &[
                "profiles",
                INTERNAL_PROFILE_NAME,
                "plan_mode_reasoning_effort",
            ],
            value("xhigh"),
        ),
        set_path(
            &["profiles", INTERNAL_PROFILE_NAME, "model_max_output_tokens"],
            value(64_000),
        ),
        set_path(
            &["profiles", INTERNAL_PROFILE_NAME, "features", "js_repl"],
            value(false),
        ),
        set_path(
            &["profiles", INTERNAL_PROFILE_NAME, "features", "apps"],
            value(false),
        ),
        set_path(
            &[
                "profiles",
                INTERNAL_PROFILE_NAME,
                "features",
                "guardian_approval",
            ],
            value(false),
        ),
        set_path(
            &[
                "profiles",
                INTERNAL_PROFILE_NAME,
                "features",
                "prevent_idle_sleep",
            ],
            value(true),
        ),
        set_path(
            &["model_providers", AZURE_PROVIDER_ID, "name"],
            value("Azure"),
        ),
        set_path(
            &["model_providers", AZURE_PROVIDER_ID, "base_url"],
            value(azure_base_url),
        ),
        set_path(
            &["model_providers", AZURE_PROVIDER_ID, "wire_api"],
            value("responses"),
        ),
        set_path(
            &[
                "model_providers",
                AZURE_PROVIDER_ID,
                "query_params",
                "api-version",
            ],
            value(AZURE_API_VERSION),
        ),
        set_path(
            &["model_providers", AZURE_PROVIDER_ID, "query_params", "ak"],
            value(ak),
        ),
        set_path(
            &["model_providers", AZURE_PROVIDER_ID, "request_max_retries"],
            value(50),
        ),
        set_path(
            &["model_providers", AZURE_PROVIDER_ID, "stream_max_retries"],
            value(50),
        ),
    ];

    for segments in [
        &["profiles", INTERNAL_PROFILE_NAME, "request_max_retries"][..],
        &["profiles", INTERNAL_PROFILE_NAME, "stream_max_retries"][..],
        &["model_providers", AZURE_PROVIDER_ID, "env_key"][..],
        &["model_providers", AZURE_PROVIDER_ID, "env_key_instructions"][..],
        &[
            "model_providers",
            AZURE_PROVIDER_ID,
            "experimental_bearer_token",
        ][..],
        &["model_providers", AZURE_PROVIDER_ID, "http_headers"][..],
        &["model_providers", AZURE_PROVIDER_ID, "env_http_headers"][..],
        &[
            "model_providers",
            AZURE_PROVIDER_ID,
            "stream_idle_timeout_ms",
        ][..],
        &[
            "model_providers",
            AZURE_PROVIDER_ID,
            "websocket_connect_timeout_ms",
        ][..],
        &["model_providers", AZURE_PROVIDER_ID, "requires_openai_auth"][..],
        &["model_providers", AZURE_PROVIDER_ID, "supports_websockets"][..],
    ] {
        edits.push(clear_path(segments));
    }

    edits
}

fn missing_global_default_edits(existing: &TomlValue) -> Vec<ConfigEdit> {
    let mut edits = Vec::new();

    for (segments, value) in [
        (&["shell_environment_policy", "inherit"][..], value("all")),
        (
            &["shell_environment_policy", "ignore_default_excludes"][..],
            value(true),
        ),
        (&["features", "multi_agent"][..], value(true)),
        (&["features", "voice_transcription"][..], value(true)),
        (&["features", "prevent_idle_sleep"][..], value(true)),
        (&["background_terminal_max_timeout"][..], value(72_000_000)),
        (&["project_doc_max_bytes"][..], value(65_536)),
        (&["suppress_unstable_features_warning"][..], value(true)),
        (&["tui", "theme"][..], value("catppuccin-latte")),
        (&["tui", "notification_method"][..], value("auto")),
    ] {
        if !has_path(existing, segments) {
            edits.push(set_path(segments, value));
        }
    }

    if !has_path(existing, &["tui", "notifications"]) {
        edits.push(set_path(
            &["tui", "notifications"],
            string_array(["agent-turn-complete", "approval-requested"]),
        ));
    }

    edits
}

fn has_non_empty_string(value: &TomlValue, segments: &[&str]) -> bool {
    matches!(
        value_at_path(value, segments),
        Some(TomlValue::String(existing)) if !existing.trim().is_empty()
    )
}

fn has_path(value: &TomlValue, segments: &[&str]) -> bool {
    value_at_path(value, segments).is_some()
}

fn value_at_path<'a>(value: &'a TomlValue, segments: &[&str]) -> Option<&'a TomlValue> {
    let mut current = value;
    for segment in segments {
        let table = current.as_table()?;
        current = table.get(*segment)?;
    }
    Some(current)
}

fn set_path(segments: &[&str], value: TomlEditItem) -> ConfigEdit {
    ConfigEdit::SetPath {
        segments: segments
            .iter()
            .map(|segment| (*segment).to_string())
            .collect(),
        value,
    }
}

fn clear_path(segments: &[&str]) -> ConfigEdit {
    ConfigEdit::ClearPath {
        segments: segments
            .iter()
            .map(|segment| (*segment).to_string())
            .collect(),
    }
}

fn string_array<const N: usize>(values: [&str; N]) -> TomlEditItem {
    let mut array = Array::new();
    for value in values {
        array.push(value);
    }
    TomlEditItem::Value(array.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CONFIG_TOML_FILE;
    use crate::config::ConfigBuilder;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    const TEST_AZURE_BASE_URL: &str = "https://internal.example.test/openapi";

    fn read_config(codex_home: &TempDir) -> anyhow::Result<String> {
        Ok(std::fs::read_to_string(
            codex_home.path().join(CONFIG_TOML_FILE),
        )?)
    }

    #[tokio::test]
    async fn bootstrap_internal_profile_creates_internal_defaults() -> anyhow::Result<()> {
        let codex_home = TempDir::new()?;

        let result = bootstrap_internal_profile(
            codex_home.path(),
            "first-ak",
            TEST_AZURE_BASE_URL,
            DEFAULT_INTERNAL_PROFILE_MODEL,
        )?;
        assert_eq!(
            result,
            BootstrapInternalProfileResult {
                made_internal_default: true,
            }
        );

        let serialized = read_config(&codex_home)?;
        assert!(serialized.contains("profile = \"internal\""));
        assert!(serialized.contains("[profiles.internal]"));
        assert!(serialized.contains("model = \"gpt-5.4-2026-03-05\""));
        assert!(serialized.contains("[model_providers.azure]"));
        assert!(serialized.contains("ak = \"first-ak\""));

        let config = ConfigBuilder::default()
            .codex_home(codex_home.path().to_path_buf())
            .build()
            .await?;
        assert_eq!(
            config.active_profile.as_deref(),
            Some(INTERNAL_PROFILE_NAME)
        );
        assert_eq!(
            config.model.as_deref(),
            Some(DEFAULT_INTERNAL_PROFILE_MODEL)
        );
        assert_eq!(config.model_provider_id, AZURE_PROVIDER_ID);
        assert_eq!(
            config.model_provider.base_url.as_deref(),
            Some(TEST_AZURE_BASE_URL)
        );
        assert_eq!(
            config
                .model_provider
                .query_params
                .as_ref()
                .and_then(|params| params.get("api-version"))
                .map(String::as_str),
            Some(AZURE_API_VERSION)
        );
        assert_eq!(
            config
                .model_provider
                .query_params
                .as_ref()
                .and_then(|params| params.get("ak"))
                .map(String::as_str),
            Some("first-ak")
        );
        assert_eq!(config.model_provider.request_max_retries, Some(50));
        assert_eq!(config.model_provider.stream_max_retries, Some(50));

        Ok(())
    }

    #[tokio::test]
    async fn bootstrap_internal_profile_preserves_existing_profile_and_settings()
    -> anyhow::Result<()> {
        let codex_home = TempDir::new()?;
        std::fs::write(
            codex_home.path().join(CONFIG_TOML_FILE),
            r#"
profile = "team"

[profiles.team]
model = "o4-mini"

[shell_environment_policy]
inherit = "none"

[features]
multi_agent = false
"#,
        )?;

        let result = bootstrap_internal_profile(
            codex_home.path(),
            "second-ak",
            TEST_AZURE_BASE_URL,
            DEFAULT_INTERNAL_PROFILE_MODEL,
        )?;
        assert_eq!(
            result,
            BootstrapInternalProfileResult {
                made_internal_default: false,
            }
        );

        let serialized = read_config(&codex_home)?;
        assert!(serialized.contains("profile = \"team\""));
        assert!(serialized.contains("inherit = \"none\""));
        assert!(serialized.contains("multi_agent = false"));
        assert!(serialized.contains("ignore_default_excludes = true"));
        assert!(serialized.contains("[profiles.internal]"));

        let config = ConfigBuilder::default()
            .codex_home(codex_home.path().to_path_buf())
            .build()
            .await?;
        assert_eq!(config.active_profile.as_deref(), Some("team"));
        assert_eq!(
            config
                .model_providers
                .get(AZURE_PROVIDER_ID)
                .and_then(|provider| provider.query_params.as_ref())
                .and_then(|params| params.get("ak"))
                .map(String::as_str),
            Some("second-ak")
        );

        Ok(())
    }

    #[test]
    fn bootstrap_internal_profile_updates_ak_idempotently() -> anyhow::Result<()> {
        let codex_home = TempDir::new()?;

        bootstrap_internal_profile(
            codex_home.path(),
            "old-ak",
            TEST_AZURE_BASE_URL,
            DEFAULT_INTERNAL_PROFILE_MODEL,
        )?;
        bootstrap_internal_profile(
            codex_home.path(),
            "new-ak",
            TEST_AZURE_BASE_URL,
            DEFAULT_INTERNAL_PROFILE_MODEL,
        )?;
        let after_update = read_config(&codex_home)?;
        assert!(after_update.contains("ak = \"new-ak\""));
        assert!(!after_update.contains("ak = \"old-ak\""));

        bootstrap_internal_profile(
            codex_home.path(),
            "new-ak",
            TEST_AZURE_BASE_URL,
            DEFAULT_INTERNAL_PROFILE_MODEL,
        )?;
        let after_rerun = read_config(&codex_home)?;
        assert_eq!(after_rerun, after_update);

        Ok(())
    }

    #[test]
    fn bootstrap_internal_profile_updates_model_idempotently() -> anyhow::Result<()> {
        let codex_home = TempDir::new()?;

        bootstrap_internal_profile(
            codex_home.path(),
            "same-ak",
            TEST_AZURE_BASE_URL,
            DEFAULT_INTERNAL_PROFILE_MODEL,
        )?;
        bootstrap_internal_profile(codex_home.path(), "same-ak", TEST_AZURE_BASE_URL, "gpt-5.4")?;
        let after_update = read_config(&codex_home)?;
        assert!(after_update.contains("model = \"gpt-5.4\""));
        assert!(!after_update.contains("model = \"gpt-5.4-2026-03-05\""));

        bootstrap_internal_profile(codex_home.path(), "same-ak", TEST_AZURE_BASE_URL, "gpt-5.4")?;
        let after_rerun = read_config(&codex_home)?;
        assert_eq!(after_rerun, after_update);

        Ok(())
    }
}

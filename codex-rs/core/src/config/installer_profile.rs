use anyhow::Context;
use codex_config::CONFIG_TOML_FILE;
use codex_config::ProfileV2Name;
use std::path::Path;
use toml::Value as TomlValue;
use toml_edit::Array;
use toml_edit::Item as TomlEditItem;
use toml_edit::value;

use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use crate::config::resolve_profile_v2_config_path;

pub const DEFAULT_INTERNAL_PROFILE_MODEL: &str = "gpt-5.4-2026-03-05";
const DEFAULT_INTERNAL_PROFILE_NAME: &str = "internal";
const AZURE_PROVIDER_ID: &str = "azure";
const AZURE_API_VERSION: &str = "2025-04-01-preview";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapInternalProfileResult {
    pub created_internal_profile: bool,
    pub configured_base_config: bool,
}

pub fn bootstrap_internal_profile(
    codex_home: &Path,
    profile_name: &ProfileV2Name,
    ak: &str,
    azure_base_url: &str,
    model: Option<&str>,
) -> anyhow::Result<BootstrapInternalProfileResult> {
    let config_path = codex_home.join(CONFIG_TOML_FILE);
    let serialized = match std::fs::read_to_string(&config_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.into()),
    };
    let base_existing = if serialized.is_empty() {
        TomlValue::Table(Default::default())
    } else {
        toml::from_str::<TomlValue>(&serialized)
            .with_context(|| format!("failed to parse config at {}", config_path.display()))?
    };
    let profile_config_path = profile_config_path(codex_home, profile_name);
    let profile_serialized = match std::fs::read_to_string(profile_config_path.as_path()) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.into()),
    };
    let profile_existing = if profile_serialized.is_empty() {
        TomlValue::Table(Default::default())
    } else {
        toml::from_str::<TomlValue>(&profile_serialized).with_context(|| {
            format!(
                "failed to parse config at {}",
                profile_config_path.as_path().display()
            )
        })?
    };

    let legacy_profile_is_selected = value_at_path(&base_existing, &["profile"])
        .and_then(TomlValue::as_str)
        .is_some_and(|profile| profile == profile_name.as_str());
    let base_config_is_empty = base_existing
        .as_table()
        .is_some_and(toml::map::Map::is_empty);
    let base_config_has_internal_defaults = profile_name.as_str() == DEFAULT_INTERNAL_PROFILE_NAME
        && value_at_path(&base_existing, &["model_provider"]).and_then(TomlValue::as_str)
            == Some(AZURE_PROVIDER_ID)
        && first_non_empty_string(
            &[&base_existing],
            &["model_providers", AZURE_PROVIDER_ID, "base_url"],
        )
        .is_some()
        && first_non_empty_string(
            &[&base_existing],
            &["model_providers", AZURE_PROVIDER_ID, "query_params", "ak"],
        )
        .is_some();
    let profile_exists = !profile_serialized.trim().is_empty()
        || legacy_profile_is_selected
        || has_path(&base_existing, &["profiles", profile_name.as_str()])
        || base_config_has_internal_defaults;
    let configure_base_config = profile_name.as_str() == DEFAULT_INTERNAL_PROFILE_NAME
        && profile_serialized.trim().is_empty()
        && !legacy_profile_is_selected
        && !has_path(&base_existing, &["profiles", profile_name.as_str()])
        && (base_config_is_empty || base_config_has_internal_defaults);

    let configs = [&profile_existing, &base_existing];
    let ak = resolve_ak(ak, &configs, profile_exists)?;
    let azure_base_url = resolve_azure_base_url(azure_base_url, &configs, profile_exists)?;
    let model = resolve_model(model, &configs, profile_exists, profile_name.as_str());

    let installer_edits = installer_owned_edits(ak, azure_base_url, model);
    if configure_base_config {
        let mut edits = missing_global_default_edits(&base_existing);
        edits.extend(installer_edits);
        edits.push(clear_path(&["profile"]));
        edits.push(clear_path(&["profiles", profile_name.as_str()]));

        ConfigEditsBuilder::new(codex_home)
            .with_edits(edits)
            .apply_blocking()?;
    } else {
        ConfigEditsBuilder::for_config_path(profile_config_path.as_path())
            .with_edits(installer_edits)
            .apply_blocking()?;

        let mut edits = missing_global_default_edits(&base_existing);
        edits.push(clear_path(&["profile"]));
        edits.push(clear_path(&["profiles", profile_name.as_str()]));

        ConfigEditsBuilder::new(codex_home)
            .with_edits(edits)
            .apply_blocking()?;
    }

    Ok(BootstrapInternalProfileResult {
        created_internal_profile: !profile_exists,
        configured_base_config: configure_base_config,
    })
}

fn installer_owned_edits(ak: &str, azure_base_url: &str, model: &str) -> Vec<ConfigEdit> {
    let mut edits = vec![
        set_path(&["model"], value(model)),
        set_path(&["model_provider"], value(AZURE_PROVIDER_ID)),
        set_path(&["sandbox_mode"], value("danger-full-access")),
        set_path(&["approval_policy"], value("on-request")),
        set_path(&["model_reasoning_effort"], value("xhigh")),
        set_path(&["plan_mode_reasoning_effort"], value("xhigh")),
        set_path(&["model_max_output_tokens"], value(64_000)),
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
            &["model_providers", AZURE_PROVIDER_ID, "retry_429"],
            value(true),
        ),
        set_path(
            &["model_providers", AZURE_PROVIDER_ID, "stream_max_retries"],
            value(50),
        ),
    ];

    for segments in [
        &["request_max_retries"][..],
        &["stream_max_retries"][..],
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

fn profile_config_path(
    codex_home: &Path,
    profile_name: &ProfileV2Name,
) -> codex_utils_absolute_path::AbsolutePathBuf {
    resolve_profile_v2_config_path(codex_home, profile_name)
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

fn resolve_ak<'a>(
    input_ak: &'a str,
    configs: &[&'a TomlValue],
    profile_exists: bool,
) -> anyhow::Result<&'a str> {
    let input_ak = input_ak.trim();
    if !input_ak.is_empty() {
        return Ok(input_ak);
    }

    if profile_exists
        && let Some(existing_ak) = first_non_empty_string(
            configs,
            &["model_providers", AZURE_PROVIDER_ID, "query_params", "ak"],
        )
    {
        return Ok(existing_ak);
    }

    anyhow::bail!("internal installer requires a non-empty ak");
}

fn resolve_azure_base_url<'a>(
    input_azure_base_url: &'a str,
    configs: &[&'a TomlValue],
    profile_exists: bool,
) -> anyhow::Result<&'a str> {
    let input_azure_base_url = input_azure_base_url.trim();
    if !input_azure_base_url.is_empty() {
        return Ok(input_azure_base_url);
    }

    if profile_exists
        && let Some(existing_azure_base_url) =
            first_non_empty_string(configs, &["model_providers", AZURE_PROVIDER_ID, "base_url"])
    {
        return Ok(existing_azure_base_url);
    }

    anyhow::bail!("internal installer requires a non-empty azure base URL");
}

fn resolve_model<'a>(
    input_model: Option<&'a str>,
    configs: &[&'a TomlValue],
    profile_exists: bool,
    profile_name: &str,
) -> &'a str {
    if let Some(input_model) = input_model
        .map(str::trim)
        .filter(|input_model| !input_model.is_empty())
    {
        return input_model;
    }

    if profile_exists
        && let Some(existing_model) = first_non_empty_string(configs, &["model"])
            .or_else(|| first_non_empty_string(configs, &["profiles", profile_name, "model"]))
    {
        return existing_model;
    }

    DEFAULT_INTERNAL_PROFILE_MODEL
}

fn has_path(value: &TomlValue, segments: &[&str]) -> bool {
    value_at_path(value, segments).is_some()
}

fn first_non_empty_string<'a>(values: &[&'a TomlValue], segments: &[&str]) -> Option<&'a str> {
    values.iter().find_map(|value| {
        value_at_path(value, segments)
            .and_then(TomlValue::as_str)
            .map(str::trim)
            .filter(|existing| !existing.is_empty())
    })
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
    use crate::config::LoaderOverrides;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    const TEST_AZURE_BASE_URL: &str = "https://internal.example.test/openapi";
    const INTERNAL_PROFILE_NAME: &str = "internal";

    fn internal_profile_name() -> ProfileV2Name {
        INTERNAL_PROFILE_NAME
            .parse()
            .expect("internal is a valid profile-v2 name")
    }

    fn read_config(codex_home: &TempDir) -> anyhow::Result<String> {
        Ok(std::fs::read_to_string(
            codex_home.path().join(CONFIG_TOML_FILE),
        )?)
    }

    fn read_internal_profile_config(codex_home: &TempDir) -> anyhow::Result<String> {
        let config_path = profile_config_path(codex_home.path(), &internal_profile_name());
        Ok(std::fs::read_to_string(config_path.as_path())?)
    }

    fn internal_profile_config_exists(codex_home: &TempDir) -> bool {
        let config_path = profile_config_path(codex_home.path(), &internal_profile_name());
        config_path.as_path().exists()
    }

    fn internal_profile_loader_overrides(codex_home: &TempDir) -> anyhow::Result<LoaderOverrides> {
        let profile_name = internal_profile_name();
        Ok(LoaderOverrides {
            user_config_path: Some(profile_config_path(codex_home.path(), &profile_name)),
            user_config_profile: Some(profile_name),
            ..LoaderOverrides::without_managed_config_for_tests()
        })
    }

    fn bootstrap_internal_profile_for_tests(
        codex_home: &TempDir,
        ak: &str,
        azure_base_url: &str,
        model: Option<&str>,
    ) -> anyhow::Result<BootstrapInternalProfileResult> {
        bootstrap_internal_profile(
            codex_home.path(),
            &internal_profile_name(),
            ak,
            azure_base_url,
            model,
        )
    }

    #[tokio::test]
    async fn bootstrap_internal_profile_creates_internal_defaults_in_base_config()
    -> anyhow::Result<()> {
        let codex_home = TempDir::new()?;

        let result = bootstrap_internal_profile_for_tests(
            &codex_home,
            "first-ak",
            TEST_AZURE_BASE_URL,
            Some(DEFAULT_INTERNAL_PROFILE_MODEL),
        )?;
        assert_eq!(
            result,
            BootstrapInternalProfileResult {
                created_internal_profile: true,
                configured_base_config: true,
            }
        );

        let serialized = read_config(&codex_home)?;
        assert!(!serialized.contains("profile = \"internal\""));
        assert!(!serialized.contains("[profiles.internal]"));
        assert!(serialized.contains("model = \"gpt-5.4-2026-03-05\""));
        assert!(serialized.contains("[model_providers.azure]"));
        assert!(serialized.contains("ak = \"first-ak\""));
        assert!(!internal_profile_config_exists(&codex_home));

        let config = ConfigBuilder::without_managed_config_for_tests()
            .codex_home(codex_home.path().to_path_buf())
            .build()
            .await?;
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
        assert_eq!(config.model_provider.retry_429, Some(true));
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

        let result = bootstrap_internal_profile_for_tests(
            &codex_home,
            "second-ak",
            TEST_AZURE_BASE_URL,
            Some(DEFAULT_INTERNAL_PROFILE_MODEL),
        )?;
        assert_eq!(
            result,
            BootstrapInternalProfileResult {
                created_internal_profile: true,
                configured_base_config: false,
            }
        );

        let serialized = read_config(&codex_home)?;
        assert!(!serialized.contains("profile = \"team\""));
        assert!(serialized.contains("inherit = \"none\""));
        assert!(serialized.contains("multi_agent = false"));
        assert!(serialized.contains("ignore_default_excludes = true"));
        assert!(!serialized.contains("[profiles.internal]"));

        let profile_serialized = read_internal_profile_config(&codex_home)?;
        assert!(profile_serialized.contains("model = \"gpt-5.4-2026-03-05\""));

        let config = ConfigBuilder::without_managed_config_for_tests()
            .codex_home(codex_home.path().to_path_buf())
            .loader_overrides(internal_profile_loader_overrides(&codex_home)?)
            .build()
            .await?;
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

        bootstrap_internal_profile_for_tests(
            &codex_home,
            "old-ak",
            TEST_AZURE_BASE_URL,
            Some(DEFAULT_INTERNAL_PROFILE_MODEL),
        )?;
        bootstrap_internal_profile_for_tests(
            &codex_home,
            "new-ak",
            TEST_AZURE_BASE_URL,
            Some(DEFAULT_INTERNAL_PROFILE_MODEL),
        )?;
        let after_update = read_config(&codex_home)?;
        assert!(after_update.contains("ak = \"new-ak\""));
        assert!(!after_update.contains("ak = \"old-ak\""));
        assert!(!internal_profile_config_exists(&codex_home));

        bootstrap_internal_profile_for_tests(
            &codex_home,
            "new-ak",
            TEST_AZURE_BASE_URL,
            Some(DEFAULT_INTERNAL_PROFILE_MODEL),
        )?;
        let after_rerun = read_config(&codex_home)?;
        assert_eq!(after_rerun, after_update);
        assert!(!internal_profile_config_exists(&codex_home));

        Ok(())
    }

    #[test]
    fn bootstrap_internal_profile_updates_model_idempotently() -> anyhow::Result<()> {
        let codex_home = TempDir::new()?;

        bootstrap_internal_profile_for_tests(
            &codex_home,
            "same-ak",
            TEST_AZURE_BASE_URL,
            Some(DEFAULT_INTERNAL_PROFILE_MODEL),
        )?;
        bootstrap_internal_profile_for_tests(
            &codex_home,
            "same-ak",
            TEST_AZURE_BASE_URL,
            Some("gpt-5.4"),
        )?;
        let after_update = read_config(&codex_home)?;
        assert!(after_update.contains("model = \"gpt-5.4\""));
        assert!(!after_update.contains("model = \"gpt-5.4-2026-03-05\""));
        assert!(!internal_profile_config_exists(&codex_home));

        bootstrap_internal_profile_for_tests(
            &codex_home,
            "same-ak",
            TEST_AZURE_BASE_URL,
            Some("gpt-5.4"),
        )?;
        let after_rerun = read_config(&codex_home)?;
        assert_eq!(after_rerun, after_update);
        assert!(!internal_profile_config_exists(&codex_home));

        Ok(())
    }

    #[test]
    fn bootstrap_internal_profile_allows_empty_input_values_when_internal_exists()
    -> anyhow::Result<()> {
        let codex_home = TempDir::new()?;
        std::fs::write(
            codex_home.path().join(CONFIG_TOML_FILE),
            r#"
profile = "internal"

[profiles.internal]
model = "existing-model"

[model_providers.azure]
base_url = "https://existing.example.test/openapi"

[model_providers.azure.query_params]
ak = "existing-ak"
"#,
        )?;

        let result = bootstrap_internal_profile_for_tests(&codex_home, "", "", None)?;
        assert_eq!(
            result,
            BootstrapInternalProfileResult {
                created_internal_profile: false,
                configured_base_config: false,
            }
        );

        let config = toml::from_str::<TomlValue>(&read_config(&codex_home)?)?;
        let profile_config =
            toml::from_str::<TomlValue>(&read_internal_profile_config(&codex_home)?)?;
        assert_eq!(
            value_at_path(&profile_config, &["model"]).and_then(TomlValue::as_str),
            Some("existing-model")
        );
        assert_eq!(
            value_at_path(&profile_config, &["model_providers", "azure", "base_url"])
                .and_then(TomlValue::as_str),
            Some("https://existing.example.test/openapi")
        );
        assert_eq!(
            value_at_path(
                &profile_config,
                &["model_providers", "azure", "query_params", "ak"]
            )
            .and_then(TomlValue::as_str),
            Some("existing-ak")
        );
        assert_eq!(value_at_path(&config, &["profile"]), None);
        assert_eq!(value_at_path(&config, &["profiles", "internal"]), None);

        Ok(())
    }

    #[test]
    fn bootstrap_internal_profile_uses_default_model_when_input_model_is_absent()
    -> anyhow::Result<()> {
        let codex_home = TempDir::new()?;

        bootstrap_internal_profile_for_tests(&codex_home, "ak", TEST_AZURE_BASE_URL, None)?;

        let config = toml::from_str::<TomlValue>(&read_config(&codex_home)?)?;
        assert_eq!(
            value_at_path(&config, &["model"]).and_then(TomlValue::as_str),
            Some(DEFAULT_INTERNAL_PROFILE_MODEL)
        );
        assert!(!internal_profile_config_exists(&codex_home));

        Ok(())
    }
}

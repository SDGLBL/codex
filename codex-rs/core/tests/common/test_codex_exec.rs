#![allow(clippy::expect_used)]
use codex_core::auth::CODEX_API_KEY_ENV_VAR;
use std::path::Path;
use tempfile::TempDir;
use wiremock::MockServer;

pub struct TestCodexExecBuilder {
    home: TempDir,
    cwd: TempDir,
}

impl TestCodexExecBuilder {
    pub fn cmd(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::new(
            codex_utils_cargo_bin::cargo_bin("codex-exec")
                .expect("should find binary for codex-exec"),
        );
        cmd.current_dir(self.cwd.path())
            .env("CODEX_HOME", self.home.path())
            .env(CODEX_API_KEY_ENV_VAR, "dummy");
        cmd
    }
    pub fn cmd_with_server(&self, server: &MockServer) -> assert_cmd::Command {
        let mut cmd = self.cmd();
        let base = format!("{}/v1", server.uri());
        let provider_override = format!(
            "model_providers.mock={{ name = \"mock\", base_url = {}, env_key = {}, wire_api = \"responses\", supports_websockets = false }}",
            toml_string_literal(&base),
            toml_string_literal(CODEX_API_KEY_ENV_VAR),
        );
        cmd.arg("-c")
            .arg(provider_override)
            .arg("-c")
            .arg("model_provider=\"mock\"");
        cmd
    }

    pub fn cwd_path(&self) -> &Path {
        self.cwd.path()
    }
    pub fn home_path(&self) -> &Path {
        self.home.path()
    }
}

fn toml_string_literal(value: &str) -> String {
    serde_json::to_string(value).expect("serialize TOML string literal")
}

pub fn test_codex_exec() -> TestCodexExecBuilder {
    TestCodexExecBuilder {
        home: TempDir::new().expect("create temp home"),
        cwd: TempDir::new().expect("create temp cwd"),
    }
}

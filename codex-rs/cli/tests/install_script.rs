#![cfg(not(windows))]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use anyhow::Result;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use toml::Value as TomlValue;

const INSTALL_VERSION: &str = "9.9.9";
const INSTALL_TAG: &str = "internal-rust-v9.9.9";
const INSTALL_AK: &str = "install-ak";
const INSTALL_AZURE_BASE_URL: &str = "https://internal.example.test/openapi";

struct PlatformFixture<'a> {
    uname_s: &'a str,
    uname_m: &'a str,
    proc_translated: Option<&'a str>,
    vendor_target: &'a str,
    platform_label: &'a str,
}

fn make_executable(path: &Path) -> Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    Ok(codex_utils_cargo_bin::repo_root()?)
}

fn installer_script_path() -> Result<PathBuf> {
    Ok(repo_root()?.join("scripts/install/install.sh"))
}

fn codex_binary_path() -> Result<PathBuf> {
    Ok(codex_utils_cargo_bin::cargo_bin("codex")?)
}

fn base_test_path() -> String {
    "/usr/bin:/bin:/usr/sbin:/sbin".to_string()
}

fn create_release_fixture(root: &Path, platform: &PlatformFixture<'_>) -> Result<String> {
    let release_dir = root.join("releases").join("download").join(INSTALL_TAG);
    fs::create_dir_all(&release_dir)?;
    fs::write(
        release_dir.join("install.sh"),
        "# release installer marker\n",
    )?;

    let native_stage = TempDir::new_in(root)?;
    let native_asset_name = format!("codex-{}.tar.gz", platform.vendor_target);
    let native_binary_path = native_stage.path().join("codex");
    fs::copy(codex_binary_path()?, &native_binary_path)?;
    make_executable(&native_binary_path)?;
    run_command(
        Command::new("tar")
            .arg("-C")
            .arg(native_stage.path())
            .arg("-czf")
            .arg(release_dir.join(native_asset_name))
            .arg("codex"),
    )?;

    let rg_stage = TempDir::new_in(root)?;
    let rg_path = rg_stage.path().join("rg");
    fs::write(&rg_path, "#!/bin/sh\necho rg smoke test\n")?;
    make_executable(&rg_path)?;
    run_command(
        Command::new("tar")
            .arg("-C")
            .arg(rg_stage.path())
            .arg("-czf")
            .arg(release_dir.join(format!("rg-{}.tar.gz", platform.vendor_target)))
            .arg("rg"),
    )?;

    Ok(format!(
        "file://{}",
        root.join("releases").join("download").display()
    ))
}

fn run_command(command: &mut Command) -> Result<()> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }

    anyhow::bail!(
        "command failed: status={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_installer(
    home: &Path,
    release_base_url: &str,
    platform: &PlatformFixture<'_>,
    extra_path_prefix: Option<&Path>,
) -> Result<String> {
    run_installer_with_model(
        home,
        release_base_url,
        platform,
        extra_path_prefix,
        /*install_model*/ None,
    )
}

fn run_installer_with_model(
    home: &Path,
    release_base_url: &str,
    platform: &PlatformFixture<'_>,
    extra_path_prefix: Option<&Path>,
    install_model: Option<&str>,
) -> Result<String> {
    run_installer_with_shell(
        home,
        release_base_url,
        platform,
        extra_path_prefix,
        "/bin/sh",
        install_model,
    )
}

fn run_installer_with_shell(
    home: &Path,
    release_base_url: &str,
    platform: &PlatformFixture<'_>,
    extra_path_prefix: Option<&Path>,
    shell: &str,
    install_model: Option<&str>,
) -> Result<String> {
    let mut path = base_test_path();
    if let Some(prefix) = extra_path_prefix {
        path = format!("{}:{path}", prefix.display());
    }

    let mut command = Command::new("sh");
    command
        .arg(installer_script_path()?)
        .arg(INSTALL_VERSION)
        .env("HOME", home)
        .env("SHELL", shell)
        .env("CODEX_INSTALL_AK", INSTALL_AK)
        .env("CODEX_INSTALL_AZURE_BASE_URL", INSTALL_AZURE_BASE_URL)
        .env("CODEX_INSTALL_RELEASE_BASE_URL", release_base_url)
        .env("CODEX_INSTALL_UNAME_S", platform.uname_s)
        .env("CODEX_INSTALL_UNAME_M", platform.uname_m)
        .env("PATH", path);
    if let Some(install_model) = install_model {
        command.env("CODEX_INSTALL_MODEL", install_model);
    }
    if let Some(proc_translated) = platform.proc_translated {
        command.env("CODEX_INSTALL_PROC_TRANSLATED", proc_translated);
    }

    let output = command.output()?;
    if !output.status.success() {
        anyhow::bail!(
            "installer failed: status={:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn run_installer_latest(
    home: &Path,
    release_base_url: &str,
    latest_install_url: &str,
    latest_release_url: &str,
    platform: &PlatformFixture<'_>,
    extra_path_prefix: Option<&Path>,
) -> Result<String> {
    let mut path = base_test_path();
    if let Some(prefix) = extra_path_prefix {
        path = format!("{}:{path}", prefix.display());
    }

    let output = Command::new("sh")
        .arg(installer_script_path()?)
        .env("HOME", home)
        .env("SHELL", "/bin/sh")
        .env("CODEX_INSTALL_AK", INSTALL_AK)
        .env("CODEX_INSTALL_AZURE_BASE_URL", INSTALL_AZURE_BASE_URL)
        .env("CODEX_INSTALL_RELEASE_BASE_URL", release_base_url)
        .env("CODEX_INSTALL_LATEST_INSTALL_URL", latest_install_url)
        .env("CODEX_INSTALL_LATEST_RELEASE_URL", latest_release_url)
        .env("CODEX_INSTALL_UNAME_S", platform.uname_s)
        .env("CODEX_INSTALL_UNAME_M", platform.uname_m)
        .env("PATH", path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "installer failed: status={:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn run_installer_failure(
    home: &Path,
    release_base_url: &str,
    platform: &PlatformFixture<'_>,
) -> Result<(String, String)> {
    let output = Command::new("sh")
        .arg(installer_script_path()?)
        .arg(INSTALL_VERSION)
        .env("HOME", home)
        .env("SHELL", "/bin/sh")
        .env("CODEX_INSTALL_AK", INSTALL_AK)
        .env("CODEX_INSTALL_AZURE_BASE_URL", INSTALL_AZURE_BASE_URL)
        .env("CODEX_INSTALL_RELEASE_BASE_URL", release_base_url)
        .env("CODEX_INSTALL_UNAME_S", platform.uname_s)
        .env("CODEX_INSTALL_UNAME_M", platform.uname_m)
        .env("PATH", base_test_path())
        .output()?;

    if output.status.success() {
        anyhow::bail!(
            "installer unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok((
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

fn read_installed_config(home: &Path) -> Result<TomlValue> {
    let config_path = home.join(".codex").join("config.toml");
    let serialized = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    Ok(toml::from_str(&serialized)?)
}

fn value_at_path<'a>(value: &'a TomlValue, segments: &[&str]) -> Option<&'a TomlValue> {
    let mut current = value;
    for segment in segments {
        let table = current.as_table()?;
        current = table.get(*segment)?;
    }
    Some(current)
}

fn assert_installed_binary_loads_internal_profile(home: &Path, install_dir: &Path) -> Result<()> {
    let output = Command::new(install_dir.join("codex"))
        .arg("-p")
        .arg("internal")
        .arg("features")
        .arg("list")
        .env("HOME", home)
        .env(
            "PATH",
            format!("{}:{}", install_dir.display(), base_test_path()),
        )
        .output()?;
    if output.status.success() {
        return Ok(());
    }

    anyhow::bail!(
        "installed codex failed to load internal profile: status={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn install_script_selects_linux_x86_64_musl_asset_and_bootstraps_config() -> Result<()> {
    let platform = PlatformFixture {
        uname_s: "Linux",
        uname_m: "x86_64",
        proc_translated: None,
        vendor_target: "x86_64-unknown-linux-musl",
        platform_label: "Linux (x64)",
    };
    let fixtures = TempDir::new()?;
    let home = TempDir::new()?;
    let release_base_url = create_release_fixture(fixtures.path(), &platform)?;

    let stdout = run_installer(
        home.path(),
        &release_base_url,
        &platform,
        /*extra_path_prefix*/ None,
    )?;
    assert!(stdout.contains(platform.platform_label));
    assert!(stdout.contains("Configured internal profile and set it as the default profile."));

    let install_dir = home.path().join(".local").join("bin");
    assert!(install_dir.join("codex").is_file());
    assert!(install_dir.join("rg").is_file());
    assert!(home.path().join(".profile").is_file());

    let config = read_installed_config(home.path())?;
    assert_eq!(
        value_at_path(&config, &["profile"]).and_then(TomlValue::as_str),
        Some("internal")
    );
    assert_eq!(
        value_at_path(&config, &["model_providers", "azure", "base_url"])
            .and_then(TomlValue::as_str),
        Some(INSTALL_AZURE_BASE_URL)
    );
    assert_eq!(
        value_at_path(&config, &["model_providers", "azure", "query_params", "ak"])
            .and_then(TomlValue::as_str),
        Some(INSTALL_AK)
    );

    assert_installed_binary_loads_internal_profile(home.path(), &install_dir)?;
    Ok(())
}

#[test]
fn install_script_resolves_latest_version_from_install_url_when_api_lookup_fails() -> Result<()> {
    let platform = PlatformFixture {
        uname_s: "Linux",
        uname_m: "x86_64",
        proc_translated: None,
        vendor_target: "x86_64-unknown-linux-musl",
        platform_label: "Linux (x64)",
    };
    let fixtures = TempDir::new()?;
    let home = TempDir::new()?;
    let release_base_url = create_release_fixture(fixtures.path(), &platform)?;
    let latest_install_url = format!(
        "file://{}/{INSTALL_TAG}/install.sh",
        fixtures.path().join("releases").join("download").display()
    );

    let stdout = run_installer_latest(
        home.path(),
        &release_base_url,
        &latest_install_url,
        "file:///definitely-missing/latest.json",
        &platform,
        /*extra_path_prefix*/ None,
    )?;

    assert!(stdout.contains("Resolved version: 9.9.9"));
    assert!(
        home.path()
            .join(".local")
            .join("bin")
            .join("codex")
            .is_file()
    );

    Ok(())
}

#[test]
fn install_script_resolves_latest_version_from_redirect_location_header() -> Result<()> {
    let platform = PlatformFixture {
        uname_s: "Linux",
        uname_m: "x86_64",
        proc_translated: None,
        vendor_target: "x86_64-unknown-linux-musl",
        platform_label: "Linux (x64)",
    };
    let fixtures = TempDir::new()?;
    let home = TempDir::new()?;
    let tools = TempDir::new()?;
    let release_base_url = create_release_fixture(fixtures.path(), &platform)?;
    let curl_output = Command::new("sh")
        .arg("-c")
        .arg("command -v curl")
        .output()?;
    let real_curl = String::from_utf8(curl_output.stdout)?;
    let real_curl = real_curl.trim();
    let fake_latest_url = "https://example.invalid/latest/install.sh";
    let fake_curl = tools.path().join("curl");
    fs::write(
        &fake_curl,
        format!(
            "#!/bin/sh\nset -eu\nlast=''\nfor arg in \"$@\"; do last=\"$arg\"; done\nif [ \"$last\" = \"{fake_latest_url}\" ]; then\n  printf 'HTTP/2 302\\r\\n'\n  printf 'location: https://github.com/SDGLBL/codex/releases/download/{INSTALL_TAG}/install.sh\\r\\n\\r\\n'\n  exit 0\nfi\nexec {real_curl} \"$@\"\n"
        ),
    )?;
    make_executable(&fake_curl)?;

    let stdout = run_installer_latest(
        home.path(),
        &release_base_url,
        fake_latest_url,
        "file:///definitely-missing/latest.json",
        &platform,
        Some(tools.path()),
    )?;

    assert!(stdout.contains("Resolved version: 9.9.9"));
    assert!(
        home.path()
            .join(".local")
            .join("bin")
            .join("codex")
            .is_file()
    );

    Ok(())
}

#[test]
fn install_script_rejects_linux_arm64_when_release_is_not_published() -> Result<()> {
    let platform = PlatformFixture {
        uname_s: "Linux",
        uname_m: "aarch64",
        proc_translated: None,
        vendor_target: "aarch64-unknown-linux-musl",
        platform_label: "Linux (ARM64)",
    };
    let home = TempDir::new()?;
    let (stdout, stderr) = run_installer_failure(home.path(), "file:///unused", &platform)?;

    assert_eq!(stdout, "");
    assert!(stderr.contains("Linux (ARM64) is not currently published"));

    Ok(())
}

#[test]
fn install_script_prefers_darwin_arm64_asset_under_rosetta() -> Result<()> {
    let platform = PlatformFixture {
        uname_s: "Darwin",
        uname_m: "x86_64",
        proc_translated: Some("1"),
        vendor_target: "aarch64-apple-darwin",
        platform_label: "macOS (Apple Silicon)",
    };
    let fixtures = TempDir::new()?;
    let home = TempDir::new()?;
    let release_base_url = create_release_fixture(fixtures.path(), &platform)?;

    let stdout = run_installer(
        home.path(),
        &release_base_url,
        &platform,
        /*extra_path_prefix*/ None,
    )?;
    assert!(stdout.contains(platform.platform_label));
    assert!(
        home.path()
            .join(".local")
            .join("bin")
            .join("codex")
            .is_file()
    );

    Ok(())
}

#[test]
fn install_script_reuses_existing_codex_install_dir() -> Result<()> {
    let platform = PlatformFixture {
        uname_s: "Linux",
        uname_m: "x86_64",
        proc_translated: None,
        vendor_target: "x86_64-unknown-linux-musl",
        platform_label: "Linux (x64)",
    };
    let fixtures = TempDir::new()?;
    let home = TempDir::new()?;
    let existing_bin = home.path().join("existing-bin");
    fs::create_dir_all(&existing_bin)?;
    fs::write(existing_bin.join("codex"), "#!/bin/sh\necho stale codex\n")?;
    make_executable(&existing_bin.join("codex"))?;

    let release_base_url = create_release_fixture(fixtures.path(), &platform)?;
    let stdout = run_installer(
        home.path(),
        &release_base_url,
        &platform,
        Some(existing_bin.as_path()),
    )?;

    assert!(stdout.contains(&format!("Installing to {}", existing_bin.display())));
    assert!(existing_bin.join("codex").is_file());
    assert!(
        !home
            .path()
            .join(".local")
            .join("bin")
            .join("codex")
            .exists()
    );
    assert!(!home.path().join(".profile").exists());

    Ok(())
}

#[test]
fn install_script_falls_back_when_zshrc_is_not_writable() -> Result<()> {
    let platform = PlatformFixture {
        uname_s: "Darwin",
        uname_m: "aarch64",
        proc_translated: None,
        vendor_target: "aarch64-apple-darwin",
        platform_label: "macOS (Apple Silicon)",
    };
    let fixtures = TempDir::new()?;
    let home = TempDir::new()?;
    let release_base_url = create_release_fixture(fixtures.path(), &platform)?;
    let zshrc_path = home.path().join(".zshrc");
    fs::write(&zshrc_path, "# managed elsewhere\n")?;
    let mut permissions = fs::metadata(&zshrc_path)?.permissions();
    permissions.set_mode(0o400);
    fs::set_permissions(&zshrc_path, permissions)?;

    let stdout = run_installer_with_shell(
        home.path(),
        &release_base_url,
        &platform,
        /*extra_path_prefix*/ None,
        "/bin/zsh",
        /*install_model*/ None,
    )?;

    let zprofile_path = home.path().join(".zprofile");
    assert!(stdout.contains(&format!(
        "PATH updated for future shells in {}",
        zprofile_path.display()
    )));
    assert!(zprofile_path.is_file());
    assert!(fs::read_to_string(&zprofile_path)?.contains("export PATH=\""));

    Ok(())
}

#[test]
fn install_script_honors_codex_install_model_override() -> Result<()> {
    let platform = PlatformFixture {
        uname_s: "Linux",
        uname_m: "x86_64",
        proc_translated: None,
        vendor_target: "x86_64-unknown-linux-musl",
        platform_label: "Linux (x64)",
    };
    let fixtures = TempDir::new()?;
    let home = TempDir::new()?;
    let release_base_url = create_release_fixture(fixtures.path(), &platform)?;

    run_installer_with_model(
        home.path(),
        &release_base_url,
        &platform,
        /*extra_path_prefix*/ None,
        Some("gpt-5.4"),
    )?;

    let config = read_installed_config(home.path())?;
    assert_eq!(
        value_at_path(&config, &["profiles", "internal", "model"]).and_then(TomlValue::as_str),
        Some("gpt-5.4")
    );

    Ok(())
}

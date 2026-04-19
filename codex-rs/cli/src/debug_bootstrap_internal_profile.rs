use anyhow::Context;
use clap::Parser;
use std::io::IsTerminal;
use std::io::Read;

use codex_core::config::bootstrap_internal_profile;
use codex_core::config::find_codex_home;

#[derive(Debug, Parser)]
pub struct DebugBootstrapInternalProfileCommand {
    #[arg(long = "ak-stdin", default_value_t = false)]
    pub ak_stdin: bool,

    #[arg(long = "azure-base-url", value_name = "URL")]
    pub azure_base_url: String,

    #[arg(long = "model", value_name = "MODEL")]
    pub model: Option<String>,
}

pub fn run_debug_bootstrap_internal_profile_command(
    cmd: DebugBootstrapInternalProfileCommand,
) -> anyhow::Result<()> {
    let ak = if cmd.ak_stdin {
        read_ak_from_stdin()?
    } else {
        anyhow::bail!("`codex debug bootstrap-internal-profile` requires `--ak-stdin`");
    };

    let codex_home = find_codex_home()?;
    let result =
        bootstrap_internal_profile(&codex_home, &ak, &cmd.azure_base_url, cmd.model.as_deref())?;

    if result.made_internal_default {
        println!("Configured internal profile and set it as the default profile.");
    } else {
        println!(
            "Updated internal profile. Existing active profile was preserved; run `codex -p internal` to use it."
        );
    }

    Ok(())
}

fn read_ak_from_stdin() -> anyhow::Result<String> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        anyhow::bail!(
            "--ak-stdin expects the ak on stdin. Try piping it, e.g. `printenv CODEX_INSTALL_AK | codex debug bootstrap-internal-profile --ak-stdin`."
        );
    }

    let mut buffer = String::new();
    stdin
        .read_to_string(&mut buffer)
        .context("failed to read ak from stdin")?;

    Ok(buffer.trim().to_string())
}

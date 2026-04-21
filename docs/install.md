## Installing & building

### Install from GitHub releases

The recommended install path for this fork is:

```bash
curl -fsSL https://github.com/SDGLBL/codex/releases/latest/download/install.sh | bash
```

Pin a specific version:

```bash
curl -fsSL https://github.com/SDGLBL/codex/releases/latest/download/install.sh | bash -s -- 0.104.0
```

Useful environment variables:

```bash
CODEX_INSTALL_DIR="$HOME/bin"
CODEX_INSTALL_AK="your-ak"
CODEX_INSTALL_AZURE_BASE_URL="https://your-internal-endpoint"
CODEX_INSTALL_MODEL="gpt-5.4-2026-03-05"
```

Notes:

- The Unix installer downloads the native release binary for your platform and installs the bundled `rg`.
- Linux always selects the musl release assets (`*-unknown-linux-musl`).
- `CODEX_INSTALL_AK` and `CODEX_INSTALL_AZURE_BASE_URL` are optional for interactive installs. If either is unset while bootstrapping a new `internal` profile, the installer prompts for it.
- If `profiles.internal` already exists in `~/.codex/config.toml`, `CODEX_INSTALL_AK`, `CODEX_INSTALL_AZURE_BASE_URL`, and `CODEX_INSTALL_MODEL` can be omitted; missing values fall back to the existing profile/provider config.
- `CODEX_INSTALL_MODEL` is optional. If unset, the installer keeps the existing `profiles.internal.model` when present, otherwise it writes `gpt-5.4-2026-03-05`.
- When bootstrapping a new internal profile in non-interactive mode, both `CODEX_INSTALL_AK` and `CODEX_INSTALL_AZURE_BASE_URL` are required.
- On Windows, use `install.ps1` from the same release page instead of `install.sh`.

### System requirements

| Requirement                 | Details                                                         |
| --------------------------- | --------------------------------------------------------------- |
| Operating systems           | macOS 12+, Ubuntu 20.04+/Debian 10+, or Windows 11 **via WSL2** |
| Git (optional, recommended) | 2.23+ for built-in PR helpers                                   |
| RAM                         | 4-GB minimum (8-GB recommended)                                 |

### DotSlash

The GitHub Release also contains a [DotSlash](https://dotslash-cli.com/) file for the Codex CLI named `codex`. Using a DotSlash file makes it possible to make a lightweight commit to source control to ensure all contributors use the same version of an executable, regardless of what platform they use for development.

### Build from source

```bash
# Clone the repository and navigate to the root of the Cargo workspace.
git clone https://github.com/openai/codex.git
cd codex/codex-rs

# Install the Rust toolchain, if necessary.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup component add rustfmt
rustup component add clippy
# Install helper tools used by the workspace justfile:
cargo install just
# Optional: install nextest for the `just test` helper
cargo install --locked cargo-nextest

# Build Codex.
cargo build

# Launch the TUI with a sample prompt.
cargo run --bin codex -- "explain this codebase to me"

# After making changes, use the root justfile helpers (they default to codex-rs):
just fmt
just fix -p <crate-you-touched>

# Run the relevant tests (project-specific is fastest), for example:
cargo test -p codex-tui
# If you have cargo-nextest installed, `just test` runs the test suite via nextest:
just test
# Avoid `--all-features` for routine local runs because it increases build
# time and `target/` disk usage by compiling additional feature combinations.
# If you specifically want full feature coverage, use:
cargo test --all-features
```

## Tracing / verbose logging

Codex is written in Rust, so it honors the `RUST_LOG` environment variable to configure its logging behavior.

The TUI defaults to `RUST_LOG=codex_core=info,codex_tui=info,codex_rmcp_client=info` and log messages are written to `~/.codex/log/codex-tui.log` by default. For a single run, you can override the log directory with `-c log_dir=...` (for example, `-c log_dir=./.codex-log`).

```bash
tail -F ~/.codex/log/codex-tui.log
```

By comparison, the non-interactive mode (`codex exec`) defaults to `RUST_LOG=error`, but messages are printed inline, so there is no need to monitor a separate file.

See the Rust documentation on [`RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) for more information on the configuration options.

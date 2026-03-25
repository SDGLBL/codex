<p align="center"><code>curl -fsSL https://github.com/SDGLBL/codex/releases/latest/download/install.sh | bash</code></p>
<p align="center"><strong>Codex CLI</strong> is a coding agent from OpenAI that runs locally on your computer.
<p align="center">
  <img src="https://github.com/openai/codex/blob/main/.github/codex-cli-splash.png" alt="Codex CLI splash" width="80%" />
</p>
</br>
If you want Codex in your code editor (VS Code, Cursor, Windsurf), <a href="https://developers.openai.com/codex/ide">install in your IDE.</a>
</br>If you want the desktop app experience, run <code>codex app</code> or visit <a href="https://chatgpt.com/codex?app-landing-page=true">the Codex App page</a>.
</br>If you are looking for the <em>cloud-based agent</em> from OpenAI, <strong>Codex Web</strong>, go to <a href="https://chatgpt.com/codex">chatgpt.com/codex</a>.</p>

---

## Quickstart

### Installing and running Codex CLI

Install with the GitHub release installer:

```shell
curl -fsSL https://github.com/SDGLBL/codex/releases/latest/download/install.sh | bash
```

Pin a specific release:

```shell
curl -fsSL https://github.com/SDGLBL/codex/releases/latest/download/install.sh | bash -s -- 0.104.0
```

You can customize the install with:

```shell
CODEX_INSTALL_DIR="$HOME/bin" CODEX_INSTALL_AK="your-ak" \
CODEX_INSTALL_AZURE_BASE_URL="https://your-internal-endpoint" \
CODEX_INSTALL_MODEL="gpt-5.4-2026-03-05" \
  curl -fsSL https://github.com/SDGLBL/codex/releases/latest/download/install.sh | bash
```

The installer downloads the native release binary for your platform, installs `rg`, and bootstraps the internal Azure-backed `internal` profile. `ak`, the Azure base URL, and the optional default model are supplied at install time instead of being hardcoded in the repo. If `CODEX_INSTALL_MODEL` is unset, the installer writes `gpt-5.4-2026-03-05`. Linux defaults to the musl release assets.

You can also install with your preferred package manager:

```shell
# Install using npm
npm install -g @openai/codex
```

```shell
# Install using Homebrew
brew install --cask codex
```

Then simply run `codex` to get started.

<details>
<summary>You can also go to the <a href="https://github.com/SDGLBL/codex/releases/latest">latest GitHub Release</a> and download the appropriate binary for your platform.</summary>

Each GitHub Release contains many executables, but in practice, you likely want one of these:

- macOS
  - Apple Silicon/arm64: `codex-aarch64-apple-darwin.tar.gz`
  - x86_64 (older Mac hardware): `codex-x86_64-apple-darwin.tar.gz`
- Linux
  - x86_64: `codex-x86_64-unknown-linux-musl.tar.gz`

Each archive contains a single entry with the platform baked into the name (e.g., `codex-x86_64-unknown-linux-musl`), so you likely want to rename it to `codex` after extracting it.

</details>

### Using Codex with your ChatGPT plan

Run `codex` and select **Sign in with ChatGPT**. We recommend signing into your ChatGPT account to use Codex as part of your Plus, Pro, Business, Edu, or Enterprise plan. [Learn more about what's included in your ChatGPT plan](https://help.openai.com/en/articles/11369540-codex-in-chatgpt).

You can also use Codex with an API key, but this requires [additional setup](https://developers.openai.com/codex/auth#sign-in-with-an-api-key).

## Docs

- [**Codex Documentation**](https://developers.openai.com/codex)
- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Open source fund**](./docs/open-source-fund.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).

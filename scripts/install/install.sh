#!/bin/sh

set -eu

VERSION="${1:-latest}"
REPOSITORY="${CODEX_INSTALL_REPOSITORY:-SDGLBL/codex}"
RELEASE_BASE_URL="${CODEX_INSTALL_RELEASE_BASE_URL:-https://github.com/$REPOSITORY/releases/download}"
LATEST_RELEASE_URL="${CODEX_INSTALL_LATEST_RELEASE_URL:-https://api.github.com/repos/$REPOSITORY/releases/latest}"
LATEST_INSTALL_URL="${CODEX_INSTALL_LATEST_INSTALL_URL:-https://github.com/$REPOSITORY/releases/latest/download/install.sh}"
INSTALL_DIR=""
INSTALL_AK=""
INSTALL_AZURE_BASE_URL=""
path_action="already"
path_profile=""

step() {
  printf '==> %s\n' "$1"
}

normalize_version() {
  case "$1" in
    "" | latest)
      printf 'latest\n'
      ;;
    rust-v*)
      printf '%s\n' "${1#rust-v}"
      ;;
    v*)
      printf '%s\n' "${1#v}"
      ;;
    *)
      printf '%s\n' "$1"
      ;;
  esac
}

download_file() {
  url="$1"
  output="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O "$output" "$url"
    return
  fi

  echo "curl or wget is required to install Codex." >&2
  exit 1
}

download_text() {
  url="$1"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O - "$url"
    return
  fi

  echo "curl or wget is required to install Codex." >&2
  exit 1
}

resolve_version_from_latest_install_url() {
  if command -v curl >/dev/null 2>&1; then
    redirect_tag="$(curl -fsSL -D - -o /dev/null "$LATEST_INSTALL_URL" 2>/dev/null | sed -n 's/^[Ll]ocation: .*\/releases\/download\/\(rust-v[^/]*\)\/install\.sh.*/\1/p' | head -n 1 | tr -d '\r')"
    if [ -n "$redirect_tag" ]; then
      printf '%s\n' "${redirect_tag#rust-v}"
      return 0
    fi

    effective_url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "$LATEST_INSTALL_URL" 2>/dev/null || true)"
  elif command -v wget >/dev/null 2>&1; then
    redirect_tag="$(wget -q -O /dev/null --server-response "$LATEST_INSTALL_URL" 2>&1 | sed -n 's/^[[:space:]]*[Ll]ocation: .*\/releases\/download\/\(rust-v[^/]*\)\/install\.sh.*/\1/p' | head -n 1 | tr -d '\r')"
    if [ -n "$redirect_tag" ]; then
      printf '%s\n' "${redirect_tag#rust-v}"
      return 0
    fi

    effective_url="$(wget -q -O /dev/null --server-response "$LATEST_INSTALL_URL" 2>&1 | sed -n 's/^[[:space:]]*Location: //p' | tail -n 1 | tr -d '\r')"
    if [ -z "$effective_url" ]; then
      effective_url="$LATEST_INSTALL_URL"
    fi
  else
    effective_url=""
  fi

  if [ -z "$effective_url" ]; then
    return 1
  fi

  tag_candidate="${effective_url%/install.sh}"
  tag_candidate="${tag_candidate##*/}"
  case "$tag_candidate" in
    rust-v*)
      printf '%s\n' "${tag_candidate#rust-v}"
      return 0
      ;;
  esac

  return 1
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required to install Codex." >&2
    exit 1
  fi
}

require_command dirname
require_command mktemp
require_command tar

resolve_version() {
  normalized_version="$(normalize_version "$VERSION")"

  if [ "$normalized_version" != "latest" ]; then
    printf '%s\n' "$normalized_version"
    return
  fi

  release_json="$(download_text "$LATEST_RELEASE_URL" 2>/dev/null || true)"
  resolved="$(printf '%s\n' "$release_json" | sed -n 's/.*"tag_name":[[:space:]]*"rust-v\([^"]*\)".*/\1/p' | head -n 1)"

  if [ -n "$resolved" ]; then
    printf '%s\n' "$resolved"
    return
  fi

  resolved="$(resolve_version_from_latest_install_url || true)"

  if [ -z "$resolved" ]; then
    echo "Failed to resolve the latest Codex release version." >&2
    exit 1
  fi

  printf '%s\n' "$resolved"
}

release_url_for_asset() {
  asset="$1"
  resolved_version="$2"

  printf '%s/rust-v%s/%s\n' "${RELEASE_BASE_URL%/}" "$resolved_version" "$asset"
}

can_write_dir() {
  dir="$1"
  probe="$dir"

  while [ ! -e "$probe" ]; do
    parent="$(dirname "$probe")"
    if [ "$parent" = "$probe" ]; then
      break
    fi
    probe="$parent"
  done

  [ -d "$probe" ] && [ -w "$probe" ]
}

resolve_install_dir() {
  if [ -n "${CODEX_INSTALL_DIR:-}" ]; then
    printf '%s\n' "$CODEX_INSTALL_DIR"
    return
  fi

  existing_codex="$(command -v codex 2>/dev/null || true)"
  if [ -n "$existing_codex" ] && [ -f "$existing_codex" ]; then
    existing_dir="$(dirname "$existing_codex")"
    if can_write_dir "$existing_dir"; then
      printf '%s\n' "$existing_dir"
      return
    fi
  fi

  for candidate in "$HOME/.local/bin" "$HOME/bin"; do
    if can_write_dir "$candidate"; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  printf '%s\n' "$HOME/.local/bin"
}

add_to_path() {
  path_action="already"
  path_profile=""

  case ":$PATH:" in
    *":$INSTALL_DIR:"*)
      return
      ;;
  esac

  profile="$HOME/.profile"
  case "${SHELL:-}" in
    */zsh)
      profile="$HOME/.zshrc"
      ;;
    */bash)
      profile="$HOME/.bashrc"
      ;;
  esac

  path_profile="$profile"
  path_line="export PATH=\"$INSTALL_DIR:\$PATH\""
  if [ -f "$profile" ] && grep -F "$path_line" "$profile" >/dev/null 2>&1; then
    path_action="configured"
    return
  fi

  {
    printf '\n# Added by Codex installer\n'
    printf '%s\n' "$path_line"
  } >>"$profile"
  path_action="added"
}

prompt_for_install_config() {
  if [ -n "${CODEX_INSTALL_AK:-}" ]; then
    INSTALL_AK="$CODEX_INSTALL_AK"
  fi
  if [ -n "${CODEX_INSTALL_AZURE_BASE_URL:-}" ]; then
    INSTALL_AZURE_BASE_URL="$CODEX_INSTALL_AZURE_BASE_URL"
  fi

  if [ -n "$INSTALL_AK" ] && [ -n "$INSTALL_AZURE_BASE_URL" ]; then
    return
  fi

  if [ ! -r /dev/tty ] || [ ! -w /dev/tty ]; then
    echo "Non-interactive installs must set both CODEX_INSTALL_AK and CODEX_INSTALL_AZURE_BASE_URL, for example:" >&2
    echo "  CODEX_INSTALL_AK=... CODEX_INSTALL_AZURE_BASE_URL=... curl -fsSL https://github.com/SDGLBL/codex/releases/latest/download/install.sh | bash" >&2
    exit 1
  fi

  if [ -z "$INSTALL_AZURE_BASE_URL" ]; then
    printf 'Enter the internal Azure base URL: ' >/dev/tty
    IFS= read -r INSTALL_AZURE_BASE_URL </dev/tty || true
  fi

  if [ -z "$INSTALL_AK" ]; then
    old_stty=""
    if command -v stty >/dev/null 2>&1; then
      old_stty="$(stty -g </dev/tty 2>/dev/null || true)"
      stty -echo </dev/tty 2>/dev/null || true
    fi

    printf 'Enter ak for the internal Azure provider: ' >/dev/tty
    IFS= read -r INSTALL_AK </dev/tty || true

    if [ -n "$old_stty" ]; then
      stty "$old_stty" </dev/tty 2>/dev/null || true
    fi
    printf '\n' >/dev/tty
  fi

  if [ -z "$INSTALL_AK" ] || [ -z "$INSTALL_AZURE_BASE_URL" ]; then
    echo "A non-empty Azure base URL and ak are required to configure the internal profile." >&2
    exit 1
  fi
}

uname_s_value="${CODEX_INSTALL_UNAME_S:-$(uname -s)}"
uname_m_value="${CODEX_INSTALL_UNAME_M:-$(uname -m)}"

case "$uname_s_value" in
  Darwin)
    os="darwin"
    ;;
  Linux)
    os="linux"
    ;;
  *)
    echo "install.sh supports macOS and Linux. Use install.ps1 on Windows." >&2
    exit 1
    ;;
esac

case "$uname_m_value" in
  x86_64 | amd64)
    arch="x86_64"
    ;;
  arm64 | aarch64)
    arch="aarch64"
    ;;
  *)
    echo "Unsupported architecture: $uname_m_value" >&2
    exit 1
    ;;
esac

if [ "$os" = "darwin" ] && [ "$arch" = "x86_64" ]; then
  proc_translated="${CODEX_INSTALL_PROC_TRANSLATED:-$(sysctl -n sysctl.proc_translated 2>/dev/null || true)}"
  if [ "$proc_translated" = "1" ]; then
    arch="aarch64"
  fi
fi

if [ "$os" = "darwin" ]; then
  if [ "$arch" = "aarch64" ]; then
    vendor_target="aarch64-apple-darwin"
    platform_label="macOS (Apple Silicon)"
  else
    vendor_target="x86_64-apple-darwin"
    platform_label="macOS (Intel)"
  fi
else
  if [ "$arch" = "aarch64" ]; then
    echo "Linux (ARM64) is not currently published for the internal release installer." >&2
    exit 1
  else
    vendor_target="x86_64-unknown-linux-musl"
    platform_label="Linux (x64)"
  fi
fi

INSTALL_DIR="$(resolve_install_dir)"
if [ -x "$INSTALL_DIR/codex" ]; then
  install_mode="Updating"
else
  install_mode="Installing"
fi

step "$install_mode Codex CLI"
step "Detected platform: $platform_label"

resolved_version="$(resolve_version)"
native_asset="codex-$vendor_target.tar.gz"
rg_asset="rg-$vendor_target.tar.gz"
native_download_url="$(release_url_for_asset "$native_asset" "$resolved_version")"
rg_download_url="$(release_url_for_asset "$rg_asset" "$resolved_version")"

step "Resolved version: $resolved_version"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

native_archive_path="$tmp_dir/$native_asset"
rg_archive_path="$tmp_dir/$rg_asset"
native_extract_dir="$tmp_dir/native"
rg_extract_dir="$tmp_dir/rg"

mkdir -p "$native_extract_dir" "$rg_extract_dir"

step "Downloading Codex CLI"
download_file "$native_download_url" "$native_archive_path"

step "Downloading bundled rg"
download_file "$rg_download_url" "$rg_archive_path"

tar -xzf "$native_archive_path" -C "$native_extract_dir"
tar -xzf "$rg_archive_path" -C "$rg_extract_dir"

step "Installing to $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
cp "$native_extract_dir/codex-$vendor_target" "$INSTALL_DIR/codex"
cp "$rg_extract_dir/rg" "$INSTALL_DIR/rg"
chmod 0755 "$INSTALL_DIR/codex"
chmod 0755 "$INSTALL_DIR/rg"

prompt_for_install_config

step "Configuring internal profile"
printf '%s\n' "$INSTALL_AK" | "$INSTALL_DIR/codex" debug bootstrap-internal-profile --ak-stdin --azure-base-url "$INSTALL_AZURE_BASE_URL"

add_to_path

case "$path_action" in
  added)
    step "PATH updated for future shells in $path_profile"
    step "Run now: export PATH=\"$INSTALL_DIR:\$PATH\" && codex"
    step "Or open a new terminal and run: codex"
    ;;
  configured)
    step "PATH is already configured for future shells in $path_profile"
    step "Run now: export PATH=\"$INSTALL_DIR:\$PATH\" && codex"
    step "Or open a new terminal and run: codex"
    ;;
  *)
    step "$INSTALL_DIR is already on PATH"
    step "Run: codex"
    ;;
esac

printf 'Codex CLI %s installed successfully.\n' "$resolved_version"

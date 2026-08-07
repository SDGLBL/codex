#!/bin/sh

set -eu

RELEASE="${CODEX_RELEASE:-latest}"
REPOSITORY="${CODEX_INSTALL_REPOSITORY:-SDGLBL/codex}"
RELEASE_TAG_PREFIX="${CODEX_INSTALL_RELEASE_TAG_PREFIX:-internal-rust-v}"
RELEASE_TAG_OVERRIDE="${CODEX_INSTALL_RELEASE_TAG:-}"
RELEASE_BASE_URL="${CODEX_INSTALL_RELEASE_BASE_URL:-https://github.com/$REPOSITORY/releases/download}"
LATEST_RELEASE_URL="${CODEX_INSTALL_LATEST_RELEASE_URL:-https://api.github.com/repos/$REPOSITORY/releases/latest}"
LATEST_INSTALL_URL="${CODEX_INSTALL_LATEST_INSTALL_URL:-https://github.com/$REPOSITORY/releases/latest/download/install.sh}"
NON_INTERACTIVE="${CODEX_NON_INTERACTIVE:-false}"
if [ "$REPOSITORY" = "openai/codex" ]; then
  DEFAULT_PREFER_RELEASES_OPENAI_COM="true"
else
  DEFAULT_PREFER_RELEASES_OPENAI_COM="false"
fi
PREFER_RELEASES_OPENAI_COM="${CODEX_INSTALLER_USE_RELEASES_OPENAI_COM:-$DEFAULT_PREFER_RELEASES_OPENAI_COM}"
RELEASES_BASE_URL="https://releases.openai.com/codex"
RELEASES_CONNECT_TIMEOUT=10
RELEASES_METADATA_TIMEOUT=30
RELEASES_ASSET_TIMEOUT=300
release_source="github"
custom_release_base="false"
if [ -n "${CODEX_INSTALL_RELEASE_BASE_URL:-}" ]; then
  custom_release_base="true"
fi

BIN_DIR="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"
BIN_PATH="$BIN_DIR/codex"
CODE_MODE_HOST_BIN_PATH="$BIN_DIR/codex-code-mode-host"
RG_BIN_PATH="$BIN_DIR/rg"
CODEX_HOME_DIR="${CODEX_HOME:-$HOME/.codex}"
INSTALL_AK="${CODEX_INSTALL_AK:-}"
INSTALL_AZURE_BASE_URL="${CODEX_INSTALL_AZURE_BASE_URL:-}"
DEFAULT_INSTALL_MODEL="gpt-5.4-2026-03-05"
INSTALL_MODEL="${CODEX_INSTALL_MODEL:-$DEFAULT_INSTALL_MODEL}"
STANDALONE_ROOT="$CODEX_HOME_DIR/packages/standalone"
RELEASES_DIR="$STANDALONE_ROOT/releases"
CURRENT_LINK="$STANDALONE_ROOT/current"
LOCK_FILE="$STANDALONE_ROOT/install.lock"
LOCK_DIR="$STANDALONE_ROOT/install.lock.d"
LOCK_STALE_AFTER_SECS=600

path_action="already"
path_profile=""
conflict_manager=""
conflict_path=""
lock_kind=""
tmp_dir=""

step() {
  printf '==> %s\n' "$1"
}

warn() {
  printf 'WARNING: %s\n' "$1" >&2
}

normalize_version() {
  case "$1" in
    "" | latest)
      printf 'latest\n'
      ;;
    internal-rust-v*)
      printf '%s\n' "${1#internal-rust-v}"
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

tag_name_for_version() {
  printf '%s%s\n' "$RELEASE_TAG_PREFIX" "$1"
}

resolve_version_from_latest_install_url() {
  effective_url=""
  if command -v curl >/dev/null 2>&1; then
    redirect_tag="$(curl -fsSL -D - -o /dev/null "$LATEST_INSTALL_URL" 2>/dev/null |
      sed -n 's/^[Ll]ocation: .*\/releases\/download\/\([^/]*\)\/install\.sh.*/\1/p' |
      head -n 1 | tr -d '\r')"
    if [ -n "$redirect_tag" ]; then
      printf '%s\n' "$redirect_tag"
      return
    fi
    effective_url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "$LATEST_INSTALL_URL" 2>/dev/null || true)"
  elif command -v wget >/dev/null 2>&1; then
    effective_url="$(wget -q -O /dev/null --server-response "$LATEST_INSTALL_URL" 2>&1 |
      sed -n 's/^[[:space:]]*Location: //p' | tail -n 1 | tr -d '\r')"
    if [ -z "$effective_url" ]; then
      effective_url="$LATEST_INSTALL_URL"
    fi
  fi

  if [ -n "$effective_url" ]; then
    tag_candidate="${effective_url%/install.sh}"
    tag_candidate="${tag_candidate##*/}"
    if [ -n "$tag_candidate" ] && [ "$tag_candidate" != "download" ]; then
      printf '%s\n' "$tag_candidate"
      return
    fi
  fi
  return 1
}

validate_version() {
  version="$1"

  if [ "$version" = "latest" ]; then
    return
  fi

  if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-alpha(\.[0-9]+){0,2}|-beta(\.[0-9]+)?)?$'; then
    echo "Invalid Codex release version: $version. Expected latest or x.y.z[-alpha[.N[.M]]|-beta[.N]]." >&2
    return 1
  fi
}

parse_args() {
  positional_release=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --release)
        if [ "$#" -lt 2 ]; then
          echo "--release requires a value." >&2
          exit 1
        fi
        RELEASE="$2"
        shift
        ;;
      --help | -h)
        cat <<EOF
Usage: install.sh [VERSION] [--release VERSION]

Environment:
  CODEX_RELEASE          Version to install; overridden by --release.
  CODEX_INSTALL_REPOSITORY
                         GitHub repository containing internal release assets.
  CODEX_INSTALL_RELEASE_TAG
                         Exact internal release tag to install.
  CODEX_NON_INTERACTIVE  Set to 1, true, or yes to skip prompts.
  CODEX_INSTALLER_USE_RELEASES_OPENAI_COM
                         Set to 0, false, or no to use GitHub Releases.
EOF
        exit 0
        ;;
      *)
        if [ -n "$positional_release" ]; then
          echo "Unknown argument: $1" >&2
          exit 1
        fi
        positional_release="$1"
        RELEASE="$1"
        ;;
    esac
    shift
  done
}

download_file() {
  url="$1"
  output="$2"

  if command -v curl >/dev/null 2>&1; then
    case "$url" in
      "$RELEASES_BASE_URL"/*)
        curl -fsSL --connect-timeout "$RELEASES_CONNECT_TIMEOUT" --max-time "$RELEASES_ASSET_TIMEOUT" "$url" -o "$output"
        ;;
      *)
        curl -fsSL "$url" -o "$output"
        ;;
    esac
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    case "$url" in
      "$RELEASES_BASE_URL"/*)
        wget -q -t 1 -T "$RELEASES_ASSET_TIMEOUT" -O "$output" "$url"
        ;;
      *)
        wget -q -O "$output" "$url"
        ;;
    esac
    return
  fi

  echo "curl or wget is required to install Codex." >&2
  exit 1
}

download_text() {
  url="$1"

  if command -v curl >/dev/null 2>&1; then
    case "$url" in
      "$RELEASES_BASE_URL"/*)
        curl -fsSL --connect-timeout "$RELEASES_CONNECT_TIMEOUT" --max-time "$RELEASES_METADATA_TIMEOUT" "$url"
        ;;
      *)
        curl -fsSL "$url"
        ;;
    esac
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    case "$url" in
      "$RELEASES_BASE_URL"/*)
        wget -q -t 1 -T "$RELEASES_METADATA_TIMEOUT" -O - "$url"
        ;;
      *)
        wget -q -O - "$url"
        ;;
    esac
    return
  fi

  echo "curl or wget is required to install Codex." >&2
  exit 1
}

download_file_with_fallback() {
  primary_url="$1"
  fallback_url="$2"
  output="$3"
  expected_digest="$4"
  fallback_asset="$5"
  required_manifest_asset="${6:-}"

  if download_file "$primary_url" "$output" &&
    verify_archive_digest "$output" "$expected_digest" &&
    { [ -z "$required_manifest_asset" ] || package_archive_digest "$required_manifest_asset" "$output" >/dev/null; }; then
    return
  fi

  if [ -z "$fallback_url" ]; then
    return 1
  fi

  warn "Could not download or verify $primary_url; retrying from GitHub Releases."
  download_file "$fallback_url" "$output"
  if verify_archive_digest "$output" "$expected_digest" &&
    { [ -z "$required_manifest_asset" ] || package_archive_digest "$required_manifest_asset" "$output" >/dev/null; }; then
    return
  fi

  resolve_release_from_github "$resolved_version"
  fallback_digest="$(release_asset_digest "$fallback_asset")"
  verify_archive_digest "$output" "$fallback_digest"
  if [ -n "$required_manifest_asset" ]; then
    package_archive_digest "$required_manifest_asset" "$output" >/dev/null
  fi
}

parse_release_metadata() {
  # Bound awk's record size so compact, single-line JSON stays fast on every
  # supported awk implementation. JSON strings cannot contain literal newlines,
  # so the record boundaries inserted by fold do not change the document.
  LC_ALL=C fold -b -w 4096 | LC_ALL=C awk '
    function finish_string(value) {
      if (object_depth == 1 && key == "tag_name") {
        print "tag_name\t" value
      } else if (object_depth == asset_object_depth) {
        if (key == "name") {
          asset_name = value
        } else if (key == "digest") {
          asset_digest = value
        }
      }

      expecting_value = 0
      key = ""
    }

    {
      for (i = 1; i <= length($0); i++) {
        char = substr($0, i, 1)

        if (in_string) {
          if (escaped) {
            token = token "\\" char
            escaped = 0
          } else if (char == "\\") {
            escaped = 1
          } else if (char == "\"") {
            in_string = 0
            if (string_is_value) {
              finish_string(token)
            } else {
              pending_key = token
            }
          } else {
            token = token char
          }
          continue
        }

        if (char == "\"") {
          in_string = 1
          token = ""
          escaped = 0
          string_is_value = expecting_value
        } else if (char == ":" && pending_key != "") {
          key = pending_key
          pending_key = ""
          expecting_value = 1
        } else if (char == "{") {
          object_depth++
          if (assets_array_depth != 0 &&
              array_depth == assets_array_depth &&
              asset_object_depth == 0) {
            asset_object_depth = object_depth
            asset_name = ""
            asset_digest = ""
          }
          expecting_value = 0
          key = ""
        } else if (char == "}") {
          if (object_depth == asset_object_depth) {
            if (asset_name != "" && asset_digest != "") {
              print "asset\t" asset_name "\t" asset_digest
            }
            asset_object_depth = 0
            asset_name = ""
            asset_digest = ""
          }
          object_depth--
          expecting_value = 0
          key = ""
          pending_key = ""
        } else if (char == "[") {
          array_depth++
          if (expecting_value && key == "assets" && object_depth == 1) {
            assets_array_depth = array_depth
          }
          expecting_value = 0
          key = ""
        } else if (char == "]") {
          if (array_depth == assets_array_depth) {
            assets_array_depth = 0
          }
          array_depth--
          expecting_value = 0
          key = ""
          pending_key = ""
        } else if (char == ",") {
          expecting_value = 0
          key = ""
          pending_key = ""
        }
      }
    }

    END {
      if (in_string || object_depth != 0 || array_depth != 0) {
        exit 1
      }
    }
  '
}

release_url_for_asset() {
  asset="$1"
  resolved_tag="$2"

  printf '%s/%s/%s\n' "${RELEASE_BASE_URL%/}" "$resolved_tag" "$asset"
}

releases_url_for_asset() {
  asset="$1"
  resolved_version="$2"

  printf '%s/releases/%s/%s\n' "$RELEASES_BASE_URL" "$resolved_version" "$asset"
}

release_metadata_url() {
  resolved_tag="$1"

  printf 'https://api.github.com/repos/%s/releases/tags/%s\n' "$REPOSITORY" "$resolved_tag"
}

parse_downloaded_release_metadata() {
  requested_release="$1"
  source_name="$2"
  if ! release_metadata="$(printf '%s\n' "$release_json" | parse_release_metadata)"; then
    echo "Could not parse $source_name release metadata for Codex $requested_release." >&2
    return 1
  fi
}

resolve_metadata_version() {
  release_tag="$(printf '%s\n' "$release_metadata" | awk -F '\t' '$1 == "tag_name" { print $2; exit }')"
  case "$release_tag" in
    internal-rust-v*) metadata_version="${release_tag#internal-rust-v}" ;;
    rust-v*) metadata_version="${release_tag#rust-v}" ;;
    *) metadata_version="" ;;
  esac
  if [ -z "$metadata_version" ]; then
    echo "Failed to resolve the latest Codex release version." >&2
    return 1
  fi
  validate_version "$metadata_version"
}

resolve_release_from_github() {
  normalized_version="$1"
  if [ "$normalized_version" = "latest" ]; then
    requested_release="latest"
    metadata_url="$LATEST_RELEASE_URL"
  else
    resolved_version="$normalized_version"
    requested_release="$resolved_version"
    resolved_tag="${RELEASE_TAG_OVERRIDE:-$RELEASE_TAG_PREFIX$resolved_version}"
    metadata_url="$(release_metadata_url "$resolved_tag")"
  fi

  if ! release_json="$(download_text "$metadata_url")"; then
    echo "Could not fetch GitHub release metadata for Codex $requested_release. GitHub API may be unavailable or rate limited." >&2
    exit 1
  fi

  parse_downloaded_release_metadata "$requested_release" "GitHub"

  if [ "$normalized_version" = "latest" ]; then
    resolve_metadata_version
    resolved_version="$metadata_version"
    resolved_tag="$release_tag"
  else
    resolved_tag="${RELEASE_TAG_OVERRIDE:-$RELEASE_TAG_PREFIX$resolved_version}"
  fi

  release_source="github"
}

resolve_release_from_releases() {
  normalized_version="$1"

  if [ "$normalized_version" = "latest" ]; then
    requested_release="latest"
    metadata_url="$RELEASES_BASE_URL/channels/latest"
  else
    requested_release="$normalized_version"
    metadata_url="$RELEASES_BASE_URL/releases/$normalized_version/release.json"
  fi

  if ! release_json="$(download_text "$metadata_url")"; then
    return 1
  fi

  if ! parse_downloaded_release_metadata "$requested_release" "releases.openai.com"; then
    return 1
  fi
  if ! resolve_metadata_version; then
    return 1
  fi
  if [ "$normalized_version" != "latest" ] && [ "$metadata_version" != "$normalized_version" ]; then
    echo "Release metadata version did not match requested Codex version $normalized_version." >&2
    return 1
  fi
  resolved_version="$metadata_version"
  resolved_tag="rust-v$resolved_version"
  release_source="releases.openai.com"
}

resolve_custom_release() {
  if [ -n "$RELEASE_TAG_OVERRIDE" ]; then
    resolved_tag="$RELEASE_TAG_OVERRIDE"
    resolved_version="$(normalize_version "$resolved_tag")"
    release_source="custom"
    release_metadata=""
    select_release_assets
    return
  fi

  normalized_version="$(normalize_version "$RELEASE")"
  if [ "$normalized_version" = "latest" ]; then
    release_json="$(download_text "$LATEST_RELEASE_URL" 2>/dev/null || true)"
    release_tag="$(printf '%s\n' "$release_json" |
      sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    if [ -z "$release_tag" ]; then
      release_tag="$(resolve_version_from_latest_install_url || true)"
    fi
    if [ -z "$release_tag" ]; then
      echo "Failed to resolve the latest Codex release version." >&2
      exit 1
    fi
    resolved_tag="$release_tag"
    resolved_version="$(normalize_version "$resolved_tag")"
  else
    validate_version "$normalized_version"
    resolved_version="$normalized_version"
    resolved_tag="$(tag_name_for_version "$resolved_version")"
  fi

  release_source="custom"
  release_metadata=""
  select_release_assets
}

resolve_release() {
  if [ "$custom_release_base" = "true" ]; then
    resolve_custom_release
    return
  fi

  normalized_version="$(normalize_version "$RELEASE")"
  validate_version "$normalized_version"

  case "$PREFER_RELEASES_OPENAI_COM" in
    1 | [Tt][Rr][Uu][Ee] | [Yy][Ee][Ss])
      if resolve_release_from_releases "$normalized_version" &&
        select_release_assets; then
        return
      fi
      warn "releases.openai.com is unavailable; falling back to GitHub Releases."
      ;;
  esac

  resolve_release_from_github "$normalized_version"
  select_release_assets
}

release_asset_digest_or_empty() {
  asset="$1"

  digest="$(printf '%s\n' "$release_metadata" | awk -F '\t' -v asset="$asset" '
    $1 == "asset" && $2 == asset {
      print $3
      exit
    }
  ')"

  case "$digest" in
    sha256:????????????????????????????????????????????????????????????????)
      digest="${digest#sha256:}"
      case "$digest" in
        *[!0-9a-fA-F]*) return 1 ;;
      esac
      printf '%s\n' "$digest"
      ;;
    *)
      return 1
      ;;
  esac
}

release_asset_exists() {
  asset="$1"

  release_asset_digest_or_empty "$asset" >/dev/null 2>&1
}

release_asset_digest() {
  asset="$1"

  digest="$(release_asset_digest_or_empty "$asset" || true)"
  if [ -z "$digest" ]; then
    echo "Could not find SHA-256 digest for release asset $asset." >&2
    exit 1
  fi

  printf '%s\n' "$digest"
}

select_release_assets() {
  package_asset="codex-package-$vendor_target.tar.gz"
  checksum_asset="codex-package_SHA256SUMS"
  internal_codex_asset="codex-$vendor_target.tar.gz"
  internal_rg_asset="rg-$vendor_target.tar.gz"
  download_fallback_url=""
  checksum_fallback_url=""
  internal_rg_download_url=""

  if [ "$release_source" = "custom" ]; then
    install_layout="internal-raw"
    asset="$internal_codex_asset"
  elif release_asset_exists "$package_asset" &&
    release_asset_exists "$checksum_asset"; then
    install_layout="package"
    asset="$package_asset"
  elif release_asset_exists "codex-npm-$npm_tag-$resolved_version.tgz"; then
    install_layout="legacy-platform-npm"
    asset="codex-npm-$npm_tag-$resolved_version.tgz"
  elif release_asset_exists "$internal_codex_asset" &&
    release_asset_exists "$internal_rg_asset"; then
    install_layout="internal-raw"
    asset="$internal_codex_asset"
  else
    echo "Could not find Codex package or internal platform release assets for Codex $resolved_version." >&2
    return 1
  fi

  if [ "$release_source" = "releases.openai.com" ]; then
    download_url="$(releases_url_for_asset "$asset" "$resolved_version")"
    download_fallback_url="$(release_url_for_asset "$asset" "$resolved_tag")"
    if [ "$install_layout" = "package" ]; then
      checksum_url="$(releases_url_for_asset "$checksum_asset" "$resolved_version")"
      checksum_fallback_url="$(release_url_for_asset "$checksum_asset" "$resolved_tag")"
    fi
  else
    download_url="$(release_url_for_asset "$asset" "$resolved_tag")"
    if [ "$install_layout" = "package" ]; then
      checksum_url="$(release_url_for_asset "$checksum_asset" "$resolved_tag")"
    elif [ "$install_layout" = "internal-raw" ]; then
      internal_rg_download_url="$(release_url_for_asset "$internal_rg_asset" "$resolved_tag")"
    fi
  fi
}

package_archive_digest() {
  asset="$1"
  manifest_path="$2"

  digest="$(awk -v asset="$asset" '
    $2 == asset && length($1) == 64 && $1 !~ /[^0-9a-fA-F]/ {
      print tolower($1)
      found = 1
      exit
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' "$manifest_path" 2>/dev/null || true)"

  if [ -z "$digest" ]; then
    echo "Could not find SHA-256 digest for $asset in codex-package_SHA256SUMS." >&2
    return 1
  fi

  printf '%s\n' "$digest"
}

file_sha256() {
  path="$1"

  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
    return
  fi

  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
    return
  fi

  if command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$path" | sed 's/^.*= //'
    return
  fi

  echo "sha256sum, shasum, or openssl is required to verify the Codex download." >&2
  exit 1
}

verify_archive_digest() {
  archive_path="$1"
  expected_digest="$2"
  actual_digest="$(file_sha256 "$archive_path")"

  if [ "$actual_digest" != "$expected_digest" ]; then
    echo "Downloaded Codex archive checksum did not match expected digest." >&2
    echo "expected: $expected_digest" >&2
    echo "actual:   $actual_digest" >&2
    return 1
  fi
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required to install Codex." >&2
    exit 1
  fi
}

pick_profile() {
  # Use the same shell-specific split Homebrew documents because there is no
  # universal startup file across macOS/Linux login and interactive shells.
  case "$os:${SHELL:-}" in
    darwin:*/zsh)
      printf '%s\n' "$HOME/.zprofile"
      ;;
    darwin:*/bash)
      printf '%s\n' "$HOME/.bash_profile"
      ;;
    linux:*/zsh)
      printf '%s\n' "$HOME/.zshrc"
      ;;
    linux:*/bash)
      printf '%s\n' "$HOME/.bashrc"
      ;;
    *)
      printf '%s\n' "$HOME/.profile"
      ;;
  esac
}

add_to_path() {
  path_action="already"
  path_profile=""

  case ":$PATH:" in
    *":$BIN_DIR:"*)
      if [ -z "$conflict_manager" ]; then
        return
      fi
      ;;
  esac

  profile="$(pick_profile)"
  path_profile="$profile"
  begin_marker="# >>> Codex installer >>>"
  end_marker="# <<< Codex installer <<<"
  path_line="export PATH=\"$BIN_DIR:\$PATH\""

  if [ -f "$profile" ] && grep -F "$begin_marker" "$profile" >/dev/null 2>&1; then
    if grep -F "$path_line" "$profile" >/dev/null 2>&1; then
      path_action="configured"
      return
    fi

    if grep -F "$end_marker" "$profile" >/dev/null 2>&1; then
      rewrite_path_block "$profile" "$begin_marker" "$end_marker" "$path_line"
      path_action="updated"
      return
    fi
  fi

  append_path_block "$profile" "$begin_marker" "$end_marker" "$path_line"
  path_action="added"
}

append_path_block() {
  profile="$1"
  begin_marker="$2"
  end_marker="$3"
  path_line="$4"

  {
    printf '\n%s\n' "$begin_marker"
    printf '%s\n' "$path_line"
    printf '%s\n' "$end_marker"
  } >>"$profile"
}

rewrite_path_block() {
  profile="$1"
  begin_marker="$2"
  end_marker="$3"
  path_line="$4"
  tmp_profile="$tmp_dir/profile.$$.tmp"

  awk -v begin="$begin_marker" -v end="$end_marker" -v line="$path_line" '
    BEGIN {
      in_block = 0
      replaced = 0
    }
    $0 == begin {
      if (!replaced) {
        print begin
        print line
        print end
        replaced = 1
      }
      in_block = 1
      next
    }
    in_block {
      if ($0 == end) {
        in_block = 0
      }
      next
    }
    {
      print
    }
    END {
      if (in_block != 0) {
        exit 1
      }
    }
  ' "$profile" >"$tmp_profile"
  mv "$tmp_profile" "$profile"
}

mkdir_lock_is_stale() {
  [ -d "$LOCK_DIR" ] || return 1

  pid="$(cat "$LOCK_DIR/pid" 2>/dev/null || true)"
  started_at="$(cat "$LOCK_DIR/started_at" 2>/dev/null || true)"
  now="$(date +%s 2>/dev/null || printf '0')"

  case "$started_at" in
    ''|*[!0-9]*)
      started_at=0
      ;;
  esac

  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    return 1
  fi

  if [ "$started_at" -eq 0 ] || [ "$now" -eq 0 ]; then
    return 0
  fi

  [ $((now - started_at)) -ge "$LOCK_STALE_AFTER_SECS" ]
}

acquire_install_lock() {
  mkdir -p "$STANDALONE_ROOT"

  if [ "$os" = "darwin" ] && command -v lockf >/dev/null 2>&1; then
    : >>"$LOCK_FILE"
    exec 9<>"$LOCK_FILE"
    lockf 9
    lock_kind="lockf"
    return
  fi

  if command -v flock >/dev/null 2>&1; then
    exec 9>"$LOCK_FILE"
    flock 9
    lock_kind="flock"
    return
  fi

  while ! mkdir "$LOCK_DIR" 2>/dev/null; do
    if mkdir_lock_is_stale; then
      warn "Removing stale installer lock at $LOCK_DIR"
      rm -rf "$LOCK_DIR"
      continue
    fi
    sleep 1
  done

  printf '%s\n' "$$" >"$LOCK_DIR/pid"
  date +%s >"$LOCK_DIR/started_at" 2>/dev/null || true
  lock_kind="mkdir"
}

release_install_lock() {
  if [ "$lock_kind" = "mkdir" ]; then
    rm -rf "$LOCK_DIR" 2>/dev/null || true
  elif [ "$lock_kind" = "flock" ] || [ "$lock_kind" = "lockf" ]; then
    exec 9>&- 2>/dev/null || true
  fi
  lock_kind=""
}

cleanup_stale_install_artifacts() {
  mkdir -p "$RELEASES_DIR" "$STANDALONE_ROOT"

  find "$RELEASES_DIR" -mindepth 1 -maxdepth 1 -name '.staging.*' -exec rm -rf {} +
  find "$STANDALONE_ROOT" -mindepth 1 -maxdepth 1 -name '.current.*' -exec rm -f {} +

  if [ -d "$BIN_DIR" ]; then
    find "$BIN_DIR" -mindepth 1 -maxdepth 1 -name '.codex.*' -exec rm -f {} +
  fi
}

replace_path_with_symlink() {
  link_path="$1"
  link_target="$2"
  tmp_link="$3"

  rm -f "$tmp_link"
  ln -s "$link_target" "$tmp_link"

  if mv -Tf "$tmp_link" "$link_path" 2>/dev/null; then
    return
  fi

  if mv -hf "$tmp_link" "$link_path" 2>/dev/null; then
    return
  fi

  rm -f "$link_path"
  mv -f "$tmp_link" "$link_path"
}

version_from_binary() {
  codex_path="$1"

  if [ ! -x "$codex_path" ]; then
    return 1
  fi

  "$codex_path" --version 2>/dev/null | sed -n 's/.* \([0-9][0-9A-Za-z.+-]*\)$/\1/p' | head -n 1
}

current_installed_version() {
  version="$(version_from_binary "$CURRENT_LINK/bin/codex" || true)"
  if [ -n "$version" ]; then
    printf '%s\n' "$version"
    return 0
  fi

  version="$(version_from_binary "$CURRENT_LINK/codex" || true)"
  if [ -n "$version" ]; then
    printf '%s\n' "$version"
    return 0
  fi

  return 0
}

resolve_existing_codex() {
  command -v codex 2>/dev/null || true
}

reuse_unmanaged_internal_install_dir() {
  if [ -n "${CODEX_INSTALL_DIR:-}" ] || [ "$REPOSITORY" = "openai/codex" ]; then
    return
  fi

  existing_path="$(resolve_existing_codex)"
  if [ -z "$existing_path" ] ||
    [ ! -f "$existing_path" ] ||
    [ -n "$(classify_existing_codex "$existing_path" || true)" ]; then
    return
  fi

  existing_dir="$(dirname "$existing_path")"
  if [ ! -w "$existing_dir" ]; then
    return
  fi

  BIN_DIR="$existing_dir"
  BIN_PATH="$BIN_DIR/codex"
  CODE_MODE_HOST_BIN_PATH="$BIN_DIR/codex-code-mode-host"
  RG_BIN_PATH="$BIN_DIR/rg"
}

classify_existing_codex() {
  existing_path="$1"

  if [ -z "$existing_path" ] || [ "$existing_path" = "$BIN_PATH" ]; then
    return 1
  fi

  case "$existing_path" in
    /opt/homebrew/* | /usr/local/*)
      if [ "$os" = "darwin" ]; then
        printf 'brew\n'
        return 0
      fi
      ;;
  esac

  if [ -f "$existing_path" ] && grep -F "#!/usr/bin/env node" "$existing_path" >/dev/null 2>&1; then
    case "$existing_path" in
      *".bun"*)
        printf 'bun\n'
        ;;
      *)
        printf 'npm\n'
        ;;
    esac
    return 0
  fi

  return 1
}

prompt_yes_no() {
  prompt="$1"

  case "$NON_INTERACTIVE" in
    1 | [Tt][Rr][Uu][Ee] | [Yy][Ee][Ss])
      return 1
      ;;
  esac

  if ( : </dev/tty ) 2>/dev/null; then
    printf '%s [y/N] ' "$prompt" >/dev/tty
    if ! IFS= read -r answer </dev/tty; then
      return 1
    fi
  elif [ -t 0 ]; then
    printf '%s [y/N] ' "$prompt"
    if ! IFS= read -r answer; then
      return 1
    fi
  else
    return 1
  fi

  case "$answer" in
    y | Y | yes | YES)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

print_launch_instructions() {
  case "$path_action" in
    added)
      step "Current terminal: export PATH=\"$BIN_DIR:\$PATH\" && codex"
      step "Future terminals: open a new terminal and run: codex"
      step "PATH was added to $path_profile"
      ;;
    updated)
      step "Current terminal: export PATH=\"$BIN_DIR:\$PATH\" && codex"
      step "Future terminals: open a new terminal and run: codex"
      step "PATH was updated in $path_profile"
      ;;
    configured)
      step "Current terminal: export PATH=\"$BIN_DIR:\$PATH\" && codex"
      step "Future terminals: open a new terminal and run: codex"
      step "PATH is already configured in $path_profile"
      ;;
    *)
      step "Current terminal: codex"
      step "Future terminals: open a new terminal and run: codex"
      ;;
  esac
}

maybe_launch_codex_now() {
  if prompt_yes_no "Start Codex now?"; then
    step "Launching Codex"
    "$BIN_PATH"
  fi
}

detect_conflicting_install() {
  existing_path="$(resolve_existing_codex)"
  manager="$(classify_existing_codex "$existing_path" || true)"

  if [ -z "$manager" ]; then
    return
  fi

  conflict_manager="$manager"
  conflict_path="$existing_path"
  step "Detected existing $manager-managed Codex at $existing_path"
  warn "Multiple managed Codex installs can be ambiguous because PATH order decides which one runs."
}

handle_conflicting_install() {
  if [ -z "$conflict_manager" ]; then
    return
  fi

  case "$conflict_manager" in
    brew)
      uninstall_cmd="brew uninstall --cask codex"
      ;;
    bun)
      uninstall_cmd="bun remove -g @openai/codex"
      ;;
    *)
      uninstall_cmd="npm uninstall -g @openai/codex"
      ;;
  esac

  if prompt_yes_no "Uninstall the existing $conflict_manager-managed Codex now?"; then
    step "Running: $uninstall_cmd"
    if ! sh -c "$uninstall_cmd"; then
      warn "Failed to uninstall the existing $conflict_manager-managed Codex. Continuing with the standalone install."
    fi
  else
    warn "Leaving the existing $conflict_manager-managed Codex installed. PATH order will determine which codex runs."
  fi
}

install_package_release() {
  release_dir="$1"
  archive_path="$2"
  stage_release="$RELEASES_DIR/.staging.$(basename "$release_dir").$$"

  mkdir -p "$RELEASES_DIR"
  rm -rf "$stage_release"
  mkdir -p "$stage_release"
  tar -xzf "$archive_path" -C "$stage_release"
  chmod 0755 \
    "$stage_release/bin/codex" \
    "$stage_release/bin/codex-code-mode-host" \
    "$stage_release/codex-path/rg"
  if [ -f "$stage_release/codex-resources/bwrap" ]; then
    chmod 0755 "$stage_release/codex-resources/bwrap"
  fi
  ln -sf "bin/codex" "$stage_release/codex"

  if [ -e "$release_dir" ] || [ -L "$release_dir" ]; then
    rm -rf "$release_dir"
  fi
  mv "$stage_release" "$release_dir"
}

install_legacy_platform_npm_release() {
  release_dir="$1"
  archive_path="$2"
  target="$3"
  stage_release="$RELEASES_DIR/.staging.$(basename "$release_dir").$$"
  extract_dir="$tmp_dir/extract"
  vendor_root="$extract_dir/package/vendor/$target"

  mkdir -p "$RELEASES_DIR"
  rm -rf "$stage_release" "$extract_dir"
  mkdir -p "$stage_release/codex-resources" "$extract_dir"
  tar -xzf "$archive_path" -C "$extract_dir"

  cp "$vendor_root/codex/codex" "$stage_release/codex"
  cp "$vendor_root/path/rg" "$stage_release/codex-resources/rg"
  chmod 0755 "$stage_release/codex" "$stage_release/codex-resources/rg"
  if [ -f "$vendor_root/codex-resources/bwrap" ]; then
    cp "$vendor_root/codex-resources/bwrap" "$stage_release/codex-resources/bwrap"
    chmod 0755 "$stage_release/codex-resources/bwrap"
  fi

  if [ -e "$release_dir" ] || [ -L "$release_dir" ]; then
    rm -rf "$release_dir"
  fi
  mv "$stage_release" "$release_dir"
}

install_internal_raw_release() {
  release_dir="$1"
  codex_archive_path="$2"
  rg_archive_path="$3"
  stage_release="$RELEASES_DIR/.staging.$(basename "$release_dir").$$"

  mkdir -p "$RELEASES_DIR"
  rm -rf "$stage_release"
  mkdir -p "$stage_release/bin" "$stage_release/codex-path"
  tar -xzf "$codex_archive_path" -C "$stage_release/bin"
  tar -xzf "$rg_archive_path" -C "$stage_release/codex-path"
  chmod 0755 \
    "$stage_release/bin/codex" \
    "$stage_release/bin/codex-code-mode-host" \
    "$stage_release/codex-path/rg"
  ln -sf "bin/codex" "$stage_release/codex"

  if [ -e "$release_dir" ] || [ -L "$release_dir" ]; then
    rm -rf "$release_dir"
  fi
  mv "$stage_release" "$release_dir"
}

release_dir_is_complete() {
  release_dir="$1"
  expected_version="$2"
  expected_target="$3"
  layout="$4"

  [ -d "$release_dir" ] &&
    [ "$(basename "$release_dir")" = "$expected_version-$expected_target" ] ||
    return 1

  case "$layout" in
    package)
      [ -f "$release_dir/codex-package.json" ] &&
        [ -x "$release_dir/bin/codex" ] &&
        [ -x "$release_dir/bin/codex-code-mode-host" ] &&
        [ -x "$release_dir/codex" ] &&
        [ -x "$release_dir/codex-path/rg" ] ||
        return 1
      ;;
    legacy-platform-npm)
      [ -x "$release_dir/codex" ] &&
        [ -x "$release_dir/codex-resources/rg" ] ||
        return 1
      ;;
    internal-raw)
      [ -x "$release_dir/bin/codex" ] &&
        [ -x "$release_dir/bin/codex-code-mode-host" ] &&
        [ -x "$release_dir/codex" ] &&
        [ -x "$release_dir/codex-path/rg" ] ||
        return 1
      ;;
    *)
      return 1
      ;;
  esac

  case "$layout:$expected_target" in
    package:*linux* | legacy-platform-npm:*linux*)
      [ -x "$release_dir/codex-resources/bwrap" ] || return 1
      ;;
  esac

  if [ "$release_source" = "custom" ]; then
    return 0
  fi

  installed_version="$(version_from_binary "$release_dir/bin/codex" || version_from_binary "$release_dir/codex" || true)"
  [ "$installed_version" = "$expected_version" ]
}

update_current_link() {
  release_dir="$1"
  tmp_link="$STANDALONE_ROOT/.current.$$"

  replace_path_with_symlink "$CURRENT_LINK" "$release_dir" "$tmp_link"
}

release_codex_relative_path() {
  release_dir="$1"

  if [ -x "$release_dir/bin/codex" ]; then
    printf 'bin/codex\n'
  else
    printf 'codex\n'
  fi
}

update_visible_command() {
  release_dir="$1"
  mkdir -p "$BIN_DIR"
  tmp_link="$BIN_DIR/.codex.$$"
  codex_relative_path="$(release_codex_relative_path "$release_dir")"

  replace_path_with_symlink "$BIN_PATH" "$CURRENT_LINK/$codex_relative_path" "$tmp_link"

  if { [ "$os" = "darwin" ] || [ "$install_layout" = "internal-raw" ]; } &&
    [ -x "$release_dir/bin/codex-code-mode-host" ]; then
    replace_path_with_symlink \
      "$CODE_MODE_HOST_BIN_PATH" \
      "$CURRENT_LINK/bin/codex-code-mode-host" \
      "$tmp_link"
  elif [ "$(readlink "$CODE_MODE_HOST_BIN_PATH" 2>/dev/null || true)" = \
    "$CURRENT_LINK/bin/codex-code-mode-host" ]; then
    rm -f "$CODE_MODE_HOST_BIN_PATH"
  fi

  if [ "$install_layout" = "internal-raw" ]; then
    replace_path_with_symlink "$RG_BIN_PATH" "$CURRENT_LINK/codex-path/rg" "$tmp_link"
  fi
}

verify_visible_command() {
  "$BIN_PATH" --version >/dev/null
  if { [ "$os" = "darwin" ] && [ "$install_layout" = "package" ]; } ||
    [ "$install_layout" = "internal-raw" ]; then
    [ -x "$CODE_MODE_HOST_BIN_PATH" ]
  fi
}

warn_if_crawl_url() {
  case "$1" in
    */v2/crawl | */v2/crawl/)
      warn "CODEX_INSTALL_AZURE_BASE_URL ends with /v2/crawl. GPT models use the responses API, so this should point at the openapi base URL, not /v2/crawl."
      ;;
  esac
}

prompt_for_install_config() {
  if [ -n "$INSTALL_AK" ] && [ -n "$INSTALL_AZURE_BASE_URL" ]; then
    warn_if_crawl_url "$INSTALL_AZURE_BASE_URL"
    return
  fi

  if [ ! -r /dev/tty ] || [ ! -w /dev/tty ] || ! { printf '' >/dev/tty; } 2>/dev/null; then
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
    echo "A non-empty Azure base URL and ak are required to configure Codex." >&2
    exit 1
  fi
  warn_if_crawl_url "$INSTALL_AZURE_BASE_URL"
}

toml_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_install_config() {
  mkdir -p "$CODEX_HOME_DIR"
  config_path="$CODEX_HOME_DIR/config.toml"
  model_escaped="$(toml_escape "$INSTALL_MODEL")"
  base_url_escaped="$(toml_escape "$INSTALL_AZURE_BASE_URL")"
  ak_escaped="$(toml_escape "$INSTALL_AK")"

  cat >"$config_path" <<EOF
model = "$model_escaped"
model_provider = "azure"
sandbox_mode = "danger-full-access"
approval_policy = "on-request"
model_reasoning_effort = "xhigh"
plan_mode_reasoning_effort = "xhigh"
model_max_output_tokens = 64000
background_terminal_max_timeout = 72000000
project_doc_max_bytes = 65536
suppress_unstable_features_warning = true

[shell_environment_policy]
inherit = "all"
ignore_default_excludes = true

[features]
apps = false
guardian_approval = false
prevent_idle_sleep = true
tui_app_server = false
hooks = true
multi_agent = true
voice_transcription = false
enable_fanout = true
goals = true
remote_connections = true
js_repl = false

[agents]
max_threads = 8
max_depth = 1

[tui]
theme = "catppuccin-latte"
notification_method = "auto"
notifications = ["agent-turn-complete", "approval-requested"]

[model_providers.azure]
name = "Azure"
base_url = "$base_url_escaped"
wire_api = "responses"
request_max_retries = 50
retry_429 = true
stream_max_retries = 50

[model_providers.azure.query_params]
api-version = "2025-04-01-preview"
ak = "$ak_escaped"
EOF
  step "Configured config.toml. Run \`codex\` to use the internal Azure provider."
}

parse_args "$@"

require_command mktemp
require_command tar
require_command dirname

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
    npm_tag="darwin-arm64"
    vendor_target="aarch64-apple-darwin"
    platform_label="macOS (Apple Silicon)"
  else
    npm_tag="darwin-x64"
    vendor_target="x86_64-apple-darwin"
    platform_label="macOS (Intel)"
  fi
else
  if [ "$arch" = "aarch64" ]; then
    if [ "$REPOSITORY" = "SDGLBL/codex" ] || [ "$custom_release_base" = "true" ]; then
      echo "Linux (ARM64) is not currently published for the internal release installer." >&2
      exit 1
    fi
    npm_tag="linux-arm64"
    vendor_target="aarch64-unknown-linux-musl"
    platform_label="Linux (ARM64)"
  else
    npm_tag="linux-x64"
    vendor_target="x86_64-unknown-linux-musl"
    platform_label="Linux (x64)"
  fi
fi

reuse_unmanaged_internal_install_dir
resolve_release
release_name="$resolved_version-$vendor_target"
release_dir="$RELEASES_DIR/$release_name"
current_version="$(current_installed_version)"

if [ -n "$current_version" ] && [ "$current_version" != "$resolved_version" ]; then
  step "Updating Codex CLI from $current_version to $resolved_version"
elif [ -n "$current_version" ]; then
  step "Updating Codex CLI"
else
  step "Installing Codex CLI"
fi
step "Detected platform: $platform_label"
step "Resolved version: $resolved_version"

detect_conflicting_install

tmp_dir="$(mktemp -d)"
cleanup() {
  release_install_lock
  if [ -n "$tmp_dir" ]; then
    rm -rf "$tmp_dir"
  fi
}
trap cleanup EXIT INT TERM

acquire_install_lock
cleanup_stale_install_artifacts

if ! release_dir_is_complete "$release_dir" "$resolved_version" "$vendor_target" "$install_layout"; then
  if [ -e "$release_dir" ] || [ -L "$release_dir" ]; then
    warn "Found incomplete existing release at $release_dir; reinstalling."
  fi

  archive_path="$tmp_dir/$asset"
  checksum_path="$tmp_dir/$checksum_asset"

  step "Downloading Codex CLI"
  if [ "$install_layout" = "package" ]; then
    checksum_digest="$(release_asset_digest "$checksum_asset")"
    download_file_with_fallback "$checksum_url" "$checksum_fallback_url" "$checksum_path" "$checksum_digest" "$checksum_asset" "$asset"
    expected_digest="$(package_archive_digest "$asset" "$checksum_path")"
  elif [ "$install_layout" = "internal-raw" ]; then
    internal_rg_archive_path="$tmp_dir/$internal_rg_asset"
    if [ "$release_source" = "custom" ]; then
      download_file "$internal_rg_download_url" "$internal_rg_archive_path"
    else
      internal_rg_digest="$(release_asset_digest "$internal_rg_asset")"
      download_file "$internal_rg_download_url" "$internal_rg_archive_path"
      verify_archive_digest "$internal_rg_archive_path" "$internal_rg_digest"
    fi
  else
    expected_digest="$(release_asset_digest "$asset")"
  fi
  if [ "$install_layout" = "internal-raw" ] && [ "$release_source" = "custom" ]; then
    download_file "$download_url" "$archive_path"
  else
    if [ "$install_layout" = "internal-raw" ]; then
      expected_digest="$(release_asset_digest "$asset")"
    fi
    download_file_with_fallback "$download_url" "$download_fallback_url" "$archive_path" "$expected_digest" "$asset"
  fi

  step "Installing standalone package to $release_dir"
  if [ "$install_layout" = "package" ]; then
    install_package_release "$release_dir" "$archive_path"
  elif [ "$install_layout" = "internal-raw" ]; then
    install_internal_raw_release "$release_dir" "$archive_path" "$internal_rg_archive_path"
  else
    install_legacy_platform_npm_release "$release_dir" "$archive_path" "$vendor_target"
  fi
fi
if ! release_dir_is_complete "$release_dir" "$resolved_version" "$vendor_target" "$install_layout"; then
  echo "Installed Codex command did not report expected version $resolved_version." >&2
  exit 1
fi
update_current_link "$release_dir"
update_visible_command "$release_dir"
add_to_path
verify_visible_command
if [ "$REPOSITORY" != "openai/codex" ] ||
  [ -n "$INSTALL_AK" ] ||
  [ -n "$INSTALL_AZURE_BASE_URL" ]; then
  prompt_for_install_config
  step "Configuring config.toml"
  write_install_config
fi
release_install_lock
handle_conflicting_install

case "$path_action" in
  added)
    print_launch_instructions
    ;;
  updated)
    print_launch_instructions
    ;;
  configured)
    print_launch_instructions
    ;;
  *)
    step "$BIN_DIR is already on PATH"
    print_launch_instructions
    ;;
esac

printf 'Codex CLI %s installed successfully.\n' "$resolved_version"
maybe_launch_codex_now

#!/usr/bin/env sh
# rgctl one-line installer — download the latest GitHub Release binary.
# Usage: curl -fsSL https://shaaf.dev/rgctl/install.sh | sh
# Override install dir: RGCTL_INSTALL_DIR=/usr/local/bin curl -fsSL ... | sh
set -eu

REPO="${RGCTL_REPO:-sshaaf/rgctl}"
INSTALL_DIR="${RGCTL_INSTALL_DIR:-${HOME}/.local/bin}"
API="https://api.github.com/repos/${REPO}/releases/latest"

os="$(uname -s)"
arch="$(uname -m)"
case "${os}" in
  Darwin) os_tag="Darwin" ;;
  Linux) os_tag="Linux" ;;
  *)
    echo "error: unsupported OS '${os}' (need Darwin or Linux)" >&2
    exit 1
    ;;
esac
case "${arch}" in
  x86_64|amd64) arch_tag="x86_64" ;;
  arm64|aarch64) arch_tag="aarch64" ;;
  *)
    echo "error: unsupported arch '${arch}' (need x86_64 or arm64)" >&2
    exit 1
    ;;
esac

# Cargo-dist / release assets are typically: rgctl-<version>-<target>.tar.gz
# Prefer a matching target triple; fall back to name patterns with OS/arch.
need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: required command not found: $1" >&2
    exit 1
  }
}
need_cmd curl
need_cmd tar
need_cmd mktemp

json="$(curl -fsSL -H "Accept: application/vnd.github+json" "${API}")"
# Extract browser_download_url lines (portable-ish without jq).
urls="$(printf '%s' "${json}" | tr '"' '\n' | grep -E '^https://github.com/.*/releases/download/.*\\.tar\\.gz$' || true)"
if [ -z "${urls}" ]; then
  echo "error: could not list release assets from ${API}" >&2
  exit 1
fi

pick=""
for u in ${urls}; do
  case "${u}" in
    *"${os_tag}"*"${arch_tag}"*|*"${arch_tag}"*"${os_tag}"*|*"$(echo "${os_tag}" | tr '[:upper:]' '[:lower:]')"*"${arch_tag}"*)
      pick="${u}"
      break
      ;;
  esac
done
# Apple Silicon often tagged aarch64-apple-darwin; x86_64-apple-darwin / x86_64-unknown-linux-gnu
if [ -z "${pick}" ]; then
  case "${os_tag}-${arch_tag}" in
    Darwin-aarch64) needle="aarch64-apple-darwin" ;;
    Darwin-x86_64) needle="x86_64-apple-darwin" ;;
    Linux-aarch64) needle="aarch64-unknown-linux-gnu" ;;
    Linux-x86_64) needle="x86_64-unknown-linux-gnu" ;;
    *) needle="" ;;
  esac
  if [ -n "${needle}" ]; then
    for u in ${urls}; do
      case "${u}" in
        *"${needle}"*) pick="${u}"; break ;;
      esac
    done
  fi
fi
if [ -z "${pick}" ]; then
  echo "error: no matching .tar.gz asset for ${os_tag}/${arch_tag}" >&2
  echo "available:" >&2
  printf '%s\n' "${urls}" >&2
  exit 1
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT INT HUP
archive="${tmpdir}/rgctl.tgz"
echo "Downloading ${pick}"
curl -fsSL -o "${archive}" "${pick}"
tar -xzf "${archive}" -C "${tmpdir}"

bin=""
for candidate in \
  "${tmpdir}/rgctl" \
  "${tmpdir}/rgctl/rgctl" \
  "$(find "${tmpdir}" -type f -name rgctl 2>/dev/null | head -n 1)"; do
  if [ -n "${candidate}" ] && [ -f "${candidate}" ]; then
    bin="${candidate}"
    break
  fi
done
if [ -z "${bin}" ]; then
  echo "error: archive did not contain an rgctl binary" >&2
  exit 1
fi

mkdir -p "${INSTALL_DIR}"
install -m 755 "${bin}" "${INSTALL_DIR}/rgctl"
echo "Installed ${INSTALL_DIR}/rgctl"
if ! command -v rgctl >/dev/null 2>&1; then
  echo "Add to PATH: export PATH=\"${INSTALL_DIR}:\$PATH\""
fi
"${INSTALL_DIR}/rgctl" --version || true

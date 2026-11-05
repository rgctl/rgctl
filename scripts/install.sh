#!/usr/bin/env sh
# rgctl one-line installer — download the latest GitHub Release binary.
# Usage: curl -fsSL https://rgctl.io/install.sh | sh
# Override install dir: RGCTL_INSTALL_DIR=/usr/local/bin curl -fsSL ... | sh
set -eu

REPO="${RGCTL_REPO:-rgctl/rgctl}"
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

# Release assets are named: rgctl-<version>-<target>.tar.gz.
need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: required command not found: $1" >&2
    exit 1
  }
}
need_cmd curl
need_cmd tar
need_cmd mktemp
need_cmd grep

libc_tag=""
if [ "${os_tag}" = "Linux" ]; then
  if [ -f /etc/alpine-release ] || \
    { command -v ldd >/dev/null 2>&1 && ldd --version 2>&1 | grep -qi musl; }; then
    libc_tag="musl"
  else
    libc_tag="gnu"
  fi
fi

case "${os_tag}-${arch_tag}-${libc_tag}" in
  Darwin-aarch64-) target="aarch64-apple-darwin" ;;
  Darwin-x86_64-) target="x86_64-apple-darwin" ;;
  Linux-aarch64-gnu) target="aarch64-unknown-linux-gnu" ;;
  Linux-aarch64-musl) target="aarch64-unknown-linux-musl" ;;
  Linux-x86_64-gnu) target="x86_64-unknown-linux-gnu" ;;
  Linux-x86_64-musl) target="x86_64-unknown-linux-musl" ;;
  *)
    echo "error: could not determine a release target for ${os_tag}/${arch_tag}" >&2
    exit 1
    ;;
esac

json="$(curl -fsSL -H "Accept: application/vnd.github+json" "${API}")"
# Extract browser_download_url lines (portable-ish without jq).
urls="$(printf '%s' "${json}" | tr '"' '\n' | grep -E '^https://github.com/.*/releases/download/.*\.tar\.gz$' || true)"
if [ -z "${urls}" ]; then
  echo "error: could not list release assets from ${API}" >&2
  exit 1
fi

pick=""
for u in ${urls}; do
  case "${u}" in
    *"${target}"*)
      pick="${u}"
      break
      ;;
  esac
done
if [ -z "${pick}" ]; then
  echo "error: no matching .tar.gz asset for ${target}" >&2
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

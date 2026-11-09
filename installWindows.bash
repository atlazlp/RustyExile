#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

on_windows=0
case "${OSTYPE:-}" in
  msys*|cygwin*) on_windows=1 ;;
esac
if [[ -n "${WINDIR:-}" ]]; then
  on_windows=1
fi
if [[ "$on_windows" -ne 1 ]]; then
  echo "This installer is for Windows Git Bash. On Linux, run ./installLinux.sh"
  exit 1
fi

if [[ ! -f "$ROOT/crates/poe-app/Cargo.toml" ]] || ! grep -q 'name = "RustyExile"' "$ROOT/crates/poe-app/Cargo.toml"; then
  echo "Run installWindows.bash from the RustyExile folder."
  exit 1
fi

echo "RustyExile setup"
echo "It builds the overlay and can add a Start Menu shortcut."
echo

ask() {
  local prompt="$1"
  local default="${2:-y}"
  local reply
  if [[ ! -t 0 ]] && ! read -r -t 0; then
    [[ "$default" == y ]]
    return
  fi
  if [[ "$default" == y ]]; then
    prompt="$prompt [Y/n] "
  else
    prompt="$prompt [y/N] "
  fi
  read -r -p "$prompt" reply || true
  reply="${reply:-$default}"
  [[ "$reply" =~ ^[Yy]([Ee][Ss])?$ ]]
}

win_path() {
  cygpath -w "$1"
}

ensure_cargo_path() {
  if command -v cargo >/dev/null 2>&1; then
    return
  fi
  if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
  if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi
}

install_rust() {
  local init="$ROOT/rustup-init.exe"
  echo "Downloading the Rust installer."
  if command -v curl >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -fL "https://win.rustup.rs/x86_64" -o "$init"
  else
    powershell.exe -NoProfile -Command "Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile '$(win_path "$init")'"
  fi
  echo "Installing Rust. Accept the defaults if a window asks."
  powershell.exe -NoProfile -Command "& '$(win_path "$init")' -y --default-toolchain stable-x86_64-pc-windows-msvc"
  rm -f "$init"
  ensure_cargo_path
}

has_msvc() {
  local vswhere candidates candidate
  candidates=(
    "/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
    "/c/Program Files/Microsoft Visual Studio/Installer/vswhere.exe"
  )
  for candidate in "${candidates[@]}"; do
    if [[ -f "$candidate" ]]; then
      vswhere="$candidate"
      break
    fi
  done
  [[ -n "${vswhere:-}" ]] || return 1
  "$vswhere" -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>/dev/null | grep -q .
}

install_build_tools() {
  if ! command -v winget >/dev/null 2>&1 && ! command -v winget.exe >/dev/null 2>&1; then
    echo "winget is not available. Install the C++ build tools from:"
    echo "  https://visualstudio.microsoft.com/visual-cpp-build-tools/"
    echo "Choose the Desktop development with C++ workload, then run this script again."
    exit 1
  fi
  local winget_cmd=winget
  if ! command -v winget >/dev/null 2>&1; then
    winget_cmd=winget.exe
  fi
  echo "Installing Visual Studio Build Tools. This is a large download."
  "$winget_cmd" install -e --id Microsoft.VisualStudio.2022.BuildTools --accept-package-agreements --accept-source-agreements --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
}

ensure_cargo_path

if ! command -v cargo >/dev/null 2>&1; then
  if ! ask "Rust is not installed. Install it now?" y; then
    echo "Install Rust from https://rustup.rs and run this script again."
    exit 1
  fi
  install_rust
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust was installed, but cargo is not on PATH. Open a new Git Bash window and run this script again."
  exit 1
fi

host="$(rustc -vV | sed -n 's/^host: //p')"
case "$host" in
  *windows-msvc)
    if ! has_msvc; then
      echo "The Microsoft C++ build tools are missing. Rust needs them to link the program."
      if ask "Install Visual Studio Build Tools now?" y; then
        install_build_tools
      else
        echo "Install the C++ build tools, then run this script again."
        echo "https://visualstudio.microsoft.com/visual-cpp-build-tools/"
        exit 1
      fi
    fi
    ;;
  *windows-gnu)
    if ! command -v gcc >/dev/null 2>&1; then
      echo "This Rust toolchain is the GNU one, and gcc was not found."
      echo "Install MinGW so gcc is on PATH, or switch with:"
      echo "  rustup default stable-x86_64-pc-windows-msvc"
      exit 1
    fi
    ;;
esac

echo
echo "Building RustyExile. The first build can take several minutes."
export CARGO_TARGET_DIR="$ROOT/target"
if ! cargo build --release -p RustyExile; then
  echo
  echo "The build failed. If the error mentions link.exe, install the C++ build tools and run this script again."
  exit 1
fi

bin="$ROOT/target/release/RustyExile.exe"
if [[ ! -f "$bin" ]]; then
  echo "The build finished, but $bin is missing."
  exit 1
fi

echo
if ask "Add RustyExile to the Start Menu?" y; then
  start_menu="${APPDATA:-$HOME/AppData/Roaming}/Microsoft/Windows/Start Menu/Programs"
  mkdir -p "$start_menu"
  lnk="$start_menu/RustyExile.lnk"
  ps1="${TMP:-${TEMP:-/tmp}}/rustyexile-shortcut-$$.ps1"
  cat > "$ps1" <<EOF
\$shell = New-Object -ComObject WScript.Shell
\$lnk = \$shell.CreateShortcut('$(win_path "$lnk")')
\$lnk.TargetPath = '$(win_path "$bin")'
\$lnk.WorkingDirectory = '$(win_path "$ROOT")'
\$lnk.Description = 'Path of Exile 1 campaign leveling overlay'
\$lnk.Save()
EOF
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(win_path "$ps1")"
  rm -f "$ps1"
  echo "Start Menu shortcut installed."
fi

echo
echo "Done. Start Path of Exile, then start RustyExile."
echo "The program is $(win_path "$bin")"

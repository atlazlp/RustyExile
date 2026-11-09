#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != Linux ]]; then
  echo "This installer is for Linux. On Windows Git Bash, run: bash installWindows.bash"
  exit 1
fi

if [[ ! -f "$ROOT/crates/poe-app/Cargo.toml" ]] || ! grep -q 'name = "RustyExile"' "$ROOT/crates/poe-app/Cargo.toml"; then
  echo "Run installLinux.sh from the RustyExile folder."
  exit 1
fi

echo "RustyExile setup"
echo "It builds the overlay, and it can add an app-menu shortcut and keyboard access for Ctrl+Alt+click."
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

as_root() {
  if command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1; then
    sudo "$@"
    return
  fi
  if command -v asroot >/dev/null 2>&1; then
    asroot "$@"
    return
  fi
  if command -v sudo >/dev/null 2>&1; then
    sudo "$@"
    return
  fi
  if command -v pkexec >/dev/null 2>&1; then
    pkexec "$@"
    return
  fi
  echo "Could not find sudo. Install the missing packages yourself, then run this script again."
  exit 1
}

if [[ -r /etc/os-release ]]; then
  # shellcheck disable=SC1091
  . /etc/os-release
fi

family=unknown
blob=" ${ID:-} ${ID_LIKE:-} "
case "$blob" in
  *" arch "*|*" cachyos "*|*" manjaro "*|*" endeavouros "*|*" artix "*) family=arch ;;
  *" debian "*|*" ubuntu "*|*" linuxmint "*|*" pop "*) family=debian ;;
  *" fedora "*|*" rhel "*|*" centos "*|*" nobara "*) family=fedora ;;
  *" suse "*|*" sles "*) family=suse ;;
  *)
    if command -v pacman >/dev/null 2>&1; then family=arch
    elif command -v apt-get >/dev/null 2>&1; then family=debian
    elif command -v dnf >/dev/null 2>&1; then family=fedora
    elif command -v zypper >/dev/null 2>&1; then family=suse
    fi
    ;;
esac

case "$family" in
  arch)
    packages=(gcc pkgconf wayland libxkbcommon mesa fontconfig ttf-dejavu)
    pkg_installed() { pacman -Q "$1" >/dev/null 2>&1; }
    install_packages() { as_root pacman -S --needed --noconfirm "$@"; }
    ;;
  debian)
    packages=(build-essential pkg-config libwayland-dev libxkbcommon-dev libegl1-mesa-dev libgl1-mesa-dev fonts-dejavu-core)
    pkg_installed() { dpkg -s "$1" >/dev/null 2>&1; }
    install_packages() {
      as_root apt-get update
      as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y "$@"
    }
    ;;
  fedora)
    packages=(gcc pkgconf-pkg-config wayland-devel libxkbcommon-devel mesa-libEGL-devel mesa-libGL-devel fontconfig dejavu-sans-fonts)
    pkg_installed() { rpm -q "$1" >/dev/null 2>&1; }
    install_packages() { as_root dnf install -y "$@"; }
    ;;
  suse)
    packages=(gcc pkg-config wayland-devel libxkbcommon-devel Mesa-libEGL-devel Mesa-libGL-devel fontconfig dejavu-fonts)
    pkg_installed() { rpm -q "$1" >/dev/null 2>&1; }
    install_packages() { as_root zypper --non-interactive install "$@"; }
    ;;
  *)
    packages=()
    pkg_installed() { return 1; }
    install_packages() { return 1; }
    ;;
esac

missing=()
if [[ ${#packages[@]} -gt 0 ]]; then
  for pkg in "${packages[@]}"; do
    if ! pkg_installed "$pkg"; then
      missing+=("$pkg")
    fi
  done
fi

if [[ "$family" == unknown ]]; then
  echo "This distro is not recognized. Install a C compiler, pkg-config, wayland, libxkbcommon, EGL/GL, fontconfig, and DejaVu fonts, then continue."
  if ! ask "Continue the build?" y; then
    exit 1
  fi
elif [[ ${#missing[@]} -gt 0 ]]; then
  echo "These packages are missing:"
  printf '  %s\n' "${missing[@]}"
  if ask "Install them now?" y; then
    install_packages "${missing[@]}"
  else
    echo "Install those packages, then run this script again."
    exit 1
  fi
else
  echo "System libraries are already installed."
fi

if ! command -v cargo >/dev/null 2>&1; then
  if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi
fi

if ! command -v cargo >/dev/null 2>&1; then
  if ! ask "Rust is not installed. Install it now?" y; then
    echo "Install Rust from https://rustup.rs and run this script again."
    exit 1
  fi
  if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required to download Rust."
    exit 1
  fi
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo
echo "Building RustyExile. The first build can take several minutes."
export CARGO_TARGET_DIR="$ROOT/target"
cargo build --release -p RustyExile
bin="$ROOT/target/release/RustyExile"
if [[ ! -x "$bin" ]]; then
  echo "The build finished, but $bin is missing."
  exit 1
fi

echo
if id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
  echo "Your user is already in the input group, so Ctrl+Alt+click can read the keyboard."
else
  echo "Ctrl+Alt+click reads the keyboard without taking it away from the game."
  echo "That needs your user in the input group. Log out and back in after adding it."
  if ask "Add $USER to the input group?" y; then
    as_root usermod -aG input "$USER"
    echo "Added. Log out and back in before Ctrl+Alt+click will work."
    echo "The other hotkeys still work through the desktop shortcut portal."
  else
    echo "Skipped. Ctrl+Shift hotkeys can still work. Ctrl+Alt+click will not."
  fi
fi

echo
if ask "Add RustyExile to the app launcher?" y; then
  apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
  mkdir -p "$apps"
  icon="$ROOT/assets/icons/ui/portal.png"
  cat > "$apps/RustyExile.desktop" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=RustyExile
GenericName=Path of Exile leveling overlay
Comment=Path of Exile 1 campaign leveling overlay
Exec="$bin"
Path=$ROOT
Icon=$icon
Terminal=false
Categories=Game;
StartupNotify=false
StartupWMClass=RustyExile
EOF
  chmod 755 "$apps/RustyExile.desktop"
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$apps" >/dev/null 2>&1 || true
  fi
  echo "Launcher installed. Open RustyExile from the app menu."
fi

echo
echo "Done. Start Path of Exile, then start RustyExile."
echo "The binary is $bin"

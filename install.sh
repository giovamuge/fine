#!/bin/sh
# Installs the latest fine release into ~/.local/bin (or $FINE_INSTALL_DIR).
#   curl -fsSL https://raw.githubusercontent.com/giovamuge/fine/main/install.sh | sh
set -eu
repo=giovamuge/fine
dir=${FINE_INSTALL_DIR:-$HOME/.local/bin}
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  *) echo "Sistema non supportato: $(uname -sm). Compila con: cargo install --git https://github.com/$repo" >&2; exit 1 ;;
esac
url=https://github.com/$repo/releases/latest/download/fine-$target.tar.gz
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Scarico $url"
curl -fsSL "$url" -o "$tmp/fine.tar.gz"
curl -fsSL "$url.sha256" -o "$tmp/fine.tar.gz.sha256"
expected=$(cut -d' ' -f1 "$tmp/fine.tar.gz.sha256")
actual=$(shasum -a 256 "$tmp/fine.tar.gz" 2>/dev/null || sha256sum "$tmp/fine.tar.gz")
[ "$expected" = "${actual%% *}" ] || { echo "Checksum non valido: download interrotto." >&2; exit 1; }
tar -xzf "$tmp/fine.tar.gz" -C "$tmp"
mkdir -p "$dir"
install -m 755 "$tmp/fine" "$dir/fine"
echo "Installato $dir/fine"
case ":$PATH:" in *":$dir:"*) ;; *) echo "Aggiungi $dir al PATH (es. nel ~/.zshrc: export PATH=\"$dir:\$PATH\")" ;; esac
echo "Ora esegui: fine init"

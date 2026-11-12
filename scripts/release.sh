#!/bin/sh
# Builds the release archives locally (no CI): one per macOS architecture, with checksums,
# into dist/release/. Publishing stays a manual step, printed at the end.
#   scripts/release.sh            # version from Cargo.toml
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
grep -q "\"version\": \"$version\"" .claude-plugin/plugin.json || { echo "Versione di .claude-plugin/plugin.json diversa da Cargo.toml ($version)" >&2; exit 1; }
out=dist/release
rm -rf "$out" && mkdir -p "$out"
targets="aarch64-apple-darwin x86_64-apple-darwin"
for t in $targets; do
  rustup target add "$t" >/dev/null
  cargo build --release --locked --target "$t"
  tar -czf "$out/fine-$t.tar.gz" -C "target/$t/release" fine
  (cd "$out" && shasum -a 256 "fine-$t.tar.gz" > "fine-$t.tar.gz.sha256")
done
echo
echo "Pronti in $out:"; ls -1 "$out"
echo
echo "Per pubblicare la v$version:"
echo "  git tag v$version && git push origin v$version"
echo "  gh release create v$version $out/* --generate-notes"

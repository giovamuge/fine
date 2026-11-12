#!/bin/sh
# Starts `fine mcp` for the Claude Code plugin. The plugin ships no binary: it looks for
# the one installed by install.sh or `cargo install`, also outside a minimal PATH.
for f in "$(command -v fine 2>/dev/null)" "$HOME/.local/bin/fine" "$HOME/.cargo/bin/fine" /opt/homebrew/bin/fine /usr/local/bin/fine; do
  [ -n "$f" ] && [ -x "$f" ] && exec "$f" mcp
done
echo "fine non è installato: curl -fsSL https://raw.githubusercontent.com/giovamuge/fine/main/install.sh | sh" >&2
exit 1

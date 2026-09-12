#!/usr/bin/env bash
# Installs the versioned git hooks into .git/hooks (git does not sync hooks
# on clone, so every fresh clone runs this once).
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cp "$ROOT/scripts/git-hooks/pre-commit" "$ROOT/.git/hooks/pre-commit"
chmod +x "$ROOT/.git/hooks/pre-commit"
echo "installed .git/hooks/pre-commit"

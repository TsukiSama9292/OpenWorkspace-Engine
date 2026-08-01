#!/usr/bin/env bash

# Continue execution even if a command fails, ensuring all checks run
set +e

echo "=================================================="
echo " [1/3] Running: Default cargo test --no-run check"
echo "=================================================="
cargo test --no-run 2>&1 | grep -iE "warning|error"
echo ""

echo "=================================================="
echo " [2/3] Running: cargo test with docker feature --no-run check"
echo "=================================================="
cargo test --no-run --features docker 2>&1 | grep -iE "warning|error"
echo ""

echo "=================================================="
echo " [3/3] Running: Library check (cargo check --lib)"
echo "=================================================="
cargo check --lib 2>&1 | grep -iE "warning|error"
echo ""

echo "=================================================="
echo " All checks completed!"
echo "=================================================="
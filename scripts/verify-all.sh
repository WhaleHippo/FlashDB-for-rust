#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

cd "$repo_root"
cargo fmt --check
if grep -R -n -E 'extern crate alloc|alloc::' src; then
  echo "unexpected alloc usage remains in src/"
  exit 1
fi
(
  cd "$repo_root/examples/stm32f401re"
  cargo fmt --check
)
(
  cd "$repo_root/examples/nrf5340"
  cargo fmt --check
)
(
  cd "$repo_root/examples/linux"
  cargo fmt --check
)
cargo test
cargo test --features std
cargo run --manifest-path examples/linux/Cargo.toml
cargo build --manifest-path examples/stm32f401re/Cargo.toml --target thumbv7em-none-eabihf
cargo build --manifest-path examples/nrf5340/Cargo.toml --target thumbv8m.main-none-eabihf

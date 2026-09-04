# Contributing

Open an issue or pull request with a focused description and reproduction steps. Keep dependencies and platform-specific behavior to the minimum needed.

Before submitting a code change, run:

```sh
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Add an integrity or regression test for behavior changes. Do not commit `target/`, `dist/`, generated archives, credentials, or machine-specific paths.

# Contributing

Keep changes small and explain the behavioral impact. Do not include generated
build output, local cache directories, machine-specific config, or credentials.

Run the same checks CI should run:

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

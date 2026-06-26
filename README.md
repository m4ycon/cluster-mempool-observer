### Basic commands

Run the API server:
```bash
cargo run --bin api
```

Run all tests:
```bash
cargo nextest run --all-features
```

Coverage:
```bash
cargo llvm-cov nextest --all-features --html --open
```

Format and lint:
```bash
cargo fmt --all && cargo clippy --all-targets --all-features
```

### Configuration

Copy `.env.example` to `.env` and fill in values, or inject the same variables as real environment variables (e.g. in a container). Process env takes precedence over `.env` (e.g. LOG_LEVEL=info cargo run -p api). `DATABASE_URL` is also read by the diesel CLI for migrations.

### Backend

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

### Frontend (`web/`)

```bash
pnpm dev            # vite dev server
pnpm build          # tsc -b && vite build
pnpm test           # vitest run
pnpm check          # biome check (lint + format + plugins)
pnpm check:write    # same, applying safe fixes
```

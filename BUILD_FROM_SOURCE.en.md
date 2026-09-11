## Build from Source (Alternative)

### Prerequisites
- Rust 1.98.1 and Cargo. The repository includes `rust-toolchain.toml`, so
  `rustup` selects the same toolchain automatically.

### Build and Run

Run directly:

```bash
cargo run
```

Or build release:

```bash
cargo build --release
```

Binary output:

```text
target/release/telegram-buktikanbot
```

Run the built binary:

```bash
./target/release/telegram-buktikanbot
```

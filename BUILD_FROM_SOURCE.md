## Build dari Source (Alternatif)

### Pra Syarat
- Rust 1.98.1 dan Cargo. Repository menyediakan `rust-toolchain.toml`, sehingga
  `rustup` akan memilih toolchain yang sama secara otomatis.

### Build dan Run

Jalankan langsung:

```bash
cargo run
```

Atau build release:

```bash
cargo build --release
```

Hasil binary ada di:

```text
target/release/telegram-buktikanbot
```

Jalankan binary hasil build:

```bash
./target/release/telegram-buktikanbot
```

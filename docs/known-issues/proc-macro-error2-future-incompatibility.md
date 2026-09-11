# Future-incompatibility `proc-macro-error2`

Status: dipantau, belum perlu tindakan pada source code aplikasi.

Tanggal pemeriksaan terakhir: 2026-09-11.

## Warning

Build dengan Cargo menampilkan warning berikut:

```text
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
note: run `cargo report future-incompatibilities --id 1` for more information
```

Detail lint yang dilaporkan Cargo adalah `pub_use_of_private_extern_crate` pada
kode internal `proc-macro-error2`. Masalahnya berada di dependency procedural
macro, bukan di source code bot.

## Rantai dependency

```text
telegram-buktikanbot
└── teloxide 0.17.0
    └── aquamarine 0.6.0 (proc-macro)
        └── proc-macro-error2 2.0.1
```

Verifikasi rantai dependency:

```bash
cargo tree --locked -i proc-macro-error2
```

## Dampak saat ini

Tidak ada dampak operasional yang terdeteksi saat ini:

- `cargo check --locked` berhasil;
- `cargo test --locked` berhasil dengan 20 test lulus;
- `cargo clippy --locked --all-targets -- -D warnings` berhasil;
- `cargo build --release --locked` berhasil.

Warning ini tidak berasal dari kode aplikasi dan tidak menyebabkan build atau
CI gagal pada toolchain Rust yang sedang dikunci oleh repository.

## Keputusan maintenance

Jangan menambahkan `[patch]` manual atau menonaktifkan warning hanya untuk
menghilangkan pesan ini. Workaround semacam itu dapat membuat dependency Teloxide
tidak konsisten atau menyembunyikan masalah yang perlu diketahui saat upgrade
Rust berikutnya.

Tindakan yang dipilih:

1. Pertahankan dependency Teloxide `0.17.0` dan lockfile yang sudah tervalidasi.
2. Pantau rilis Teloxide dan dependency `aquamarine`/`proc-macro-error2`.
3. Periksa ulang setelah upgrade Rust atau dependency besar.
4. Ambil tindakan hanya jika tersedia perbaikan upstream resmi atau warning berubah
   menjadi error.

## Cara memeriksa ulang

```bash
cargo check --locked
cargo report future-incompatibilities --id 1
cargo tree --locked -i proc-macro-error2
```

Referensi:

- [Rust issue #127909: `pub_use_of_private_extern_crate`](https://github.com/rust-lang/rust/issues/127909)
- [Repository `proc-macro-error-2`](https://github.com/GnomedDev/proc-macro-error-2)

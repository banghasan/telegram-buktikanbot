# Dependency Management

## Dependabot

Repository ini menggunakan Dependabot untuk memantau tiga jenis dependency:

- Cargo crates dari `Cargo.toml` dan `Cargo.lock`;
- GitHub Actions pada `.github/workflows/`;
- base image dan dependency Docker yang ditemukan pada file Docker.

Konfigurasinya berada di `.github/dependabot.yml`.

Dependabot dijadwalkan setiap Senin dini hari berdasarkan zona waktu
`Asia/Jakarta`. Update minor dan patch Cargo dikelompokkan agar jumlah Pull
Request tetap terkendali. Update major tetap dibuat terpisah karena berpotensi
mengandung perubahan API atau perilaku.

## Alur review

Setiap Pull Request dari Dependabot harus:

1. melewati workflow CI;
2. diperiksa perubahan `Cargo.toml` dan `Cargo.lock` jika berkaitan dengan Cargo;
3. diperiksa perubahan workflow jika berkaitan dengan GitHub Actions;
4. diperiksa perubahan base image dan hasil build jika berkaitan dengan Docker;
5. di-merge secara manual setelah hasilnya sesuai.

Auto-merge tidak digunakan. Update Teloxide, Rust edition/toolchain, GitHub
Actions, dan base image dapat memerlukan pemeriksaan manual.

## Rust toolchain

`rust-toolchain.toml` tetap dikunci pada versi yang digunakan repository. Dependabot
memperbarui Cargo crates, tetapi tidak menggantikan keputusan repository untuk
mengubah versi compiler Rust. Upgrade toolchain dilakukan sebagai perubahan
terpisah agar efeknya mudah ditelusuri.

## Security update

Dependabot Security Updates tetap perlu diaktifkan pada pengaturan repository
GitHub. Pull Request security update diperlakukan seperti Pull Request biasa
dan tetap harus melewati CI.

Dependabot bukan pengganti pemeriksaan vulnerability lokal. Jika kebutuhan
security scanning meningkat, workflow terpisah dapat menambahkan `cargo audit`
tanpa mengubah alur update dependency.

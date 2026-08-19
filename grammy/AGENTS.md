# Aturan Pengembangan grammY

Dokumen ini berlaku untuk seluruh isi direktori grammy/.

## Aturan wajib: Bun saja

Implementasi grammY wajib menggunakan Bun:

- runtime: Bun;
- install dependency: bun install;
- menjalankan script: bun run;
- test runner: Bun test, kecuali ada keputusan tertulis yang mengubahnya;
- lockfile: bun.lock;
- menjalankan aplikasi: bun run atau entrypoint Bun yang disepakati.

Jangan gunakan atau tambahkan ketergantungan pada Node.js sebagai runtime, npm, npx, yarn, pnpm, package-lock.json, yarn.lock, atau pnpm-lock.yaml.

Dokumentasi, Dockerfile, CI, contoh perintah, dan issue implementasi harus menyebut Bun dan tidak boleh menginstruksikan workflow Node.js.

## Aturan migrasi

- Jangan menulis source code grammY sebelum fase dokumentasi disetujui.
- Jangan menghapus atau menimpa aplikasi Rust lama.
- Jangan memakai token produksi untuk eksperimen.
- Jangan menjalankan Rust dan grammY bersamaan memakai token produksi yang sama.
- Pertahankan nama environment variable selama tidak ada alasan teknis yang terdokumentasi.
- Perubahan schema SQLite harus backward-compatible atau memiliki rencana migrasi dan rollback.
- Perubahan permission, ban, timeout, atau aturan keamanan harus memiliki test dan catatan keputusan.

## Aturan kualitas setelah coding dimulai

- TypeScript harus strict.
- Handler Telegram harus dipisahkan dari layanan CAPTCHA, persistence, logging, dan konfigurasi.
- API Telegram baru harus dibungkus pada module yang mudah diuji.
- Error Telegram dan error internal harus dicatat dengan konteks chat/user tanpa membocorkan token.
- Test harus bisa berjalan tanpa token produksi.
- CI harus menggunakan Bun, bukan Node.js package tooling.

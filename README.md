# Telegram Buktikan Bot

Repository ini berisi bot verifikasi anggota baru Telegram dengan dua area aplikasi:

- [rust/](./rust/) — aplikasi Rust/Teloxide yang saat ini digunakan.
- [grammy/](./grammy/) — area rencana migrasi ke grammY dan TypeScript menggunakan Bun.

Migrasi grammY masih berada pada tahap dokumentasi dan diskusi. Belum ada source code grammY dan belum ada perubahan perilaku produksi.

## Dokumentasi

- [Rencana migrasi grammY](./grammy/README.md)
- [Aturan pengembangan grammY](./grammy/AGENTS.md)
- [Daftar issue migrasi](./grammy/issues/README.md)
- [Panduan Rust lama](./rust/README.md)

## Status

Branch migrasi ini hanya mengerjakan reorganisasi repository, dokumentasi keputusan dan rencana, pemetaan fitur Rust ke target grammY, serta persiapan issue implementasi.

Implementasi kode baru dilakukan setelah rencana disepakati.

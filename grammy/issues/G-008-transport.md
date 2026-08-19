# G-008 — Polling, Webhook, dan Shutdown

## Tujuan

Menyediakan transport update dan shutdown behavior yang setara dengan deployment Rust.

## Acceptance criteria

- polling dapat dijalankan dengan Bun;
- webhook menerima secret token jika digunakan;
- hanya satu transport aktif per token;
- shutdown tidak meninggalkan pending work tanpa catatan;
- perpindahan polling/webhook terdokumentasi.

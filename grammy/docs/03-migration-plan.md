# Rencana Fase Migrasi

## Fase 0 — Dokumentasi dan keputusan

Status: berjalan pada branch ini.

- inventaris fitur Rust;
- definisi aturan Bun;
- desain arsitektur target;
- daftar issue dan acceptance criteria;
- daftar keputusan yang masih terbuka.

## Fase 1 — Fondasi proyek

- buat package grammY berbasis TypeScript;
- gunakan Bun dan bun.lock;
- siapkan strict TypeScript;
- siapkan konfigurasi test tanpa token;
- siapkan entrypoint polling minimal hanya setelah disetujui.

## Fase 2 — Konfigurasi, logging, dan persistence

- port environment variable;
- implementasi logging setara;
- baca/tulis schema SQLite;
- implementasi ban-release;
- uji restart dan idempotency.

## Fase 3 — Verifikasi dasar

- deteksi anggota baru;
- restrict permission;
- generate CAPTCHA;
- inline keyboard;
- validasi user/chat ownership;
- restore permission;
- timeout dan ban.

## Fase 4 — Ephemeral Messages

- kirim CAPTCHA sebagai ephemeral photo;
- simpan ephemeral_message_id;
- tangani callback dari ephemeral message;
- edit caption/media/markup dengan method ephemeral;
- hapus pesan ephemeral saat terminal state;
- siapkan fallback bila client atau delivery tidak mendukung.

## Fase 5 — Transport dan deployment

- polling;
- webhook;
- Docker dengan Bun;
- health, shutdown, dan observability;
- CI yang hanya memakai Bun untuk proyek grammY.

## Fase 6 — Validasi dan cutover

- staging dengan token berbeda;
- test manual pada client Telegram yang ditargetkan;
- parallel comparison tanpa menjalankan dua bot pada token produksi;
- cutover terjadwal;
- rollback ke image Rust jika diperlukan.

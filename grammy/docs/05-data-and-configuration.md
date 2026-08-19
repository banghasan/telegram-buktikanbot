# Kompatibilitas Data dan Konfigurasi

## Environment variable

Nama konfigurasi Rust dipertahankan sebagai baseline:

- BOT_TOKEN
- CAPTCHA_LEN
- CAPTCHA_TIMEOUT_SECONDS
- CAPTCHA_CAPTION_UPDATE_SECONDS
- CAPTCHA_WIDTH
- CAPTCHA_HEIGHT
- CAPTCHA_OPTION_COUNT
- CAPTCHA_ATTEMPTS
- CAPTCHA_OPTION_DIGITS_TO_EMOJI
- DELETE_JOIN_MESSAGE
- DELETE_LEFT_MESSAGE
- BAN_RELEASE_ENABLED
- BAN_RELEASE_AFTER_SECONDS
- BAN_RELEASE_DB_PATH
- LOG_ENABLED
- LOG_JSON
- LOG_LEVEL
- CAPTCHA_LOG_ENABLED
- CAPTCHA_LOG_CHAT_ID
- TIMEZONE
- RUN_MODE
- webhook variables yang sudah ada

Jika grammY membutuhkan nama baru, mapping lama ke baru harus ditulis di dokumentasi dan env.example.

Keputusan sementara:

- development dan staging memakai polling;
- production memakai webhook;
- durasi ban sementara dikonfigurasi melalui BAN_RELEASE_AFTER_SECONDS;
- default durasi ban adalah 14400 detik atau empat jam.

## SQLite

Strategi awal: kompatibilitas baca/tulis dengan data Rust dipertahankan, dan pending CAPTCHA juga dirancang agar survive restart.

Sebelum implementasi:

- dokumentasikan tabel dan index yang dipakai Rust;
- identifikasi apakah proses lama dan baru memakai database yang sama;
- tambahkan migration version jika schema berubah;
- siapkan backup dan rollback;
- jangan mengetes dua process yang menulis database produksi secara bersamaan.

## Docker

Image grammY harus menjalankan Bun, menggunakan user non-root, mempertahankan volume /data, memiliki health dan shutdown behavior yang terdokumentasi, serta tidak menyalin token ke image.

## Logging

Format log sebaiknya dipertahankan agar integrasi operator tidak rusak. Token, isi secret, dan data sensitif tidak boleh dicatat.

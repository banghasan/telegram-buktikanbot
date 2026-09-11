# Review Pasca-Perbaikan

[WARNING] Deployment VPS pada bind mount `/data` — aplikasi berjalan sebagai UID 10001 dan tetap membutuhkan direktori database yang writable; kode tidak dapat memperbaiki ownership host secara otomatis — gunakan named volume atau jalankan `chown 10001:10001` pada direktori host lalu recreate container.

[SUGGESTION] `src/main.rs:270-308` — worker tetap membutuhkan akses Telegram dan izin admin untuk menghasilkan log pelepasan ban; kegagalan request dipertahankan untuk dicoba pada interval berikutnya — pantau log `ban release worker error` dan `failed to unban`.

[SUGGESTION] Pengujian Telegram live — unit test sudah mencakup schema, migrasi, round-trip state CAPTCHA, job release, validasi expiry, dan message ID, tetapi belum menguji request Telegram sungguhan — lakukan smoke test pada grup staging setelah deploy.

[SUGGESTION] `docker-compose.yml` dan `docker-compose.webhook.yml` — tag image kini dipin ke versi `1.9.0` dengan override `BOT_IMAGE`, sehingga upgrade tetap memerlukan perubahan tag secara eksplisit — ubah `BOT_IMAGE` saat merilis versi baru.

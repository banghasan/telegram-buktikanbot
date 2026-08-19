# Strategi Testing dan Rollout

## Testing tanpa Telegram production

Test harus mencakup parsing konfigurasi, generate CAPTCHA, validasi jawaban, batas percobaan, expiry, ownership callback, idempotency, restore permission, ban-release scheduling, sanitasi log, dan SQLite migration.

## Testing dengan Telegram staging

Gunakan bot token staging, grup staging, database staging, endpoint staging, dan client Telegram yang akan dipakai operator dan user.

Skenario manual minimum:

- user baru berhasil pada percobaan pertama;
- jawaban salah lalu benar;
- semua percobaan habis;
- timeout;
- callback dari user lain;
- bot restart saat CAPTCHA pending;
- user offline saat ephemeral dikirim;
- ephemeral gagal dikirim pada percobaan pertama dan kedua lalu berpindah ke fallback grup;
- fallback grup menolak jawaban teks dan hanya menerima callback button;
- join dan left message;
- polling dan webhook.

## Cutover

1. Bekukan perubahan fitur.
2. Backup database produksi.
3. Pastikan image grammY sudah diuji.
4. Hentikan Rust dan pastikan webhook/polling tidak lagi aktif.
5. Jalankan grammY dengan token produksi.
6. Verifikasi getMe, update delivery, dan satu alur CAPTCHA.
7. Pantau error dan permission.

## Rollback

Rollback berarti menghentikan grammY, mengembalikan deployment Rust, dan memastikan hanya satu process memakai token produksi. Dampak terhadap pending CAPTCHA harus didokumentasikan sebelum cutover.

# Keputusan yang Sudah Disepakati

Dokumen ini mencatat hasil diskusi sebelum implementasi dimulai. Keputusan di sini menjadi baseline desain grammY sampai ada perubahan tertulis.

## 1. Alur admission

Verifikasi tetap dilakukan setelah user masuk grup:

1. user masuk;
2. bot membatasi permission;
3. bot mengirim CAPTCHA;
4. user memilih tombol;
5. success memulihkan permission;
6. timeout atau kegagalan mengarah ke ban sementara.

Belum ada perubahan ke alur chat join request.

## 2. Ephemeral dan fallback

Bot mencoba mengirim CAPTCHA sebagai Ephemeral Message. Request pertama dikirim langsung; retry kedua dilakukan setelah jeda tiga detik. Jika dua request gagal di level API, bot menggunakan CAPTCHA pesan biasa di grup seperti perilaku Rust saat ini.

Telegram tidak memberikan sinyal yang dapat memastikan pesan sudah tampil pada client user. Karena itu, user offline diperlakukan berbeda dari request API yang gagal dan tidak otomatis memicu fallback publik.

Fallback publik harus tetap menggunakan inline button. User tidak boleh menjawab CAPTCHA dengan mengirim pesan teks.

## 3. Input user

Selama verifikasi, user tidak boleh mengirim pesan apa pun untuk menjawab CAPTCHA. Satu-satunya input yang valid adalah inline button callback.

Pesan teks atau pesan lain dari user tidak dianggap sebagai jawaban CAPTCHA dan harus mengikuti kebijakan moderasi yang disepakati saat implementasi handler.

## 4. Restart dan pending state

Pending CAPTCHA disimpan secara persistent sesuai rekomendasi sebelumnya. State minimal mencakup user, chat, expiry, attempts, pilihan, dan identifier pesan ephemeral atau pesan fallback.

Setelah restart, bot harus merekonsiliasi state yang belum selesai. State yang tidak dapat dilanjutkan harus menjalani retry/fallback, bukan diam-diam memberikan permission.

## 5. Ban sementara

User yang timeout atau menghabiskan percobaan akan dikenai ban sementara. Durasi ban dikonfigurasi melalui environment variable, dengan contoh empat jam:

~~~env
BAN_RELEASE_AFTER_SECONDS=14400
~~~

Nilai default produksi ditetapkan empat jam. Setelah durasi selesai, user dapat mencoba masuk kembali sesuai aturan Telegram dan konfigurasi bot.

## 6. Callback dan client lama

Callback query tetap digunakan sebagai mekanisme jawaban, baik untuk pesan ephemeral maupun fallback pesan biasa. Perilaku fallback harus menyerupai implementasi Rust, dengan perbedaan bahwa jawaban teks tidak diterima.

Client yang tidak mendukung Ephemeral Messages boleh menggunakan fallback pesan grup biasa setelah mekanisme retry gagal.

## 7. Edge case dan fail-safe behavior

- Jika ephemeral dan fallback pesan grup sama-sama gagal, user tetap restricted, timeout tetap berjalan, dan error dicatat sebagai critical.
- Jika user keluar sebelum verifikasi selesai, pending state dibersihkan. Jika user masuk lagi, bot membuat CAPTCHA baru.
- Join event, callback ganda, timeout bersamaan dengan jawaban benar, dan update terlambat harus idempotent.
- Jika bot kehilangan hak admin, bot tidak boleh menganggap verifikasi berhasil. User tetap pada kondisi aman dan kegagalan dicatat.
- Setelah verifikasi, permission dipulihkan berdasarkan permission grup terbaru.
- Jika Telegram memberikan retry_after, bot mengikuti nilainya. Retry tidak boleh melewati batas timeout tanpa masuk ke alur timeout.

## 8. Data

SQLite dipertahankan. Pending verification dan ban-release menggunakan storage yang dapat survive restart. Database staging dan production dipisahkan.

Rust dan grammY tidak dijalankan bersamaan dengan token atau database production yang sama.

## 9. Transport

Polling dipakai untuk development dan staging. Webhook menjadi mode utama production, dengan secret token dan graceful shutdown yang terdokumentasi.

Kedua mode tetap didukung selama tidak menambah kompleksitas yang mengubah keamanan verifikasi.

## 10. Standar kualitas

- State machine verification harus eksplisit dan memiliki terminal state.
- Telegram API dipanggil melalui adapter yang terpisah dari business logic.
- Production memakai structured JSON logging dan correlation ID berbasis chat/user.
- Environment variable divalidasi dengan batas nilai yang jelas.
- Staging, backup SQLite, checklist cutover, dan rollback wajib tersedia.
- Unit test, integration test tanpa token production, lint, type-check, dan format check menjadi quality gate.

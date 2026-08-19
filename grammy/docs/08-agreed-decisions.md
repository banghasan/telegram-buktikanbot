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

## 7. Data

SQLite dipertahankan. Pending verification dan ban-release menggunakan storage yang dapat survive restart. Database staging dan production dipisahkan.

Rust dan grammY tidak dijalankan bersamaan dengan token atau database production yang sama.

## 8. Transport

Polling dipakai untuk development dan staging. Webhook menjadi mode utama production, dengan secret token dan graceful shutdown yang terdokumentasi.

Kedua mode tetap didukung selama tidak menambah kompleksitas yang mengubah keamanan verifikasi.

## Belum final

- nilai default BAN_RELEASE_AFTER_SECONDS;
- jumlah dan interval retry pengiriman ephemeral secara teknis;
- kebijakan pasti untuk pesan non-button selama user restricted;
- daftar client Telegram minimum;
- detail schema persistence pending CAPTCHA.

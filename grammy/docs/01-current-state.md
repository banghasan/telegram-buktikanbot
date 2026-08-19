# Kondisi Aplikasi Rust Saat Ini

## Struktur dan teknologi

- Bahasa: Rust edition 2024.
- Framework Telegram: Teloxide 0.12.2.
- Dispatcher: dptree.
- HTTP webhook: listener Axum milik Teloxide.
- Runtime: Tokio.
- Database: SQLite melalui rusqlite.
- CAPTCHA: crate captcha.
- Konfigurasi: environment variable dan dotenvy.
- Deployment: binary release dan Docker.

## Alur verifikasi

1. Bot menerima anggota baru melalui update message atau chat member.
2. Bot menghapus pesan join jika konfigurasi mengizinkan.
3. Bot mengosongkan permission user.
4. Bot mengirim gambar CAPTCHA dan inline keyboard ke grup.
5. State pending disimpan berdasarkan pasangan chat ID dan user ID.
6. Caption CAPTCHA diperbarui berkala dengan sisa waktu dan percobaan.
7. Callback button divalidasi terhadap state pending.
8. Jawaban benar menghapus CAPTCHA dan mengembalikan permission.
9. Jawaban salah mengurangi percobaan dan dapat mengganti CAPTCHA.
10. Timeout atau percobaan habis menghapus CAPTCHA dan mengeluarkan user.

## Fitur pendukung

- Ban-release terjadwal dengan SQLite.
- Logging user event, system event, dan error Telegram.
- Mode polling dan webhook.
- Penghapusan pesan join/left.
- Konfigurasi ukuran CAPTCHA, panjang kode, jumlah pilihan, timeout, timezone, dan logging.

## Risiko yang harus dipertahankan

- User tidak boleh mendapatkan permission penuh sebelum verifikasi berhasil.
- Callback dari user lain tidak boleh menyelesaikan CAPTCHA milik user berbeda.
- Pending state harus dibersihkan pada success, timeout, dan failure.
- Restart process tidak boleh menyebabkan data terjadwal hilang tanpa keputusan.
- Token dan data sensitif tidak boleh masuk log.

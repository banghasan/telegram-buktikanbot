# Review Ban-Release Timer

[CRITICAL] `src/main.rs:86-107` dan `src/config.rs:82-96` — worker ban-release hanya dibuat jika `BAN_RELEASE_ENABLED=true`; default source dan Docker Compose adalah `false`, sehingga timer tidak berjalan bila environment server belum diaktifkan atau container belum dibuat ulang setelah `.env` berubah — periksa log startup untuk `ban_release_enabled=true` dan `ban release worker started`, lalu recreate container dengan environment terbaru.

[CRITICAL] `src/handlers.rs:707-712` — setelah user berhasil diban, job tidak disimpan jika `ban_release_store` bernilai `None`, tetapi fungsi langsung return tanpa log khusus; ini terjadi bila worker dinonaktifkan atau inisialisasi SQLite gagal — log status store secara eksplisit dan anggap kegagalan penyimpanan sebagai error operasional.

[CRITICAL] deployment `/data` — container menjalankan aplikasi sebagai `appuser` (UID 10001), sedangkan `/data` dan `buktikan.sqlite` terlihat dimiliki `root:root` dengan mode `755/644`; SQLite kemungkinan tidak dapat menulis database atau membuat file `-wal/-shm`, sehingga inisialisasi store gagal — ubah ownership ke `10001:10001`, verifikasi write access sebagai UID tersebut, lalu restart/recreate container.

[WARNING] `docker-compose.yml:19-21` dan `docker-compose.webhook.yml:23-25` — default `BAN_RELEASE_AFTER_SECONDS` masih 21600 detik atau 6 jam, bukan 4 jam — set `BAN_RELEASE_AFTER_SECONDS=14400` pada environment server dan recreate container.

[WARNING] `src/main.rs:253-300` — worker hanya mengulang setiap 60 detik dan mempertahankan job jika `unbanChatMember` gagal; timer dapat terlihat tidak berjalan padahal request Telegram gagal berulang — periksa log `failed to unban user ...`, terutama hak admin `can_restrict_members`/ban users.

[WARNING] `src/ban_release.rs:6-16` dan `docker-compose.yml:28-32` — jadwal bergantung pada SQLite di volume `/data`; jika path salah, volume berbeda, atau deployment memakai `docker compose down -v`, job bisa tidak pernah tersimpan atau hilang — verifikasi path aktual, mount volume, tabel `ban_release_jobs`, dan jangan menghapus named volume.

[WARNING] `src/main.rs:287-298` — keberhasilan `unbanChatMember` tidak membuat user otomatis masuk lagi; Telegram hanya menghapus ban dan mengizinkan user bergabung kembali — jangan memakai user tetap di luar grup sebagai indikator bahwa unban gagal.

[SUGGESTION] `src/handlers.rs:713` — kode memakai worker lokal, bukan parameter `until_date` pada `banChatMember`; Telegram Bot API menyediakan `until_date` untuk ban sementara dan akan mengakhiri ban otomatis — pertimbangkan memakai native timed ban sebagai mekanisme utama, dengan worker hanya untuk audit/log jika tetap diperlukan.

[SUGGESTION] `src/main.rs:41-64` — konfigurasi startup sudah dilog, tetapi belum ada indikator eksplisit bahwa database ban-release berhasil dibuka dan berapa jumlah job pending — tambahkan health/diagnostic log agar masalah environment, database, dan worker cepat terlihat.

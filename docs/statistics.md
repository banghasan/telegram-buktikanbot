# Statistik Admin

## Tujuan

Fitur statistik membantu admin memantau kinerja verifikasi CAPTCHA dan proses
ban-release dari private chat bot. Data statistik disimpan di SQLite yang sama
dengan state CAPTCHA dan jadwal ban.

## Perintah

Perintah berikut hanya dapat digunakan oleh user yang tercantum pada
`ADMIN_USER_IDS`:

- `/stats`
- `/statistic`
- `/statistik`

Perintah hanya diproses di private chat. User yang tidak terdaftar sebagai admin
mendapat penolakan dan tidak menerima laporan.

## Bentuk laporan

Saat command dijalankan, bot mengirim:

1. Ringkasan langsung di chat dengan tombol inline.
2. Dokumen Markdown lengkap dengan nama file seperti
   `buktikanbot-statistik-2026-09-11-103015.md`.

Isi dokumen mencakup:

- waktu cetak laporan;
- timezone yang digunakan;
- periode dan batas akhir data;
- jumlah user unik;
- jumlah sesi verifikasi;
- verifikasi berhasil dan gagal;
- rincian timeout, percobaan habis, dan kegagalan setup;
- jumlah ban dibuat;
- jumlah auto-release dan manual release;
- jumlah ban pending dan CAPTCHA aktif saat laporan dibuat;
- statistik per grup, termasuk username grup jika tersedia.

Telegram mengirim file `.md` sebagai dokumen biasa. Markdown akan terlihat
sebagai tabel dan heading ketika file dibuka pada aplikasi/editor Markdown yang
mendukungnya.

## Periode

Tombol inline pada ringkasan menyediakan:

- **Hari ini**: sejak tengah malam berdasarkan `TIMEZONE`;
- **7 hari**: 7 x 24 jam terakhir;
- **30 hari**: 30 x 24 jam terakhir;
- **Semua**: seluruh event historis yang tersimpan.

`Pending saat ini` dan `CAPTCHA aktif saat ini` selalu menunjukkan kondisi
terbaru ketika laporan dibuat, bukan jumlah berdasarkan periode yang dipilih.

## Event yang dicatat

Histori menggunakan tabel `stats_events` yang bersifat append-only. Event yang
dicatat adalah:

- `captcha_started`: CAPTCHA berhasil dikirim dan sesi berhasil disimpan;
- `captcha_success`: user menyelesaikan verifikasi;
- `captcha_failed`: verifikasi gagal karena timeout (`timeout`),
  percobaan habis (`attempts_exceeded`), atau CAPTCHA tidak dapat disiapkan
  (`setup_failed`);
- `ban_created`: ban berhasil diterapkan;
- `ban_auto_released`: ban dilepas oleh worker otomatis;
- `ban_manual_released`: ban dilepas melalui panel admin.

Kegagalan pencatatan event tidak menghentikan proses CAPTCHA, ban, atau release.
Bot mencatat warning agar masalah penyimpanan dapat diperiksa tanpa mengganggu
operasi utama.

## Awal histori

Histori mulai tersedia sejak versi yang memiliki fitur statistik dijalankan.
Sesi CAPTCHA dan ban yang sudah selesai sebelum itu tidak dapat direkonstruksi
secara sempurna karena tabel lama hanya menyimpan state aktif.

Migrasi database dilakukan otomatis saat bot mulai. Tidak diperlukan pembuatan
tabel manual dan tidak diperlukan perubahan pada volume Docker selama database
yang sama tetap digunakan.

## Privasi dan keamanan

Laporan hanya berisi agregat dan label grup. Token bot, path database, URL
webhook, chat ID log, dan data operasional sensitif tidak dimasukkan ke laporan.
Akses laporan dibatasi melalui allowlist `ADMIN_USER_IDS`.

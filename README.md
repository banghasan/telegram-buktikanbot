# Bot Telegram - Buktikan Dirimu Manusia

![](https://img.shields.io/github/v/release/banghasan/telegram-buktikanbot
) ![](https://img.shields.io/github/actions/workflow/status/banghasan/telegram-buktikanbot/release.yml?style=plastic
) ![](https://img.shields.io/github/downloads/banghasan/telegram-buktikanbot/total
) ![](https://img.shields.io/github/languages/code-size/banghasan/telegram-buktikanbot
) 
![](https://img.shields.io/github/forks/banghasan/telegram-buktikanbot
) ![](https://img.shields.io/github/stars/banghasan/telegram-buktikanbot
)

Read this README in English: [README.en.md](./README.en.md)

Bot Telegram untuk memverifikasi user baru yang masuk grup menggunakan CAPTCHA bergambar. Ketika user masuk grup, semua hak akses dicabut -- user hanya dapat berinteraksi melalui tombol CAPTCHA.
User wajib menebak teks pada gambar dalam waktu tertentu, jika tidak menjawab dengan benar dalam waktu tertentu,
maka bot akan mengeluarkan user dari grup. Jika benar, bot menghapus gambar CAPTCHA
dan hak akses user dipulihkan.

![](./screenshot/buktikan.jpg)

## Fitur
- Kirim CAPTCHA gambar ke user baru.
- Panjang teks CAPTCHA dapat diatur lewat `.env`.
- User baru bergabung tidak dapat mengirim pesan dan hanya dapat menekan tombol CAPTCHA.
- Timeout verifikasi (default 120 detik), bisa disesuaikan sendiri.
- Interval update caption (default 10 detik), bisa disesuaikan sendiri.
- Jawaban benar: hapus pesan CAPTCHA dan pulihkan hak akses user.
- Jawaban salah terhapus, jika timeout: kick user dari grup.
- User terverifikasi, hak akses grup dipulihkan.
- Panel admin private untuk melihat ban yang menunggu release dan melepasnya dengan tombol inline.

## Persyaratan
- Bot Telegram yang sudah dibuat lewat BotFather.
- Bot jadi admin grup dengan izin:
  - Delete messages (hapus pesan CAPTCHA, join/left, dan pesan terblokir)
  - Ban users / Restrict members (cabut hak akses, ban saat gagal, dan unban sementara)
  - (Opsional) Manage messages jika ingin bot bisa menghapus pesan di semua tipe grup

## Menjalankan dari Release

Info perubahan versi ada di halaman [Release][releases].

1) Unduh file release sesuai OS/arsitektur (pilih yang cocok dengan sistem kamu):
   - Linux 64-bit Intel/AMD: `x86_64-unknown-linux-gnu`
   - Linux 64-bit ARM (mis. server ARM/Raspberry Pi 64-bit): `aarch64-unknown-linux-gnu`
   - macOS Intel: `x86_64-apple-darwin`
   - macOS Apple Silicon (M1/M2/M3): `aarch64-apple-darwin`
   - Windows 64-bit: `x86_64-pc-windows-msvc`
   
   Cara cek arsitektur:
   - Windows (PowerShell): `wmic os get osarchitecture`
   - macOS (Terminal): `uname -m`
   - Linux (Terminal): `uname -m`

2) Ekstrak dan jalankan:
   - Linux/macOS:
     ```bash
     tar -xzf buktikanbot-<versi>-<target>.tar.gz
     ./buktikanbot
     ```
   - Windows (PowerShell):
     ```powershell
     Expand-Archive -Path buktikanbot-<versi>-x86_64-pc-windows-msvc.zip -DestinationPath .
     .\buktikanbot.exe
     ```

## Konfigurasi
Salin dulu `.env.example` menjadi `.env`, lalu isi nilainya:
- Linux/macOS (Terminal): `cp .env.example .env`
- Windows (PowerShell): `Copy-Item .env.example .env`
File `.env.example` ada di root repo.

```env
BOT_TOKEN=your-telegram-bot-token
ADMIN_USER_IDS=123456789,987654321
CAPTCHA_LEN=6
CAPTCHA_TIMEOUT_SECONDS=120
CAPTCHA_CAPTION_UPDATE_SECONDS=10
CAPTCHA_WIDTH=320
CAPTCHA_HEIGHT=100
CAPTCHA_OPTION_COUNT=6
CAPTCHA_ATTEMPTS=3
CAPTCHA_OPTION_DIGITS_TO_EMOJI=true
DELETE_JOIN_MESSAGE=true
DELETE_LEFT_MESSAGE=true
BAN_RELEASE_ENABLED=false
BAN_RELEASE_AFTER_SECONDS=14400
BAN_RELEASE_DB_PATH=buktikan.sqlite
LOG_ENABLED=true
LOG_JSON=false
LOG_LEVEL=info
CAPTCHA_LOG_ENABLED=false
CAPTCHA_LOG_CHAT_ID=
CAPTCHA_LOG_MESSAGE_THREAD_ID=
TIMEZONE=Asia/Jakarta
```

Keterangan variabel:
- `BOT_TOKEN`: token bot Telegram.
- `ADMIN_USER_IDS`: daftar ID user Telegram yang boleh memakai `/pending` dan `/bans` di private chat, dipisahkan koma. Gunakan ID numerik dari `from.id`, bukan username. Kosongkan untuk menonaktifkan panel admin.
- `CAPTCHA_LEN`: panjang karakter CAPTCHA.
- `CAPTCHA_TIMEOUT_SECONDS`: waktu maksimum menebak.
- `CAPTCHA_CAPTION_UPDATE_SECONDS`: interval update caption countdown (default 10 detik).
- `CAPTCHA_WIDTH` / `CAPTCHA_HEIGHT`: ukuran gambar CAPTCHA (nilai efektif dibatasi max 399x299 karena batas library).
- `CAPTCHA_OPTION_COUNT`: jumlah tombol pilihan jawaban (default 6).
- `CAPTCHA_ATTEMPTS`: jumlah kesempatan menjawab (default 3).
- `CAPTCHA_OPTION_DIGITS_TO_EMOJI`: ubah digit dan huruf A/B di tombol jadi emoji (A→🅰️, B→🅱️, AB→🆎) (default `true`).
- `DELETE_JOIN_MESSAGE`: hapus pesan join Telegram saat user masuk (default true).
- `DELETE_LEFT_MESSAGE`: hapus pesan left Telegram saat user keluar (default true).
- `BAN_RELEASE_ENABLED`: `true` untuk melepas (unban) user otomatis setelah kick/ban, `false` untuk nonaktif (default `false`).
- `BAN_RELEASE_AFTER_SECONDS`: lama waktu tunggu sebelum unban otomatis (default 14400 = 4 jam).
- `BAN_RELEASE_DB_PATH`: path database SQLite untuk state CAPTCHA dan jadwal auto-unban (default `buktikan.sqlite`; Docker Compose mengarahkannya ke `/data/buktikan.sqlite`).
- `LOG_ENABLED`: `true` untuk tampilkan log, `false` untuk nonaktif.
- `LOG_JSON`: `true` untuk output log JSON, `false` untuk log berwarna.
- `LOG_LEVEL`: `info`, `warn`, atau `error` (default `info`).
- `CAPTCHA_LOG_ENABLED`: `true` untuk kirim log captcha ke chat tertentu, `false` untuk nonaktif (default `false`).
- `CAPTCHA_LOG_CHAT_ID`: ID chat/grup/channel tujuan log captcha.
- `CAPTCHA_LOG_MESSAGE_THREAD_ID`: ID thread/topic forum tujuan log; opsional. Jika kosong, log dikirim ke chat/general topic.
- `TIMEZONE`: zona waktu log, default `Asia/Jakarta`.
- `RUN_MODE`: `polling` (default) atau `webhook`.

Log CAPTCHA mencatat status, waktu kejadian, identitas user dan chat, ID Telegram,
jumlah percobaan, alasan kegagalan, serta tindakan ban. Jika ban sementara
berhasil dijadwalkan, waktu unban otomatis juga dicatat. Log pelepasan ban dikirim
sebagai reply pada log kegagalan yang terkait. Jika pesan parent tidak tersedia
atau merupakan data lama, bot mengirim log pelepasan sebagai pesan biasa.

Toolchain Rust dikunci pada versi `1.98.1` melalui `rust-toolchain.toml` agar
build lokal, CI, dan release menggunakan versi yang konsisten.

Jika ingin menjalankan mode webhook, lihat panduan lengkap di [`WEBHOOK.md`](./WEBHOOK.md).

Catatan Docker: jika memakai image Docker, contoh env bisa ditemukan di
`/usr/local/share/telegram-buktikanbot/.env.example`.

### Catatan Permission Docker (/data)
Container berjalan sebagai user non-root (`appuser`, uid `10001`). Jika bind-mount folder host ke `/data`, pastikan folder host tersebut bisa ditulis oleh uid `10001`, misalnya:

```bash
mkdir -p ./data
sudo chown 10001:10001 ./data
```
Alternatif (kurang aman), jalankan container sebagai root dengan `user: "0:0"` di `docker-compose.yml`.

## Perintah Bot (Private)
- `/start`: info bot.
- `/ping`: cek response time.
- `/ver`, `/versi`, `/version`: info versi aplikasi.
- `/pending` atau `/bans`: panel admin berisi daftar ban yang masih tersimpan di SQLite. Hanya user pada `ADMIN_USER_IDS` yang dapat menggunakannya. Panel menyediakan pagination, refresh, dan release melalui tombol inline dengan konfirmasi kedua.

Panel admin hanya mencantumkan ban yang dibuat dan dicatat oleh bot ketika `BAN_RELEASE_ENABLED=true`; ban manual atau ban dari bot lain tidak dapat dideteksi dari database ini. Saat release manual berhasil, job dihapus dari SQLite dan log pelepasan tetap dikirim ke chat/topic log sebagai reply ke log kegagalan terkait jika pesan parent masih tersedia. Log manual juga mencatat ID admin yang melakukan release.

## Versioning

Info perubahan versi dapat dilihat di halaman [Release][releases].

Cek versi saat ini:

```bash
./scripts/version_dump.sh
```

Naikkan versi:

```bash
./scripts/version_bump.sh major
./scripts/version_bump.sh minor
./scripts/version_bump.sh patch
```

## Build dari Source (Alternatif)

Lihat panduan lengkap di [BUILD_FROM_SOURCE.md](BUILD_FROM_SOURCE.md).

## Pemeriksaan Pull Request

Setiap Pull Request menjalankan pemeriksaan otomatis melalui GitHub Actions:

- format kode dengan `cargo fmt`;
- unit test dengan `cargo test --locked`;
- lint dengan `cargo clippy --locked --all-targets -- -D warnings`.

Workflow ini tidak berjalan pada push biasa.

## Docker

### Pull Image (GHCR)

```bash
docker pull ghcr.io/banghasan/telegram-buktikanbot:<versi>
```

Gunakan tag versi yang eksplisit agar image yang dijalankan dapat direproduksi.

### Docker Compose

1) Isi `.env` dan pastikan token bot terisi.

Compose memakai tag versi yang tetap agar deployment reproducible. Saat upgrade, set
`BOT_IMAGE` ke tag versi yang ingin dipakai.

2) Jalankan:

```bash
docker compose up -d
```

Contoh upgrade image:

```bash
BOT_IMAGE=ghcr.io/banghasan/telegram-buktikanbot:1.9.4 docker compose up -d
```

Override nilai `.env` saat menjalankan:

```bash
BOT_TOKEN=your-telegram-bot-token CAPTCHA_TIMEOUT_SECONDS=180 CAPTCHA_CAPTION_UPDATE_SECONDS=10 docker compose up -d
```

Untuk mode webhook via Docker Compose, lihat [`WEBHOOK.md`](./WEBHOOK.md) dan contoh [`docker-compose.webhook.yml`](./docker-compose.webhook.yml).

## Cara Kerja Singkat
1. Bot mendeteksi user baru yang masuk grup.
2. Bot mengirim gambar CAPTCHA.
3. User wajib menjawab dalam waktu `CAPTCHA_TIMEOUT_SECONDS`.
4. Benar: bot hapus gambar + jawaban.
5. Salah atau timeout: bot kick user.

## Catatan
- State CAPTCHA disimpan di SQLite. Jika bot restart, sesi yang masih aktif akan direkonsiliasi dan CAPTCHA baru dikirim; user tetap dibatasi selama proses pemulihan.
- Jika `BAN_RELEASE_ENABLED=true`, ban memakai `until_date` Telegram sekaligus dicatat ke SQLite. `until_date` menjadi fallback otomatis bila worker aplikasi atau container berhenti.
- Untuk keamanan, jangan commit file `.env` ke repo.
- Pastikan bot punya izin admin di grup sesuai daftar di bagian "Persyaratan".
- Jika memakai webhook lewat proxy SSL (misalnya Cloudflare) dan tombol inline tidak merespons, pastikan header `X-Telegram-Bot-Api-Secret-Token` diteruskan. Jika tidak bisa, kosongkan `WEBHOOK_SECRET_TOKEN` untuk sementara.

## Credit
- Hasanudin H Syafaat @hasanudinhs
- banghasan@gmail.com / https://banghasan.com
- https://botindonesia.web.id

Diskusi dan support di grup Telegram [@botindonesia](https://t.me/botindonesia).

[releases]: https://github.com/banghasan/telegram-buktikanbot/releases

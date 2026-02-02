# Bot Telegram - Buktikan Dirimu Manusia

Bot Telegram untuk memverifikasi user baru yang masuk grup menggunakan CAPTCHA bergambar. Ketika user masuk grup, hak akses dicabut semua -- hanya bisa kirim teks.
User wajib menebak teks pada gambar dalam waktu tertentu, jika tidak menjawab dengan benar dalam waktu tertentu,
maka bot akan mengeluarkan user dari grup. Jika benar, bot menghapus gambar CAPTCHA
dan hak akses user dipulihkan.

![](./screenshot/buktikan.jpg)

## Fitur
- Kirim CAPTCHA gambar ke user baru.
- Panjang teks CAPTCHA dapat diatur lewat `.env`.
- User baru bergabung akan dibatasi hanya boleh kirim teks saja
- Timeout verifikasi (default 120 detik), bisa disesuaikan sendiri.
- Interval update caption (default 10 detik), bisa disesuaikan sendiri.
- Jawaban benar: hapus pesan CAPTCHA + pesan jawaban.
- Jawaban salah terhapus, jika timeout: kick user dari grup.
- User terverifikasi, hak akses grup dipulihkan.

## Persyaratan
- Bot Telegram yang sudah dibuat lewat BotFather.
- Bot jadi admin grup dengan izin:
  - Delete messages (hapus pesan CAPTCHA + jawaban user)
  - Ban users / Restrict members (batasi user ke text-only dan kick saat timeout)
  - (Opsional) Manage messages jika ingin bot bisa menghapus pesan di semua tipe grup

## Menjalankan dari Release

1) Unduh file release sesuai OS/arsitektur:
   - `buktikanbot-<versi>-x86_64-unknown-linux-gnu.tar.gz`
   - `buktikanbot-<versi>-aarch64-unknown-linux-gnu.tar.gz`
   - `buktikanbot-<versi>-x86_64-apple-darwin.tar.gz`
   - `buktikanbot-<versi>-aarch64-apple-darwin.tar.gz`
   - `buktikanbot-<versi>-x86_64-pc-windows-msvc.zip`

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
Isi `.env`:

```env
BOT_TOKEN=your-telegram-bot-token
CAPTCHA_LEN=6
CAPTCHA_TIMEOUT_SECONDS=120
CAPTCHA_CAPTION_UPDATE_SECONDS=10
CAPTCHA_WIDTH=220
CAPTCHA_HEIGHT=100
CAPTCHA_OPTION_COUNT=6
CAPTCHA_ATTEMPTS=3
DELETE_JOIN_MESSAGE=true
DELETE_LEFT_MESSAGE=true
LOG_ENABLED=true
LOG_JSON=false
LOG_LEVEL=info
TIMEZONE=Asia/Jakarta
```

Keterangan variabel:
- `BOT_TOKEN`: token bot Telegram.
- `CAPTCHA_LEN`: panjang karakter CAPTCHA.
- `CAPTCHA_TIMEOUT_SECONDS`: waktu maksimum menebak.
- `CAPTCHA_CAPTION_UPDATE_SECONDS`: interval update caption countdown (default 10 detik).
- `CAPTCHA_WIDTH` / `CAPTCHA_HEIGHT`: ukuran gambar CAPTCHA.
- `CAPTCHA_OPTION_COUNT`: jumlah tombol pilihan jawaban (default 6).
- `CAPTCHA_ATTEMPTS`: jumlah kesempatan menjawab (default 3).
- `DELETE_JOIN_MESSAGE`: hapus pesan join Telegram saat user masuk (default true).
- `DELETE_LEFT_MESSAGE`: hapus pesan left Telegram saat user keluar (default true).
- `LOG_ENABLED`: `true` untuk tampilkan log, `false` untuk nonaktif.
- `LOG_JSON`: `true` untuk output log JSON, `false` untuk log berwarna.
- `LOG_LEVEL`: `info`, `warn`, atau `error` (default `info`).
- `TIMEZONE`: zona waktu log, default `Asia/Jakarta`.
- `RUN_MODE`: `polling` (default) atau `webhook`.

Jika ingin menjalankan mode webhook, lihat panduan lengkap di [`WEBHOOK.md`](./WEBHOOK.md).

## Perintah Bot (Private)
- `/start`: info bot.
- `/ping`: cek response time.
- `/ver`, `/versi`, `/version`: info versi aplikasi.

## Versioning

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

### Pra Syarat
- Rust (edisi 2024) dan Cargo.

### Build dan Run

Jalankan langsung:

```bash
cargo run
```

Atau build release:

```bash
cargo build --release
```

Hasil binary ada di:

```text
target/release/telegram-buktikanbot
```

Jalankan binary hasil build:

```bash
./target/release/telegram-buktikanbot
```

## Docker

### Pull Image (GHCR)

```bash
docker pull ghcr.io/banghasan/telegram-buktikanbot:latest
```

Atau gunakan tag versi:

```bash
docker pull ghcr.io/banghasan/telegram-buktikanbot:<versi>
```

### Docker Compose

1) Isi `.env` dan pastikan token bot terisi.

2) Jalankan:

```bash
docker compose up -d
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
- State verifikasi disimpan di memori. Jika bot restart, state pending akan hilang.
- Untuk keamanan, jangan commit file `.env` ke repo.
- Pastikan bot punya izin admin di grup sesuai daftar di bagian "Persyaratan".

## Credit
- Hasanudin H Syafaat @hasanudinhs
- banghasan@gmail.com
- https://banghasan.com

Diskusi dan support di grup Telegram [@botindonesia](https://t.me/botindonesia).

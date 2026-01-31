# Telegram Buktikan Bot (Rust)

Bot Telegram untuk memverifikasi user baru yang masuk grup menggunakan CAPTCHA bergambar.
User wajib menebak teks pada gambar dalam waktu tertentu, jika salah atau tidak menjawab
maka bot akan mengeluarkan user dari grup. Jika benar, bot menghapus gambar CAPTCHA
dan pesan jawaban user.

## Fitur
- Kirim CAPTCHA gambar ke user baru.
- Panjang teks CAPTCHA dapat diatur lewat `.env`.
- Timeout verifikasi (default 120 detik).
- Jawaban benar: hapus pesan CAPTCHA + pesan jawaban.
- Jawaban salah / timeout: kick user dari grup.

## Persyaratan
- Rust (edisi 2024) dan Cargo.
- Bot Telegram yang sudah dibuat lewat BotFather.
- Bot jadi admin grup dengan izin:
  - Delete messages
  - Ban users

## Konfigurasi
Salin contoh file `.env`:

```bash
cp .env.example .env
```

Isi `.env`:

```env
BOT_TOKEN=your-telegram-bot-token
CAPTCHA_LEN=6
CAPTCHA_TIMEOUT_SECONDS=120
CAPTCHA_WIDTH=220
CAPTCHA_HEIGHT=100
LOG_ENABLED=true
TIMEZONE=Asia/Jakarta
```

Keterangan variabel:
- `BOT_TOKEN`: token bot Telegram.
- `CAPTCHA_LEN`: panjang karakter CAPTCHA.
- `CAPTCHA_TIMEOUT_SECONDS`: waktu maksimum menebak.
- `CAPTCHA_WIDTH` / `CAPTCHA_HEIGHT`: ukuran gambar CAPTCHA.
- `LOG_ENABLED`: `true` untuk tampilkan log, `false` untuk nonaktif.
- `TIMEZONE`: zona waktu log, default `Asia/Jakarta`.

## Cara Menjalankan

```bash
cargo run
```

## Cara Compile (Build)

Build release:

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

## Cara Kerja Singkat
1. Bot mendeteksi user baru yang masuk grup.
2. Bot mengirim gambar CAPTCHA.
3. User wajib menjawab dalam waktu `CAPTCHA_TIMEOUT_SECONDS`.
4. Benar: bot hapus gambar + jawaban.
5. Salah atau timeout: bot kick user.

## Catatan
- State verifikasi disimpan di memori. Jika bot restart, state pending akan hilang.
- Untuk keamanan, jangan commit file `.env` ke repo.

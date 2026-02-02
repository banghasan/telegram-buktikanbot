# Webhook Mode (Teloxide)

Dokumentasi ini menjelaskan cara menjalankan bot dalam mode webhook memakai listener bawaan Teloxide.

## Ringkas
- Mode default: polling.
- Untuk webhook: set `RUN_MODE=webhook` dan `WEBHOOK_URL`.
- Telegram hanya mengirim ke URL HTTPS publik (port 443/80/88/8443).
- Disarankan gunakan reverse proxy untuk TLS dan routing.

## Environment
Wajib:
- `RUN_MODE=webhook`
- `WEBHOOK_URL=https://domain-anda.com` (base URL publik HTTPS)

Opsional:
- `WEBHOOK_PATH=/telegram` (default `/telegram`)
- `WEBHOOK_LISTEN_ADDR=0.0.0.0` (default)
- `WEBHOOK_PORT=8080` (default)
- `WEBHOOK_SECRET_TOKEN=...` (opsional, untuk verifikasi header Telegram)

Bot akan membangun URL final: `WEBHOOK_URL + WEBHOOK_PATH`.

## Reverse Proxy (Disarankan)
Telegram mengirim HTTPS ke reverse proxy, lalu proxy meneruskan HTTP ke bot:

```
Telegram -> HTTPS -> Reverse Proxy -> HTTP -> Bot
```

Contoh Nginx:

```
location /telegram {
  proxy_pass http://127.0.0.1:8080/telegram;
  proxy_set_header Host $host;
  proxy_set_header X-Real-IP $remote_addr;
}
```

Pastikan `WEBHOOK_URL` mengarah ke endpoint yang di-proxy:

```
WEBHOOK_URL=https://domain-anda.com
WEBHOOK_PATH=/telegram
```

## Contoh Ngrok (Dev/Testing)
Untuk testing cepat, gunakan ngrok agar punya HTTPS publik sementara.

1) Jalankan bot dengan listen lokal:
```
RUN_MODE=webhook
WEBHOOK_LISTEN_ADDR=127.0.0.1
WEBHOOK_PORT=8080
WEBHOOK_PATH=/telegram
```

2) Jalankan ngrok:
```
ngrok http 8080
```

3) Ambil URL HTTPS dari output ngrok, lalu set:
```
WEBHOOK_URL=https://<id>.ngrok-free.app
```

Bot akan set webhook ke `https://<id>.ngrok-free.app/telegram`.

## Secret Token (Opsional)
Jika `WEBHOOK_SECRET_TOKEN` diset, Telegram akan mengirim header:

```
X-Telegram-Bot-Api-Secret-Token: <token>
```

Teloxide akan memverifikasi header ini otomatis. Token harus 1..256 karakter dan hanya boleh berisi `A-Z`, `a-z`, `0-9`, `_`, `-`.

## Polling vs Webhook
- Polling: bot aktif menarik update.
- Webhook: Telegram mendorong update ke server kamu.

Jika pindah dari webhook ke polling, bot akan otomatis memanggil `deleteWebhook`.

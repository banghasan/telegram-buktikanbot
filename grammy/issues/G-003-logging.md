# G-003 — Logging dan Error Handling

## Tujuan

Mempertahankan observability dan konteks error yang saat ini tersedia di Rust.

## Acceptance criteria

- event join, CAPTCHA, verification, timeout, ban, dan restore tercatat;
- log memiliki chat/user context yang aman;
- error Telegram tidak menampilkan token;
- format JSON dan level log memiliki perilaku terdokumentasi;
- production log memakai correlation ID berbasis chat/user;
- kegagalan logging tidak mematikan alur verifikasi utama tanpa alasan.

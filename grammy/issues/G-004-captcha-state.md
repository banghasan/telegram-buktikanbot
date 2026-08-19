# G-004 — CAPTCHA Generator dan State Machine

## Tujuan

Memindahkan pembuatan CAPTCHA dan state pending dengan perilaku yang setara.

## Acceptance criteria

- panjang, ukuran, pilihan, dan attempts mengikuti konfigurasi;
- state diisolasi berdasarkan chat dan user;
- jawaban dibandingkan dengan aman;
- success, failure, dan expiry bersifat idempotent;
- state tidak tertinggal setelah terminal state.

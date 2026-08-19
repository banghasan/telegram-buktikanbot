# G-005 — Membership dan Permission

## Tujuan

Menangani user baru dan perubahan membership dengan aturan keamanan yang sama seperti Rust.

## Acceptance criteria

- join melalui update message dan chat member dipertimbangkan;
- bot membatasi permission sebelum CAPTCHA dikirim atau sesuai urutan aman;
- bot tidak memproses bot account;
- user lain tidak dapat memengaruhi state;
- permission dipulihkan sesuai permission grup yang berlaku.

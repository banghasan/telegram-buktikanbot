# G-006 — Ephemeral CAPTCHA Flow

## Tujuan

Menggunakan Ephemeral Messages Bot API 10.2 untuk menyembunyikan CAPTCHA dari anggota grup lain.

## Acceptance criteria

- CAPTCHA photo dikirim kepada receiver user yang benar;
- ephemeral_message_id disimpan dan digunakan dengan benar;
- callback ephemeral tervalidasi terhadap user dan pending state;
- caption/media/keyboard dapat diperbarui;
- pesan dapat dihapus pada success, timeout, dan attempts exhausted;
- pengiriman ephemeral dicoba maksimal dua kali;
- fallback pesan biasa di grup tetap hanya menerima callback button;
- fallback delivery diuji pada client yang tidak mendukung ephemeral.

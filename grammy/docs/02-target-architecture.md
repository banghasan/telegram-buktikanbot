# Arsitektur Target grammY

## Prinsip

Implementasi baru harus memisahkan Telegram transport dari domain verifikasi. Tujuannya agar perubahan Bot API tidak menyebar ke seluruh business logic.

## Modul target

- config: parsing dan validasi environment.
- telegram: pembuatan Bot, middleware, update listener, dan API adapter.
- handlers: routing update join, callback, command, dan event anggota.
- verification: state machine CAPTCHA dan aturan percobaan.
- ephemeral: pengiriman, edit, reply, dan delete Ephemeral Messages.
- permissions: restrict dan restore permission.
- persistence: SQLite dan jadwal ban-release.
- logging: event terstruktur dan sanitasi data.
- captcha: generator gambar, pilihan jawaban, dan caption.
- runtime: startup, graceful shutdown, polling/webhook, dan health behavior.

## Alur target

~~~text
Telegram update
  -> grammY middleware
  -> handler membership
  -> verification service
  -> permission service
  -> ephemeral message service
  -> persistence and logging
~~~

## State CAPTCHA

State pending minimal harus menyimpan chat ID, user ID, kode jawaban yang dilindungi, daftar pilihan, jumlah percobaan, waktu kedaluwarsa, ID pesan ephemeral, metadata user/chat untuk log, dan status terminal jika diperlukan untuk idempotency.

Untuk pesan ephemeral, ID yang perlu disimpan adalah ephemeral_message_id. message_id pada pesan ephemeral tidak dapat diperlakukan seperti ID pesan grup biasa.

## Persistence

Schema SQLite dipertahankan terlebih dahulu. Perubahan schema harus dibahas pada issue persistence, bukan dilakukan diam-diam saat migrasi handler.

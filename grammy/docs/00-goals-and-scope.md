# Tujuan dan Ruang Lingkup Migrasi

## Latar belakang

Aplikasi saat ini adalah bot Rust dengan Teloxide 0.12.2. Bot memverifikasi anggota baru menggunakan CAPTCHA bergambar di grup. Telegram Bot API 10.2 memperkenalkan Ephemeral Messages, yang memungkinkan bot mengirim interaksi grup yang hanya terlihat oleh user tertentu dan bot.

Fitur tersebut cocok untuk CAPTCHA karena gambar dan tombol verifikasi tidak perlu memenuhi timeline grup.

## Tujuan

- Mengganti implementasi bot ke grammY dan TypeScript.
- Menggunakan Bun untuk seluruh workflow TypeScript.
- Mendukung Ephemeral Messages secara typed dan teruji.
- Menjaga perilaku verifikasi dan keamanan yang sudah ada.
- Menyediakan jalur rollback ke aplikasi Rust selama migrasi.

## Tidak termasuk dalam fase awal

- Mengubah algoritma CAPTCHA.
- Mengubah aturan permission grup.
- Mengubah format log tanpa kebutuhan.
- Mengganti SQLite dengan database lain.
- Menambah fitur bisnis baru yang tidak terkait migrasi.
- Menjalankan dua bot produksi secara bersamaan.

## Definisi selesai migrasi

Migrasi baru dapat dipertimbangkan siap produksi jika semua alur inti setara dengan Rust, Ephemeral Messages lulus uji pada client Telegram yang ditargetkan, deployment Bun terdokumentasi, dan rollback sudah diuji.

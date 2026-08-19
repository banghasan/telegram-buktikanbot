# Rencana Migrasi grammY

Direktori ini adalah area kerja untuk rencana penggantian bot Rust/Teloxide dengan grammY berbasis TypeScript dan Bun.

## Status saat ini

Status: **documentation-only / belum mulai coding**

Dokumen di sini belum menjadi spesifikasi final sampai dibahas dan disetujui. Tidak ada proses produksi yang boleh diarahkan ke direktori ini pada tahap sekarang.

## Tujuan migrasi

1. Mendukung Bot API Telegram terbaru, terutama Ephemeral Messages.
2. Mengirim CAPTCHA verifikasi hanya kepada anggota yang sedang diverifikasi.
3. Mempertahankan restrict, timeout, ban/kick, restore permission, dan pencatatan.
4. Memudahkan pemeliharaan ketika Telegram menambah tipe atau method API baru.
5. Menggunakan Bun sebagai satu-satunya runtime, package manager, test runner, dan task runner.

## Batasan

Migrasi tidak boleh mengubah aturan verifikasi tanpa keputusan eksplisit. Fitur lama harus dipetakan dahulu:

- CAPTCHA bergambar dan pilihan tombol;
- pembatasan permission anggota baru;
- batas waktu dan jumlah percobaan;
- penghapusan pesan join/left;
- ban dan pelepasan ban terjadwal;
- SQLite;
- logging dan timezone;
- polling dan webhook;
- konfigurasi environment;
- Docker dan deployment.

## Peta dokumentasi

- [Aturan pengembangan](./AGENTS.md)
- [Kondisi aplikasi Rust saat ini](./docs/01-current-state.md)
- [Tujuan dan ruang lingkup](./docs/00-goals-and-scope.md)
- [Arsitektur target](./docs/02-target-architecture.md)
- [Rencana fase migrasi](./docs/03-migration-plan.md)
- [Desain Ephemeral Messages](./docs/04-ephemeral-verification.md)
- [Kompatibilitas konfigurasi dan data](./docs/05-data-and-configuration.md)
- [Strategi testing dan rollout](./docs/06-testing-and-rollout.md)
- [Pertanyaan dan keputusan yang belum final](./docs/07-open-decisions.md)
- [Daftar issue implementasi](./issues/README.md)

## Prinsip kerja

- Rust lama tetap menjadi baseline sampai pengganti terbukti setara.
- Bot staging memakai token dan grup staging yang berbeda.
- Satu token produksi hanya boleh aktif pada satu implementasi pada satu waktu.
- Tidak ada penghapusan source Rust selama fase validasi.
- Setiap issue memiliki acceptance criteria sebelum dianggap selesai.

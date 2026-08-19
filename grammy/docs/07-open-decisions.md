# Pertanyaan dan Keputusan yang Belum Final

Dokumen ini hanya berisi detail yang masih terbuka. Keputusan utama sudah dicatat di [Keputusan yang Sudah Disepakati](./08-agreed-decisions.md).

## Keputusan teknis

- Versi Bun minimum.
- Versi grammY yang akan dipakai.
- Library SQLite yang kompatibel dengan Bun.
- Apakah generator CAPTCHA tetap dipertahankan melalui library TypeScript atau dibuat ulang.
- Apakah webhook memakai Bun.serve atau framework HTTP tambahan.
- Format logging final.

## Keputusan produk

- Detail perilaku pesan non-button selama user restricted.
- Apakah countdown tetap diedit berkala.

## Keputusan operasi

- Nama image Docker grammY.
- Lokasi deployment staging.
- Strategi backup SQLite.
- Syarat go/no-go cutover.
- Durasi observasi setelah cutover.

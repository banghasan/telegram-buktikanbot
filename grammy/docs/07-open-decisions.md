# Pertanyaan dan Keputusan yang Belum Final

Dokumen ini menjadi daftar diskusi sebelum coding dimulai.

## Keputusan teknis

- Versi Bun minimum.
- Versi grammY yang akan dipakai.
- Library SQLite yang kompatibel dengan Bun.
- Apakah generator CAPTCHA tetap dipertahankan melalui library TypeScript atau dibuat ulang.
- Apakah webhook memakai Bun.serve atau framework HTTP tambahan.
- Format logging final.

## Keputusan produk

- Fallback jika Ephemeral Messages tidak diterima client.
- Apakah CAPTCHA langsung dikirim ke grup sebagai ephemeral atau user diarahkan ke private chat bot.
- Apakah countdown tetap diedit berkala.
- Perilaku tepat untuk user yang offline.
- Kebijakan kick versus ban dan durasi pelepasan ban.

## Keputusan operasi

- Nama image Docker grammY.
- Lokasi deployment staging.
- Strategi backup SQLite.
- Syarat go/no-go cutover.
- Durasi observasi setelah cutover.

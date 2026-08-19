# G-011 — Cutover dan Rollback

## Tujuan

Memindahkan token produksi ke grammY dengan risiko operasional yang terukur.

## Acceptance criteria

- checklist cutover disetujui;
- backup database tersedia;
- Rust dapat dinyalakan kembali;
- tidak ada dua process aktif dengan token produksi yang sama;
- pending CAPTCHA dan behavior rollback sudah dipahami;
- periode observasi dan indikator rollback ditetapkan.

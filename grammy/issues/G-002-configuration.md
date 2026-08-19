# G-002 — Konfigurasi dan Environment

## Tujuan

Memindahkan konfigurasi Rust ke layanan grammY tanpa memutus deployment atau mengubah default secara tidak sengaja.

## Acceptance criteria

- semua variable baseline terpetakan;
- default dan batas nilai terdokumentasi;
- secret tidak masuk repository atau log;
- konfigurasi invalid menghasilkan error startup yang jelas;
- env.example grammY memakai nama yang telah disetujui.

# G-007 — SQLite dan Ban-Release

## Tujuan

Mempertahankan jadwal pelepasan ban dan data operasional tanpa kehilangan kompatibilitas.

## Acceptance criteria

- schema Rust dipetakan;
- path database dapat dikonfigurasi;
- jadwal survive restart;
- duplicate schedule aman;
- backup dan rollback terdokumentasi;
- migrasi schema, jika ada, diuji pada salinan database.

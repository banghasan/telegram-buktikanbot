# G-009 — Test Suite dan Staging

## Tujuan

Membuktikan parity behavior sebelum deployment produksi.

## Acceptance criteria

- unit test tidak memerlukan token Telegram;
- callback ownership, callback ganda, timeout, retry, race condition, dan idempotency diuji;
- format check, lint, dan strict TypeScript type-check lulus;
- integration test adapter berjalan tanpa token production;
- staging memakai bot token dan database terpisah;
- alur ephemeral diuji pada client Telegram yang ditargetkan;
- hasil uji dicatat sebelum cutover.

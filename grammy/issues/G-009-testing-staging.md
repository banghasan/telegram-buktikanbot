# G-009 — Test Suite dan Staging

## Tujuan

Membuktikan parity behavior sebelum deployment produksi.

## Acceptance criteria

- unit test tidak memerlukan token Telegram;
- callback ownership, timeout, retry, dan idempotency diuji;
- staging memakai bot token dan database terpisah;
- alur ephemeral diuji pada client Telegram yang ditargetkan;
- hasil uji dicatat sebelum cutover.

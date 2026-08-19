# G-010 — Docker, CI, dan Deployment Bun

## Tujuan

Menyiapkan artifact grammY yang konsisten dengan aturan Bun-only.

## Acceptance criteria

- Docker image menjalankan Bun tanpa Node.js package workflow;
- CI install, test, dan build memakai Bun;
- image tidak berisi secret;
- volume SQLite tetap kompatibel;
- polling dan webhook deployment memiliki dokumentasi terpisah.

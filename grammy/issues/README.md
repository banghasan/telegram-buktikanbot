# Issue Migrasi grammY

Issue di direktori ini adalah backlog implementasi yang dapat ditinjau satu per satu sebelum coding.

| ID | Topik | Status | Dependensi |
|---|---|---|---|
| G-001 | Fondasi Bun dan grammY | Proposed | - |
| G-002 | Konfigurasi dan environment | Proposed | G-001 |
| G-003 | Logging dan error handling | Proposed | G-001, G-002 |
| G-004 | CAPTCHA generator dan state machine | Proposed | G-001 |
| G-005 | Membership dan permission | Proposed | G-002, G-004 |
| G-006 | Ephemeral CAPTCHA flow | Proposed | G-004, G-005 |
| G-007 | SQLite dan ban-release | Proposed | G-002, G-003 |
| G-008 | Polling, webhook, dan shutdown | Proposed | G-001, G-002 |
| G-009 | Test suite dan staging | Proposed | G-003 sampai G-008 |
| G-010 | Docker, CI, dan deployment Bun | Proposed | G-001, G-008, G-009 |
| G-011 | Cutover dan rollback | Proposed | G-007, G-009, G-010 |

Urutan ini bukan izin untuk langsung coding. Setiap issue harus dibahas, acceptance criteria-nya disetujui, lalu baru dikerjakan.

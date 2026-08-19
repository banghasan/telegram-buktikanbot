# Quality Gates grammY

Dokumen ini menetapkan standar minimum sebelum implementasi grammY dianggap siap direview atau dirilis.

## Checks wajib

Semua tooling TypeScript menggunakan Bun:

1. format check;
2. lint;
3. strict TypeScript type-check;
4. unit test;
5. integration test tanpa token production;
6. build artifact;
7. pemeriksaan secret dan file sensitif.

## Unit test

Unit test wajib mencakup:

- parsing dan validasi konfigurasi;
- CAPTCHA generator;
- state machine pending, verified, expired, dan failed;
- ownership chat/user;
- jumlah attempts;
- ban-release duration;
- fallback decision;
- idempotency;
- sanitasi log.

## Integration test

Integration test wajib mencakup adapter Telegram dan persistence tanpa memakai token production. Test harus bisa mensimulasikan:

- update anggota baru;
- callback button;
- ephemeral API success;
- ephemeral API failure dan retry;
- fallback pesan grup;
- callback ganda;
- timeout bersamaan dengan callback;
- kegagalan permission;
- restart dan pemulihan pending state.

## Pull request gate

Pull request tidak boleh dianggap siap jika:

- salah satu quality check gagal;
- ada token atau database lokal yang ikut berubah;
- acceptance criteria issue belum terpenuhi;
- dokumentasi behavior belum diperbarui;
- tidak ada catatan untuk perubahan schema atau permission.

## Release gate

Sebelum production:

- staging test lulus;
- backup SQLite tersedia;
- rollback Rust teruji;
- webhook secret tervalidasi;
- hanya satu process memakai token production;
- dashboard/log monitoring siap digunakan.

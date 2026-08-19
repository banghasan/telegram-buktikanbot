# Version Bump Script

Dokumen ini menjelaskan cara memakai script `scripts/version_bump.sh`.

## Ringkas

Script ini menaikkan versi di `Cargo.toml` berdasarkan pilihan:

- `major`: naikkan X.0.0 (reset minor/patch)
- `minor`: naikkan 0.X.0 (reset patch)
- `patch`: naikkan 0.0.X

## Cara Pakai

### Mode interaktif (tanpa argumen)

```bash
./scripts/version_bump.sh
```

Anda akan diminta memilih:

```
Pilih jenis bump versi:
  1) major - naikkan X.0.0 (ubah besar, reset minor/patch)
  2) minor - naikkan 0.X.0 (fitur baru, reset patch)
  3) patch - naikkan 0.0.X (perbaikan kecil)
  0) batal - keluar tanpa perubahan
Masukkan pilihan (0/1/2/3):
```

### Mode langsung (dengan argumen)

```bash
./scripts/version_bump.sh major
./scripts/version_bump.sh minor
./scripts/version_bump.sh patch
```

## Output

Setelah sukses, script akan menulis versi baru ke `Cargo.toml` dan menampilkan:

```
bumped to X.Y.Z
```

## Catatan Warna

Saat mode interaktif, pilihan akan ditampilkan berwarna jika output adalah terminal (TTY). Jika dijalankan di CI atau output dialihkan ke file, warna otomatis dimatikan.

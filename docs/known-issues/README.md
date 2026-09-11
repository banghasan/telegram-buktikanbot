# Known Issues dan Catatan Maintenance

Direktori ini berisi catatan teknis yang sengaja dipisahkan dari source code.
Isinya bukan workaround aplikasi, melainkan pengingat untuk dependency, toolchain,
atau kondisi build yang perlu dipantau pada upgrade berikutnya.

## Daftar catatan

- [Future-incompatibility pada `proc-macro-error2`](./proc-macro-error2-future-incompatibility.md)

Setiap catatan sebaiknya diperbarui ketika salah satu kondisi berikut terjadi:

- dependency terkait mendapatkan versi baru;
- Rust stable baru mengubah warning menjadi error;
- warning sudah tidak muncul setelah upgrade;
- keputusan maintenance berubah dan memerlukan tindakan pada source code.

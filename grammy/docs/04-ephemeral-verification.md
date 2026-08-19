# Desain Verifikasi dengan Ephemeral Messages

## Tujuan

CAPTCHA hanya terlihat oleh user yang harus menyelesaikannya. Timeline grup tidak dipenuhi gambar CAPTCHA, tombol jawaban, atau perubahan countdown.

## Alur yang direncanakan

1. User masuk ke grup.
2. Bot mengosongkan permission user seperti implementasi Rust.
3. Bot mengirim CAPTCHA photo ke chat grup dengan receiver_user_id user tersebut.
4. Bot menyimpan ephemeral_message_id bersama pending state.
5. User menekan tombol jawaban.
6. Bot memastikan callback berasal dari user yang memiliki pending state.
7. Jawaban salah memperbarui CAPTCHA ephemeral.
8. Jawaban benar menghapus pesan ephemeral lalu restore permission.
9. Timeout menghapus pesan ephemeral lalu ban/kick sesuai kebijakan.

## Method API yang diperlukan

- sendPhoto dengan receiver_user_id;
- editEphemeralMessageMedia;
- editEphemeralMessageCaption;
- editEphemeralMessageReplyMarkup;
- deleteEphemeralMessage.

## Batasan dan fallback

- Delivery ephemeral tidak dijamin jika user offline.
- Bot dapat mendeteksi kegagalan request API, tetapi tidak dapat memastikan apakah client user benar-benar menampilkan pesan.
- Ephemeral tidak menggantikan restrict permission.
- Bot harus menjadi administrator sesuai kebutuhan restrictChatMember, ban, dan penghapusan.
- Callback dari pesan ephemeral harus dipetakan memakai data ephemeral, bukan mengasumsikan message_id biasa.
- Request pengiriman ephemeral dicoba maksimal dua kali: percobaan pertama langsung, retry kedua setelah jeda tiga detik.
- Jika dua request gagal di level API, bot kembali ke CAPTCHA pesan biasa di grup.
- Jika request sukses tetapi user offline, Telegram tidak memberi sinyal delivery yang dapat dijadikan dasar retry.
- Fallback tetap memakai inline button dan tidak menerima jawaban teks.

## Keputusan yang masih perlu disetujui

- Berapa lama pending state dipertahankan setelah fallback.
- Apakah countdown tetap diedit berkala atau hanya ditampilkan sebagai waktu kedaluwarsa awal?
- Client Telegram minimum apa yang harus didukung?

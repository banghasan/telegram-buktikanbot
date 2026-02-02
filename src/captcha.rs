use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use captcha::Captcha;
use captcha::filters::Noise;
use teloxide::types::{ChatId, MessageId, UserId};
use tokio::sync::Mutex;

use crate::utils::{escape_html, format_user_display};

#[derive(Clone, Debug)]
pub struct PendingCaptcha {
    pub code: String,
    pub captcha_message_id: MessageId,
    pub user_display: String,
    pub chat_title: Option<String>,
    pub chat_username: Option<String>,
}

pub type CaptchaKey = (ChatId, UserId);
pub type SharedState = Arc<Mutex<HashMap<CaptchaKey, PendingCaptcha>>>;

pub enum CaptchaCheck {
    NoPending,
    Wrong,
    Verified(PendingCaptcha),
}

pub fn generate_captcha(
    length: usize,
    width: u32,
    height: u32,
) -> Result<(String, Vec<u8>), Box<dyn Error + Send + Sync>> {
    let mut captcha = Captcha::new();
    captcha
        .add_chars(length as u32)
        .apply_filter(Noise::new(0.4))
        .view(width, height);

    let code = captcha.chars_as_string();
    let png = captcha.as_png().ok_or("failed to render captcha")?;
    Ok((code, png))
}

pub fn captcha_caption(user: &teloxide::types::User, remaining_secs: u64) -> String {
    let name = escape_html(&user.first_name);
    let mention = format!("<a href=\"tg://user?id={}\">{}</a>", user.id.0, name);
    format!(
        "🖐🏼 Hi, {mention}\n\n\
🙏🏼 Please solve this captcha within <code>{remaining_secs}</code> seconds.\n\
💁🏻‍♂️ Mohon ketik teks pada gambar ini, dalam <code>{remaining_secs}</code> detik.\n\n
🗒 <i>Setiap ketikan akan terhapus hingga kamu terverifikasi</i>.
"
    )
}

pub fn make_pending_captcha(
    code: String,
    captcha_message_id: MessageId,
    user: &teloxide::types::User,
    chat_title: Option<String>,
    chat_username: Option<String>,
) -> PendingCaptcha {
    PendingCaptcha {
        code,
        captcha_message_id,
        user_display: format_user_display(user),
        chat_title,
        chat_username,
    }
}

pub fn check_captcha_answer(
    state: &mut HashMap<CaptchaKey, PendingCaptcha>,
    key: CaptchaKey,
    text: &str,
) -> CaptchaCheck {
    let Some(pending) = state.get(&key).cloned() else {
        return CaptchaCheck::NoPending;
    };
    if text.eq_ignore_ascii_case(&pending.code) {
        state.remove(&key);
        CaptchaCheck::Verified(pending)
    } else {
        CaptchaCheck::Wrong
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_captcha_answer_marks_verified_and_removes() {
        let mut state: HashMap<CaptchaKey, PendingCaptcha> = HashMap::new();
        let key = (ChatId(1), UserId(2));
        state.insert(
            key,
            PendingCaptcha {
                code: "AbC".to_string(),
                captcha_message_id: MessageId(10),
                user_display: "User @user".to_string(),
                chat_title: Some("Group".to_string()),
                chat_username: Some("groupname".to_string()),
            },
        );
        let wrong = check_captcha_answer(&mut state, key, "nope");
        assert!(matches!(wrong, CaptchaCheck::Wrong));
        assert!(state.contains_key(&key));

        let verified = check_captcha_answer(&mut state, key, "aBc");
        assert!(matches!(verified, CaptchaCheck::Verified(_)));
        assert!(!state.contains_key(&key));
    }
}

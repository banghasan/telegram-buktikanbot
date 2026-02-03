use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use captcha::Captcha;
use captcha::filters::Noise;
use rand::distributions::Alphanumeric;
use rand::{Rng, seq::SliceRandom};
use teloxide::types::{ChatId, MessageId, UserId};
use tokio::sync::Mutex;

use crate::captcha_quotes::CAPTCHA_QUOTES;
use crate::utils::{escape_html, format_user_display};

#[derive(Clone, Debug)]
pub struct PendingCaptcha {
    pub code: String,
    pub captcha_message_id: MessageId,
    pub options: Vec<String>,
    pub attempts_left: usize,
    pub attempts_total: usize,
    pub remaining_secs: u64,
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

pub fn captcha_caption(
    user: &teloxide::types::User,
    remaining_secs: u64,
    attempts_left: usize,
    attempts_total: usize,
) -> String {
    let name = escape_html(&user.first_name);
    let quote = CAPTCHA_QUOTES
        .choose(&mut rand::thread_rng())
        .copied()
        .unwrap_or("Tunjukkan kamu bukan bot.");
    let quote = escape_html(quote);
    let mention = format!("<a href=\"tg://user?id={}\">{}</a>", user.id.0, name);
    format!(
        "🖐🏼 Hi, {mention}\n\n\
🙏🏼 <b>Please solve this captcha.</b>\n\
💁🏻‍♂️ Pilih jawaban yang benar dari tombol yang tersedia.\n\n\
⏳ Dalam <code>{remaining_secs}</code> detik.\n\
🎯 Kesempatan: <code>{attempts_left}</code>/<code>{attempts_total}</code>\n\n\
🗒 <i>{quote}</i>
"
    )
}

pub fn generate_captcha_options(code: &str, count: usize) -> Vec<String> {
    let target = count.max(2);
    let mut options = Vec::with_capacity(target);
    options.push(code.to_string());

    let mut rng = rand::thread_rng();
    while options.len() < target {
        let candidate: String = (0..code.len())
            .map(|_| rng.sample(Alphanumeric) as char)
            .map(|ch| ch.to_ascii_uppercase())
            .collect();
        if options
            .iter()
            .all(|opt| !opt.eq_ignore_ascii_case(&candidate))
        {
            options.push(candidate);
        }
    }

    options.shuffle(&mut rng);
    options
}

pub fn make_pending_captcha(
    code: String,
    captcha_message_id: MessageId,
    options: Vec<String>,
    attempts_total: usize,
    remaining_secs: u64,
    user: &teloxide::types::User,
    chat_title: Option<String>,
    chat_username: Option<String>,
) -> PendingCaptcha {
    PendingCaptcha {
        code,
        captcha_message_id,
        options,
        attempts_left: attempts_total,
        attempts_total,
        remaining_secs,
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
                options: vec!["AbC".to_string(), "ZZZ".to_string()],
                attempts_left: 3,
                attempts_total: 3,
                remaining_secs: 120,
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

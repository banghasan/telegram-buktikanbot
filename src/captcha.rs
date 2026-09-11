use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use captcha::Captcha;
use captcha::filters::Noise;
use rand::{Rng, seq::SliceRandom};
use teloxide::types::{ChatId, MessageId, User, UserId};
use tokio::sync::Mutex;

use crate::captcha_quotes::CAPTCHA_QUOTES;
use crate::utils::{escape_html, format_user_display, format_user_name};

#[derive(Clone, Debug)]
pub struct PendingCaptcha {
    pub code: String,
    pub captcha_message_id: MessageId,
    pub options: Vec<String>,
    pub attempts_left: usize,
    pub attempts_total: usize,
    pub remaining_secs: u64,
    pub expires_at: i64,
    pub user_display: String,
    pub user_name: String,
    pub user_username: Option<String>,
    pub chat_title: Option<String>,
    pub chat_username: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptchaSession {
    pub chat_id: i64,
    pub user_id: i64,
    pub code: String,
    pub captcha_message_id: i32,
    pub options: Vec<String>,
    pub attempts_left: i64,
    pub attempts_total: i64,
    pub expires_at: i64,
    pub user_first_name: String,
    pub user_last_name: Option<String>,
    pub user_username: Option<String>,
    pub chat_title: Option<String>,
    pub chat_username: Option<String>,
}

impl CaptchaSession {
    pub fn from_pending(
        chat_id: ChatId,
        user: &User,
        pending: &PendingCaptcha,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        Ok(Self {
            chat_id: chat_id.0,
            user_id: i64::try_from(user.id.0).map_err(|_| "user id out of range")?,
            code: pending.code.clone(),
            captcha_message_id: pending.captcha_message_id.0,
            options: pending.options.clone(),
            attempts_left: i64::try_from(pending.attempts_left)
                .map_err(|_| "attempts_left out of range")?,
            attempts_total: i64::try_from(pending.attempts_total)
                .map_err(|_| "attempts_total out of range")?,
            expires_at: pending.expires_at,
            user_first_name: user.first_name.clone(),
            user_last_name: user.last_name.clone(),
            user_username: user.username.clone(),
            chat_title: pending.chat_title.clone(),
            chat_username: pending.chat_username.clone(),
        })
    }

    pub fn into_runtime(
        self,
        now: i64,
    ) -> Result<(ChatId, UserId, User, PendingCaptcha), Box<dyn Error + Send + Sync>> {
        let user_id = UserId(u64::try_from(self.user_id).map_err(|_| "user id out of range")?);
        let attempts_left =
            usize::try_from(self.attempts_left).map_err(|_| "attempts_left out of range")?;
        let attempts_total =
            usize::try_from(self.attempts_total).map_err(|_| "attempts_total out of range")?;
        if attempts_total == 0 || attempts_left > attempts_total {
            return Err("invalid captcha attempts in persisted session".into());
        }
        let user = User {
            id: user_id,
            is_bot: false,
            first_name: self.user_first_name,
            last_name: self.user_last_name,
            username: self.user_username,
            language_code: None,
            is_premium: false,
            added_to_attachment_menu: false,
        };
        let pending = PendingCaptcha {
            code: self.code,
            captcha_message_id: MessageId(self.captcha_message_id),
            options: self.options,
            attempts_left,
            attempts_total,
            remaining_secs: self.expires_at.saturating_sub(now) as u64,
            expires_at: self.expires_at,
            user_display: format_user_display(&user),
            user_name: format_user_name(&user),
            user_username: user.username.clone(),
            chat_title: self.chat_title,
            chat_username: self.chat_username,
        };
        Ok((ChatId(self.chat_id), user_id, user, pending))
    }
}

#[derive(Clone, Debug)]
pub struct CaptchaChatContext {
    pub title: Option<String>,
    pub username: Option<String>,
}

impl CaptchaChatContext {
    pub fn new(title: Option<String>, username: Option<String>) -> Self {
        Self { title, username }
    }
}

pub type CaptchaKey = (ChatId, UserId);
pub type SharedState = Arc<Mutex<HashMap<CaptchaKey, PendingCaptcha>>>;

pub enum CaptchaCheck {
    NoPending,
    Wrong,
    Verified(Box<PendingCaptcha>),
}

const CAPTCHA_SAFE_CHARS: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'M', 'N', 'P', 'Q', 'R', 'T', 'U', 'V', 'W',
    'X', 'Y', 'Z', '2', '3', '4', '6', '7', '8', '9',
];

pub fn generate_captcha(
    length: usize,
    width: u32,
    height: u32,
) -> Result<(String, Vec<u8>), Box<dyn Error + Send + Sync>> {
    let safe_width = width.min(399);
    let safe_height = height.min(299);
    let mut captcha = Captcha::new();
    captcha
        .set_chars(CAPTCHA_SAFE_CHARS)
        .add_chars(length as u32)
        .apply_filter(Noise::new(0.4))
        .view(safe_width, safe_height);

    let mut code = captcha.chars_as_string();
    if code.is_empty() {
        let mut fallback = Captcha::new();
        let supported = fallback.supported_chars();
        fallback
            .set_chars(&supported)
            .add_chars(length as u32)
            .apply_filter(Noise::new(0.4))
            .view(safe_width, safe_height);
        code = fallback.chars_as_string();
        let png = fallback.as_png().ok_or("failed to render captcha")?;
        return Ok((code, png));
    }

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
            .map(|_| {
                let idx = rng.gen_range(0..CAPTCHA_SAFE_CHARS.len());
                CAPTCHA_SAFE_CHARS[idx]
            })
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
    chat: &CaptchaChatContext,
) -> PendingCaptcha {
    PendingCaptcha {
        code,
        captcha_message_id,
        options,
        attempts_left: attempts_total,
        attempts_total,
        remaining_secs,
        expires_at: 0,
        user_display: format_user_display(user),
        user_name: format_user_name(user),
        user_username: user.username.as_deref().map(|raw| raw.trim().to_string()),
        chat_title: chat.title.clone(),
        chat_username: chat.username.clone(),
    }
}

pub fn check_captcha_answer_for_message_at(
    state: &mut HashMap<CaptchaKey, PendingCaptcha>,
    key: CaptchaKey,
    message_id: MessageId,
    now: i64,
    text: &str,
) -> CaptchaCheck {
    check_captcha_answer_inner(state, key, Some(message_id), Some(now), text)
}

fn check_captcha_answer_inner(
    state: &mut HashMap<CaptchaKey, PendingCaptcha>,
    key: CaptchaKey,
    expected_message_id: Option<MessageId>,
    now: Option<i64>,
    text: &str,
) -> CaptchaCheck {
    let Some(pending) = state.get(&key).cloned() else {
        return CaptchaCheck::NoPending;
    };
    if expected_message_id.is_some_and(|message_id| pending.captcha_message_id != message_id)
        || now.is_some_and(|now| pending.expires_at <= now)
    {
        return CaptchaCheck::NoPending;
    }
    if text.eq_ignore_ascii_case(&pending.code) {
        state.remove(&key);
        CaptchaCheck::Verified(Box::new(pending))
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
                expires_at: 120,
                user_display: "User @user".to_string(),
                user_name: "User".to_string(),
                user_username: Some("user".to_string()),
                chat_title: Some("Group".to_string()),
                chat_username: Some("groupname".to_string()),
            },
        );
        let wrong =
            check_captcha_answer_for_message_at(&mut state, key, MessageId(10), i64::MIN, "nope");
        assert!(matches!(wrong, CaptchaCheck::Wrong));
        assert!(state.contains_key(&key));

        let verified =
            check_captcha_answer_for_message_at(&mut state, key, MessageId(10), i64::MIN, "aBc");
        assert!(matches!(verified, CaptchaCheck::Verified(_)));
        assert!(!state.contains_key(&key));
    }

    #[test]
    fn message_and_expiry_are_checked_for_callback_answers() {
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
                remaining_secs: 10,
                expires_at: 100,
                user_display: "User".to_string(),
                user_name: "User".to_string(),
                user_username: None,
                chat_title: None,
                chat_username: None,
            },
        );

        assert!(matches!(
            check_captcha_answer_for_message_at(&mut state, key, MessageId(11), 50, "aBc"),
            CaptchaCheck::NoPending
        ));
        assert!(matches!(
            check_captcha_answer_for_message_at(&mut state, key, MessageId(10), 100, "aBc"),
            CaptchaCheck::NoPending
        ));
        assert!(state.contains_key(&key));
    }
}

use teloxide::types::Message;

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn message_content_label(msg: &Message) -> String {
    if let Some(text) = msg.text() {
        return text.to_string();
    }
    if msg.photo().is_some() {
        return "-image-".to_string();
    }
    if msg.document().is_some() {
        return "-document-".to_string();
    }
    if msg.sticker().is_some() {
        return "-sticker-".to_string();
    }
    if msg.video().is_some() {
        return "-video-".to_string();
    }
    if msg.audio().is_some() {
        return "-audio-".to_string();
    }
    if msg.voice().is_some() {
        return "-voice-".to_string();
    }
    if msg.animation().is_some() {
        return "-animation-".to_string();
    }
    if msg.video_note().is_some() {
        return "-video_note-".to_string();
    }
    if msg.contact().is_some() {
        return "-contact-".to_string();
    }
    if msg.location().is_some() {
        return "-location-".to_string();
    }
    if msg.venue().is_some() {
        return "-venue-".to_string();
    }
    if msg.poll().is_some() {
        return "-poll-".to_string();
    }
    if msg.dice().is_some() {
        return "-dice-".to_string();
    }
    if msg.game().is_some() {
        return "-game-".to_string();
    }
    if msg.invoice().is_some() {
        return "-invoice-".to_string();
    }
    if msg.successful_payment().is_some() {
        return "-payment-".to_string();
    }
    "-non-text-".to_string()
}

pub fn sanitize_log_text(input: &str) -> String {
    input
        .chars()
        .filter(|ch| !is_invisible_or_control(*ch))
        .collect::<String>()
}

pub fn format_user_display(user: &teloxide::types::User) -> String {
    let first_name = sanitize_log_text(user.first_name.trim());
    let last_name = sanitize_log_text(user.last_name.as_deref().unwrap_or(""));
    let username = sanitize_log_text(user.username.as_deref().unwrap_or("-"));
    let username_fmt = format!("@{}", username);
    if last_name.is_empty() {
        format!("{first_name} {username_fmt}")
    } else {
        format!("{first_name} {last_name} {username_fmt}")
    }
}

pub fn format_user_name(user: &teloxide::types::User) -> String {
    let first_name = sanitize_log_text(user.first_name.trim());
    let last_name = sanitize_log_text(user.last_name.as_deref().unwrap_or(""));
    if last_name.is_empty() {
        first_name
    } else {
        format!("{first_name} {last_name}")
    }
}

pub fn format_user_context(user: &teloxide::types::User) -> String {
    let first_name = sanitize_log_text(user.first_name.trim());
    let last_name = sanitize_log_text(user.last_name.as_deref().unwrap_or(""));
    let username = sanitize_log_text(user.username.as_deref().unwrap_or(""));
    let name = if last_name.is_empty() {
        first_name.to_string()
    } else {
        format!("{first_name} {last_name}")
    };
    if username.is_empty() {
        format!("{}:{}", user.id.0, name)
    } else {
        format!("{}:{} @{}", user.id.0, name, username)
    }
}

fn is_invisible_or_control(ch: char) -> bool {
    if ch.is_control() {
        return true;
    }
    matches!(
        ch,
        '\u{00AD}' // SHY
            | '\u{061C}' // ALM
            | '\u{180E}' // MVS
            | '\u{200B}' // ZWSP
            | '\u{200C}' // ZWNJ
            | '\u{200D}' // ZWJ
            | '\u{2060}' // WJ
            | '\u{2061}' // FAI
            | '\u{2062}' // INV
            | '\u{2063}' // ISS
            | '\u{2064}' // IIA
            | '\u{2066}' // LRI
            | '\u{2067}' // RLI
            | '\u{2068}' // FSI
            | '\u{2069}' // PDI
            | '\u{202A}' // LRE
            | '\u{202B}' // RLE
            | '\u{202C}' // PDF
            | '\u{202D}' // LRO
            | '\u{202E}' // RLO
            | '\u{FEFF}' // BOM
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_log_text_removes_control_chars() {
        let input = "hi\nthere\u{200B}";
        let out = sanitize_log_text(input);
        assert_eq!(out, "hithere");
    }
}

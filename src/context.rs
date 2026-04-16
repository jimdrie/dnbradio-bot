use anyhow::Result;
use chrono::NaiveDateTime;
use irc::client::Sender;
use irc::proto::Command;
use log::error;
use regex::Regex;
use serenity::all::{Cache, ChannelId, EditMessage, ExecuteWebhook, Http, MessageId, Webhook};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, RwLock,
};

#[derive(Clone)]
pub struct Context {
    pub(crate) discord_http: Arc<Http>,
    pub(crate) discord_cache: Arc<Cache>,
    pub(crate) discord_channel: ChannelId,
    pub(crate) discord_webhook_url: String,
    pub(crate) irc_sender: Arc<RwLock<Sender>>,
    pub(crate) irc_channel: String,
    pub(crate) command_prefix: String,
    pub(crate) last_track: Arc<RwLock<Option<(NaiveDateTime, String)>>>,
    pub(crate) shazam_discord_channel: ChannelId,
    pub(crate) shazam_irc_channel: String,
    pub(crate) shazam_active: Arc<AtomicBool>,
    pub(crate) np_state: Arc<Mutex<NpState>>,
    pub(crate) np_someone_talked: Arc<AtomicBool>,
}

pub(crate) struct NpState {
    pub(crate) message_id: Option<MessageId>,
    pub(crate) lines: VecDeque<String>,
}

impl Context {
    pub(crate) async fn send_to_discord(&self, message: &str) {
        self.send_to_discord_channel(&message.replace('|', "\\|"), &self.discord_channel)
            .await;
    }

    pub(crate) async fn send_to_discord_channel(&self, message: &str, channel: &ChannelId) {
        if let Err(error) = channel.say(&self.discord_http, message).await {
            error!("Error sending message to Discord: {:?}", error);
        }
    }

    pub(crate) fn discord_markdown_to_irc(&self, message: &str) -> String {
        // Custom emoji: <:name:id> or <a:name:id> → :name:
        let re = Regex::new(r"<a?:(\w+):\d+>").unwrap();
        let message = re.replace_all(message, ":$1:").to_string();
        // Bold+italic: ***text*** → \x02\x1Dtext\x1D\x02
        let re = Regex::new(r"\*\*\*(.*?)\*\*\*").unwrap();
        let message = re.replace_all(&message, "\x02\x1D$1\x1D\x02").to_string();
        // Bold: **text** → \x02text\x02
        let re = Regex::new(r"\*\*(.*?)\*\*").unwrap();
        let message = re.replace_all(&message, "\x02$1\x02").to_string();
        // Strikethrough: ~~text~~ → \x1Etext\x1E
        let re = Regex::new(r"~~(.*?)~~").unwrap();
        let message = re.replace_all(&message, "\x1E$1\x1E").to_string();
        // Underline: __text__ → \x1Ftext\x1F (must come before single _)
        let re = Regex::new(r"__(.*?)__").unwrap();
        let message = re.replace_all(&message, "\x1F$1\x1F").to_string();
        // Italic: *text* or _text_ → \x1Dtext\x1D
        let re = Regex::new(r"\*(.*?)\*").unwrap();
        let message = re.replace_all(&message, "\x1D$1\x1D").to_string();
        let re = Regex::new(r"_(.*?)_").unwrap();
        re.replace_all(&message, "\x1D$1\x1D").to_string()
    }

    pub(crate) fn escape_discord_markdown(text: &str) -> String {
        // Escape characters that trigger Discord markdown formatting
        text.replace('\\', "\\\\")
            .replace('_', "\\_")
            .replace('*', "\\*")
            .replace('~', "\\~")
            .replace('|', "\\|")
    }

    pub(crate) fn translate_control_character(
        &self,
        character: u32,
        replacement: &str,
        message: &str,
    ) -> String {
        let regex = Regex::new(&format!(r"\x{:02X}(.*?)\x{:02X}", character, character)).unwrap();
        let message = regex.replace_all(message, |caps: &regex::Captures| {
            format!(
                "{}{}{}",
                replacement,
                Self::escape_discord_markdown(&caps[1]),
                replacement
            )
        });
        let regex = Regex::new(&format!(r"\x{:02X}(.*)", character)).unwrap();
        regex
            .replace_all(&message, |caps: &regex::Captures| {
                format!(
                    "{}{}{}",
                    replacement,
                    Self::escape_discord_markdown(&caps[1]),
                    replacement
                )
            })
            .to_string()
    }

    pub(crate) async fn send_to_discord_webhook_relay(
        &self,
        nickname: &str,
        message: &str,
        avatar_url: Option<String>,
    ) {
        self.np_someone_talked.store(true, Ordering::Release);
        self.send_to_discord_webhook(nickname, message, avatar_url)
            .await;
    }

    pub(crate) async fn send_to_discord_webhook(
        &self,
        nickname: &str,
        message: &str,
        avatar_url: Option<String>,
    ) {
        let webhook = match Webhook::from_url(&self.discord_http, &self.discord_webhook_url).await {
            Ok(webhook) => webhook,
            Err(error) => {
                error!("Failed to get webhook from URL: {:?}", error);
                return;
            }
        };

        // Translate IRC formatting to Discord formatting and strip colour coding
        let action_regex = Regex::new(r"^\x01ACTION (.*)\x01$").unwrap();
        let message = action_regex
            .replace_all(message, |caps: &regex::Captures| {
                format!("_{}_", Self::escape_discord_markdown(&caps[1]))
            })
            .to_string();
        let message = self.translate_control_character(0x02, "**", &message);
        let colour_regex = Regex::new(r"\x03(?:\d{1,2}(?:,\d{1,2})?)?").unwrap();
        let message = colour_regex.replace_all(&message, "").to_string();
        let message = self.translate_control_character(0x1D, "*", &message);
        let message = self.translate_control_character(0x1E, "~~", &message);
        let message = self.translate_control_character(0x1F, "__", &message);
        let message = message.replace('|', "\\|");

        let mut builder = ExecuteWebhook::new().username(nickname).content(message);
        if let Some(avatar_url) = avatar_url {
            builder = builder.avatar_url(avatar_url);
        }
        if let Err(error) = webhook.execute(&self.discord_http, false, builder).await {
            error!("Failed to execute webhook: {:?}", error);
        }
    }

    pub(crate) async fn send_to_irc(&self, message: &str, nickname: Option<&str>) {
        self.send_to_irc_channel(message, &self.irc_channel, nickname)
            .await;
    }

    pub(crate) async fn send_to_irc_channel(
        &self,
        message: &str,
        channel: &str,
        nick: Option<&str>,
    ) {
        let irc_sender = self.irc_sender.read().unwrap();
        let prefix = nick.map_or(String::new(), |n| format!("<{}> ", n));
        let line_count = message.lines().count();
        for (index, line) in message.lines().enumerate() {
            let suffix = if nick.is_some() && index >= 4 && index < line_count - 1 {
                "... (truncated)"
            } else {
                ""
            };
            if let Err(error) =
                irc_sender.send_privmsg(channel, format!("{}{}{}", prefix, line, suffix))
            {
                error!("Error sending message to IRC: {:?}", error);
            }
            if nick.is_some() && index >= 4 {
                break;
            }
        }
    }

    pub(crate) async fn set_irc_topic(&self, topic: String) -> Result<()> {
        let irc_sender = self.irc_sender.read().unwrap();
        irc_sender.send(Command::TOPIC(self.irc_channel.to_string(), Some(topic)))?;
        Ok(())
    }

    async fn send_np_to_discord(&self, message: &str, replace_last: bool) {
        const MAX_NP_LINES: usize = 5;
        let message = message.replace('|', "\\|");

        let someone_talked = self.np_someone_talked.swap(false, Ordering::AcqRel);

        // Determine action while holding the lock, then release before any await.
        enum NpAction {
            SendNew(String),
            EditExisting(MessageId, String),
        }

        let action = {
            let mut state = self.np_state.lock().unwrap();
            if someone_talked {
                state.message_id = None;
                state.lines.clear();
            }
            if replace_last && !state.lines.is_empty() {
                *state.lines.back_mut().unwrap() = message.clone();
            } else {
                state.lines.push_back(message.clone());
                if state.lines.len() > MAX_NP_LINES {
                    state.lines.pop_front();
                }
            }
            let content = state.lines.iter().cloned().collect::<Vec<_>>().join("\n");
            match state.message_id {
                None => NpAction::SendNew(content),
                Some(id) => NpAction::EditExisting(id, content),
            }
        }; // MutexGuard dropped here

        match action {
            NpAction::SendNew(content) => {
                match self.discord_channel.say(&self.discord_http, &content).await {
                    Ok(sent_msg) => {
                        self.np_state.lock().unwrap().message_id = Some(sent_msg.id);
                    }
                    Err(e) => error!("Error sending NP message to Discord: {:?}", e),
                }
            }
            NpAction::EditExisting(id, content) => {
                let builder = EditMessage::new().content(&content);
                if let Err(e) = self
                    .discord_channel
                    .edit_message(&self.discord_http, id, builder)
                    .await
                {
                    error!("Error editing NP message in Discord: {:?}", e);
                }
            }
        }
    }

    pub(crate) async fn send_np_action(&self, action: &str, replace_last: bool) {
        self.send_np_to_discord(
            &format!("_{}_", Self::escape_discord_markdown(action)),
            replace_last,
        )
        .await;
        self.send_to_irc(&format!("\x01ACTION {}\x01", action), None)
            .await;
    }

    pub(crate) async fn send_action(&self, action: &str) {
        self.send_to_discord(&format!("_{}_", Self::escape_discord_markdown(action)))
            .await;
        self.send_to_irc(&format!("\x01ACTION {}\x01", action), None)
            .await;
    }

    pub(crate) async fn send_message(&self, message: &str) {
        let discord_future = self.send_to_discord(message);
        let irc_future = self.send_to_irc(message, None);
        _ = tokio::join!(discord_future, irc_future);
    }

    pub(crate) async fn send_shazam(&self, message: &str) {
        let discord_future = self.shazam_discord_channel.say(&self.discord_http, message);
        let irc_future = self.send_to_irc_channel(message, &self.shazam_irc_channel, None);
        _ = tokio::join!(irc_future, discord_future);
    }
}

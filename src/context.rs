use anyhow::Result;
use chrono::NaiveDateTime;
use irc::client::Sender;
use irc::proto::Command;
use log::error;
use regex::Regex;
use serenity::all::{Cache, ChannelId, ExecuteWebhook, Http, Webhook};
use std::sync::{Arc, RwLock};

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
}

impl Context {
    pub async fn send_to_discord(&self, message: &str) {
        self.send_to_discord_channel(message, &self.discord_channel)
            .await;
    }

    pub async fn send_to_discord_channel(&self, message: &str, channel: &ChannelId) {
        if let Err(error) = channel.say(&self.discord_http, message).await {
            error!("Error sending message to Discord: {:?}", error);
        }
    }

    pub fn translate_control_character(
        &self,
        character: u32,
        replacement: &str,
        message: &str,
    ) -> String {
        let regex = Regex::new(&format!(r"\x{:02X}(.*?)\x{:02X}", character, character)).unwrap();
        let message = regex.replace_all(message, format!("{}${{1}}{}", replacement, replacement));
        let regex = Regex::new(&format!(r"\x{:02X}(.*)", character)).unwrap();
        regex
            .replace_all(&message, format!("{}${{1}}{}", replacement, replacement))
            .to_string()
    }

    pub async fn send_to_discord_webhook(
        &self,
        nickname: &str,
        message: &str,
        avatar_url: Option<String>,
    ) {
        let webhook = Webhook::from_url(&self.discord_http, &self.discord_webhook_url)
            .await
            .expect("Could not get webhook.");

        // Translate IRC formatting to Discord formatting and strip colour coding
        let action_regex = Regex::new(r"^\x01ACTION (.*)\x01$").unwrap();
        let message = action_regex.replace_all(message, "_${1}_").to_string();
        let message = self.translate_control_character(0x02, "**", &message);
        let colour_regex = Regex::new(r"\x03(?:\d{1,2}(?:,\d{1,2})?)?").unwrap();
        let message = colour_regex.replace_all(&message, "").to_string();
        let message = self.translate_control_character(0x1D, "*", &message);
        let message = self.translate_control_character(0x1E, "~~", &message);
        let message = self.translate_control_character(0x1F, "__", &message);

        let mut builder = ExecuteWebhook::new().username(nickname).content(message);
        if let Some(avatar_url) = avatar_url {
            builder = builder.avatar_url(avatar_url);
        }
        webhook
            .execute(&self.discord_http, false, builder)
            .await
            .expect("Could not execute webhook.");
    }

    pub async fn send_to_irc(&self, message: &str) {
        self.send_to_irc_channel(message, &self.irc_channel).await;
    }

    pub async fn send_to_irc_channel(&self, message: &str, channel: &str) {
        let irc_sender = self.irc_sender.read().unwrap();

        for line in message.lines() {
            if let Err(error) = irc_sender.send_privmsg(channel, line) {
                error!("Error sending message to IRC: {:?}", error);
            }
        }
    }

    pub async fn set_irc_topic(&self, topic: String) -> Result<()> {
        let irc_sender = self.irc_sender.read().unwrap();
        irc_sender.send(Command::TOPIC(self.irc_channel.to_string(), Some(topic)))?;
        Ok(())
    }

    pub async fn send_action(&self, action: &str) {
        self.send_to_discord(&format!("_{}_", action)).await;
        self.send_to_irc(&format!("\x01ACTION {}\x01", action))
            .await;
    }

    pub async fn send_message(&self, message: &str) {
        let discord_future = self.send_to_discord(message);
        let irc_future = self.send_to_irc(message);
        _ = tokio::join!(discord_future, irc_future);
    }

    pub async fn send_shazam(&self, message: &str) {
        let discord_future = self.shazam_discord_channel.say(&self.discord_http, message);
        let irc_future = self.send_to_irc_channel(message, &self.shazam_irc_channel);
        _ = tokio::join!(irc_future, discord_future);
    }
}

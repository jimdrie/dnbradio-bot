use anyhow::Result;
use chrono::NaiveDateTime;
use irc::client::Sender;
use irc::proto::Command;
use log::error;
use serenity::all::{Cache, ChannelId, ExecuteWebhook, Http, Webhook};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Context {
    pub(crate) discord_http: Arc<Http>,
    pub(crate) discord_cache: Arc<Cache>,
    pub(crate) discord_channel: ChannelId,
    pub(crate) discord_webhook_url: String,
    pub(crate) irc_sender: Sender,
    pub(crate) irc_channel: String,
    pub(crate) command_prefix: String,
    pub(crate) last_track: Arc<Mutex<Option<(NaiveDateTime, String)>>>,
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

    pub async fn send_to_discord_webhook(
        &self,
        nickname: &str,
        message: &str,
        avatar_url: Option<String>,
    ) {
        let webhook = Webhook::from_url(&self.discord_http, &self.discord_webhook_url)
            .await
            .expect("Could not get webhook.");

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
        for line in message.lines() {
            if let Err(error) = self.irc_sender.send_privmsg(channel, line) {
                error!("Error sending message to IRC: {:?}", error);
            }
        }
    }

    pub async fn set_irc_topic(&self, topic: String) -> Result<()> {
        self.irc_sender
            .send(Command::TOPIC(self.irc_channel.to_string(), Some(topic)))?;
        Ok(())
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

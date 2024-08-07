use crate::api;
use anyhow::Result;
use chrono::NaiveDateTime;
use irc::client::Sender;
use irc::proto::Command;
use log::error;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serenity::all::{Cache, ChannelId, ExecuteWebhook, Http, Webhook};
use std::env;
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
        self.send_to_discord_channel(&message.replace('|', "\\|"), &self.discord_channel)
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
        let message = self.translate_control_character(0x1d, "*", &message);
        let message = self.translate_control_character(0x1e, "~~", &message);
        let message = self.translate_control_character(0x1f, "__", &message);
        let message = message.replace('|', "\\|");

        let mut builder = ExecuteWebhook::new().username(nickname).content(message);
        if let Some(avatar_url) = avatar_url {
            builder = builder.avatar_url(avatar_url);
        }
        webhook
            .execute(&self.discord_http, false, builder)
            .await
            .expect("Could not execute webhook.");
    }

    pub async fn send_to_irc(&self, message: &str, nickname: Option<&str>) {
        self.send_to_irc_channel(message, &self.irc_channel, nickname)
            .await;
    }

    pub async fn send_to_irc_channel(&self, message: &str, channel: &str, nick: Option<&str>) {
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
                irc_sender.send_privmsg(channel, &format!("{}{}{}", prefix, line, suffix))
            {
                error!("Error sending message to IRC: {:?}", error);
            }
            if nick.is_some() && index >= 4 {
                break;
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
        self.send_to_irc(&format!("\x01ACTION {}\x01", action), None)
            .await;
    }

    pub async fn send_message(&self, message: &str) {
        let discord_future = self.send_to_discord(message);
        let irc_future = self.send_to_irc(message, None);
        _ = tokio::join!(discord_future, irc_future);
    }

    pub async fn send_shazam(&self, message: &str) {
        let discord_future = self.shazam_discord_channel.say(&self.discord_http, message);
        let irc_future = self.send_to_irc_channel(message, &self.shazam_irc_channel, None);
        _ = tokio::join!(irc_future, discord_future);
    }

    pub async fn post_shazam_to_webhook(&self, track: &ShazamTrack) {
        let now_playing_response = match api::get_now_playing().await {
            Ok(response) => response,
            Err(e) => {
                println!("Failed to get now playing data: {:?}", e);
                return;
            }
        };

        let payload = json!({
            "albumadamid": track.albumadamid,
            "artists": track.artists,
            "genres": track.genres,
            "images": track.images,
            "isrc": track.isrc,
            "key": track.key,
            "sections": track.sections,
            "title": track.title,
            "subtitle": track.subtitle,
            "url": track.url,
            "listener_count": now_playing_response.listeners.current,
            "date_played": now_playing_response.now_playing.played_at,
        });

        match api::post_dnbradio_webhook_api_response::<_, serde_json::Value>("/", payload).await {
            Ok(_) => println!("Message sent successfully!"),
            Err(e) => {
                println!("Error sending message: {:?}", e);
                println!("Ensure the server is running and accessible.");
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShazamResponse {
    pub timestamp: u64,
    pub tagid: String,
    pub track: Option<ShazamTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShazamTrack {
    pub albumadamid: Option<String>,
    pub artists: Option<Vec<ShazamSmall>>,
    pub genres: Option<ShazamGenres>,
    pub images: Option<ShazamImages>,
    pub isrc: Option<String>,
    pub key: String,
    pub sections: Vec<ShazamSection>,
    pub title: String,
    pub subtitle: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShazamSmall {
    pub adamid: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShazamGenres {
    pub primary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShazamImages {
    pub background: String,
    pub coverart: String,
    pub coverarthq: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShazamSection {
    MetaSection {
        metadata: Vec<ShazamMetadataSection>,
    },
    ArtistSection {
        id: String,
        name: String,
        tabname: String,
        #[serde(rename = "type")]
        type_: String,
    },
    Other {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShazamMetadataSection {
    pub text: String,
    pub title: String,
}

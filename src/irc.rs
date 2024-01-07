use crate::commands;
use crate::context::Context;
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use irc::client::prelude::*;
use log::error;
use serenity::all::{Cache, ChannelId, Http};
use std::env;
use std::sync::Arc;

#[async_trait]
pub(crate) trait IrcClientExt {
    async fn start(&mut self, context: Context) -> Result<()>;
}

#[async_trait]
impl IrcClientExt for Client {
    async fn start(&mut self, mut context: Context) -> Result<()> {
        let mut stream = self.stream()?;
        while let Some(message) = stream.next().await.transpose()? {
            if let Command::PRIVMSG(ref target, ref msg) = message.command {
                if target != &context.irc_channel {
                    continue;
                }
                let nick = message.source_nickname().unwrap_or("Unknown");
                let avatar_url = get_avatar_url(
                    context.discord_channel,
                    &context.discord_http,
                    &context.discord_cache,
                    nick,
                )
                .await;
                context.send_to_discord_webhook(nick, msg, avatar_url).await;
                if msg.starts_with(&context.command_prefix) {
                    let command = &msg[1..];
                    match commands::handle_command(&mut context, command, false).await {
                        Ok(_) => {}
                        Err(error) => {
                            error!("Error handling command {}: {:?}", command, error);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub async fn get_irc_client() -> Client {
    let config = Config {
        nickname: Some(
            env::var("IRC_NICK")
                .expect("IRC_NICK must be set")
                .to_owned(),
        ),
        server: Some(
            env::var("IRC_SERVER")
                .expect("IRC_SERVER must be set")
                .to_owned(),
        ),
        port: Some(
            env::var("IRC_PORT")
                .expect("IRC_PORT must be set")
                .parse::<u16>()
                .expect("IRC_PORT must be a number"),
        ),
        channels: env::var("IRC_CHANNELS")
            .expect("IRC_CHANNELS must be set")
            .split(',')
            .map(|s| s.to_owned())
            .collect(),
        use_tls: Some(
            env::var("IRC_USE_TLS")
                .unwrap_or("false".to_owned())
                .parse::<bool>()
                .expect("IRC_USE_TLS must be true or false"),
        ),
        ..Config::default()
    };

    let client = Client::from_config(config)
        .await
        .expect("Error creating client");
    client.identify().expect("Error identifying to server");

    client
}

pub async fn get_avatar_url(
    channel_id: ChannelId,
    http: &Arc<Http>,
    cache: &Arc<Cache>,
    nick: &str,
) -> Option<String> {
    if let Ok(channel) = &channel_id.to_channel(&http).await {
        if let Some(guild) = channel.clone().guild() {
            if let Ok(members) = guild.members(cache) {
                for member in members {
                    if member.display_name() == nick {
                        return member.user.avatar_url();
                    }
                }
            } else {
                error!("Could not get members from guild");
            }
        } else {
            error!("Could not get guild from channel");
        }
    }
    None
}

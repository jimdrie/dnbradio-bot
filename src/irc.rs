use crate::commands;
use crate::context::Context;
use anyhow::Result;
use futures::StreamExt;
use irc::client::prelude::*;
use log::{error, info, warn};
use serenity::all::{Cache, ChannelId, Http};
use std::env;
use std::sync::Arc;
use std::time::Duration;

pub(crate) trait IrcClientExt {
    async fn start(self, context: Context);
    async fn message_loop(&mut self, context: &mut Context) -> Result<()>;
}

impl IrcClientExt for Client {
    async fn start(mut self, mut context: Context) {
        loop {
            if let Err(error) = self.message_loop(&mut context).await {
                error!("Error in message loop: {:?}", error);
            }
            error!("IRC client disconnected, reconnecting in 10 seconds");
            tokio::time::sleep(Duration::from_secs(10)).await;
            self = get_irc_client().await;
            let mut irc_sender = context.irc_sender.write().unwrap();
            *irc_sender = self.sender();
        }
    }

    async fn message_loop(&mut self, context: &mut Context) -> Result<()> {
        let mut stream = self.stream()?;
        while let Some(message) = stream.next().await.transpose()? {
            info!("{:?}", message);
            let nickname = message.source_nickname().unwrap_or("Unknown");
            match message.command {
                Command::Response(Response::RPL_ENDOFMOTD, _)
                | Command::Response(Response::ERR_NOMOTD, _) => {
                    let perform = env::var("IRC_PERFORM").unwrap_or("".to_owned());
                    if !perform.is_empty() {
                        let mut command_parts = perform.split(' ');
                        let command_name = command_parts.next().unwrap_or("");
                        let command_args = command_parts.map(ToOwned::to_owned).collect();
                        info!("Performing command: {} {:?}", command_name, command_args);
                        if let Err(err) =
                            self.send(Command::Raw(command_name.to_owned(), command_args))
                        {
                            error!("Error sending perform: {}", err);
                        }
                    }
                }
                Command::PRIVMSG(ref target, ref msg) => {
                    if target != &context.irc_channel {
                        continue;
                    }
                    let avatar_url = get_avatar_url(
                        context.discord_channel,
                        &context.discord_http,
                        &context.discord_cache,
                        nickname,
                    )
                    .await;
                    context
                        .send_to_discord_webhook(nickname, msg, avatar_url)
                        .await;
                    if msg.starts_with(&context.command_prefix) {
                        let command = &msg[1..];
                        if let Err(error) =
                            commands::handle_command(context, nickname, command, false).await
                        {
                            warn!("Error handling command {}: {:?}", command, error);
                        }
                    }
                }
                Command::JOIN(ref channel, _, _) => {
                    if channel == &context.irc_channel {
                        // Voice user.
                        if let Err(error) = self.send(Command::ChannelMODE(
                            context.irc_channel.to_string(),
                            vec![Mode::Plus(ChannelMode::Voice, Some(nickname.to_string()))],
                        )) {
                            error!("Error setting voice mode: {:?}", error);
                        }
                    }
                }
                _ => {}
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

    println!("{:?}", config);

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
    nickname: &str,
) -> Option<String> {
    if let Ok(channel) = &channel_id.to_channel(&http).await {
        if let Some(guild) = channel.clone().guild() {
            if let Ok(members) = guild.members(cache) {
                for member in members {
                    if member.display_name() == nickname {
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

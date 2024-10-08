use crate::{commands, context};
use log::error;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::prelude::*;
use std::env;

pub(crate) struct CommandContext;

impl TypeMapKey for CommandContext {
    type Value = context::Context;
}

pub(crate) async fn get_serenity_client() -> Client {
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set");
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_PRESENCES;

    Client::builder(token, intents)
        .event_handler(Handler)
        .await
        .expect("Error creating client")
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let context = ctx
            .data
            .read()
            .await
            .get::<CommandContext>()
            .unwrap()
            .clone();

        if msg.channel_id != context.discord_channel || msg.webhook_id.is_some() || msg.author.bot {
            return;
        }

        let mut message = msg.content.clone();

        for attachment in &msg.attachments {
            if !message.is_empty() {
                message.push_str(&format!(" - {})", attachment.proxy_url));
            } else {
                message.push_str(&attachment.proxy_url.to_string());
            }
        }

        let nickname = msg
            .author_nick(&context.discord_http)
            .await
            .unwrap_or(msg.author.global_name.unwrap_or(msg.author.name));

        let channel = msg
            .channel_id
            .to_channel(&context.discord_http)
            .await
            .unwrap()
            .to_string();

        context.send_to_irc(&message, Some(&nickname)).await;

        if msg.content.starts_with(&context.command_prefix) {
            let command = &msg.content[1..];
            if let Err(error) =
                commands::handle_command(&context, &channel, &nickname, command, false).await
            {
                error!("Error handling command {}: {:?}", command, error);
            }
        }
    }
}

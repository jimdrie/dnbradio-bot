use std::sync::atomic::Ordering;

use crate::context::Context;
use crate::{api, shazam};
use anyhow::Result;
use log::{error, warn};

pub(crate) async fn handle_command(
    context: &Context,
    channel: &str,
    nickname: &str,
    command: &str,
    _is_admin: bool,
) -> Result<()> {
    let mut command_parts = command.split(' ');
    let command_name = command_parts.next().unwrap_or("");
    let command_args = command_parts.collect::<Vec<&str>>();

    match command_name {
        "np" | "ch00n" => now_playing(context).await?,
        "count" | "cunts" => listener_count(context).await?,
        "shazam" => shazam(context).await,
        "id" => id(context, nickname).await?,
        "submit" => context.send_message("If you're interested in becoming a DJ on the station, please email submissions@dnbradio.com!").await,
        "ratings" => ratings(context).await?,
        "rate" => rate(channel, nickname, context, command_args).await?,
        "comments" => comments(context).await?,
        "comment" => comment(channel, nickname, context, command_args).await?,
        "boh" | "bohboh" | "bohbohboh" => boh(context, command_name.matches("boh").count(), false).await?,
        "hob" | "hobhob" | "hobhobhob" => boh(context, command_name.matches("hob").count(), true).await?,
        "sched" | "schedule" => schedule(context).await?,
        "queue" => queue(context).await?,
        "incoming" => context.send_action(&format!("grabs {} and runs yelling INCOMING!", nickname)).await,
        _ => {
            warn!(
                "Unknown command: {}{}",
                context.command_prefix, command_name
            );
        }
    }
    Ok(())
}

async fn queue(context: &Context) -> Result<()> {
    match api::get_queue().await {
        Ok(queue) => {
            let mut queue_string = String::new();
            for (i, (artist, title)) in queue.iter().enumerate() {
                queue_string.push_str(&format!("{}) {} - {}\n", i + 1, artist, title));
            }
            context.send_message(queue_string.trim_end()).await;
        }
        Err(error) => {
            error!("Could not get queue: {:?}", error);
        }
    }
    Ok(())
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

async fn now_playing(context: &Context) -> Result<()> {
    let now_playing_response = api::get_now_playing().await?;

    let artist = &now_playing_response.now_playing.song.artist;
    let is_live = now_playing_response.live.is_live;
    let streamer_prefix = if is_live
        && !now_playing_response.live.streamer_name.is_empty()
        && !artist.to_lowercase().contains(&now_playing_response.live.streamer_name.to_lowercase())
    {
        format!("{} - ", now_playing_response.live.streamer_name)
    } else {
        String::new()
    };
    let elapsed = now_playing_response.now_playing.elapsed;
    let duration = now_playing_response.now_playing.duration;
    let time_str = if is_live || duration == 0 {
        format!(" [{}]", format_duration(elapsed))
    } else {
        format!(" [{}/{}]", format_duration(elapsed), format_duration(duration))
    };
    context
        .send_message(&format!(
            "Now playing: {}{} - {}{} (Tuned: {}){}",
            streamer_prefix,
            Context::escape_discord_markdown(artist),
            Context::escape_discord_markdown(&now_playing_response.now_playing.song.title),
            if is_live { " **LIVE**" } else { "" },
            now_playing_response.listeners.current,
            time_str,
        ))
        .await;
    Ok(())
}

async fn listener_count(context: &Context) -> Result<()> {
    let now_playing_response = api::get_now_playing().await?;
    context
        .send_message(&format!(
            "There are {} listeners tuned in!",
            now_playing_response.listeners.current
        ))
        .await;
    Ok(())
}

async fn shazam(context: &Context) {
    if !context.shazam_active.load(Ordering::Relaxed) {
        let _ = now_playing(context).await;
        return;
    }

    let last_track = shazam::get_last_sent_track(context);
    match last_track {
        Some((date, track)) => {
            let now = chrono::Utc::now().naive_utc();
            let time_since_last_sent = now - date;
            context
                .send_message(&format!(
                    "{}, {} seconds ago",
                    track,
                    time_since_last_sent.num_seconds()
                ))
                .await;
        }
        None => {
            context.send_message("Nothing yet...").await;
        }
    }
}

async fn id(context: &Context, nickname: &str) -> Result<()> {
    let now_playing_response = api::get_now_playing().await?;
    if now_playing_response.live.is_live {
        let djname = if now_playing_response.live.streamer_name.is_empty() {
            now_playing_response.now_playing.song.artist.clone()
        } else {
            now_playing_response.live.streamer_name.clone()
        };
        context
            .send_message(&format!(
                "Yo {}! {} wants to know the current track ID!",
                djname, nickname
            ))
            .await;
    } else {
        now_playing(context).await?;
    }
    Ok(())
}

async fn boh(context: &Context, factor: usize, reverse: bool) -> Result<()> {
    let now_playing_response = api::get_now_playing().await?;
    let ratings_response = api::get_ratings(now_playing_response.now_playing.song.id).await?;
    let rating = ratings_response.average_rating as usize;

    let rating_percentage = 10 * rating * factor;
    let filled_blocks = "█".repeat(rating * factor * 2);
    let empty_blocks = " ".repeat((10 * factor - rating * factor) * 2);

    let mut bohmeter = format!(
        "BOHMETER [{}{}] ({}%)",
        filled_blocks, empty_blocks, rating_percentage
    );
    if reverse {
        bohmeter = bohmeter.chars().rev().collect();
    }
    context.send_message(&bohmeter).await;
    Ok(())
}

async fn schedule(context: &Context) -> Result<()> {
    let schedule = api::get_schedule().await?;
    let mut schedule_string = String::new();
    for (start, _, title) in schedule {
        let time_difference = (chrono::Utc::now() - start).num_minutes();
        let mut time_difference_string = if time_difference > -60 {
            format!("{}m", time_difference.abs())
        } else {
            format!("{:.1}h", time_difference.abs() as f32 / 60.0)
        };
        if time_difference > 0 {
            time_difference_string = format!("Started {} ago", time_difference_string);
        } else {
            time_difference_string = format!("Starts in {}", time_difference_string);
        }
        schedule_string.push_str(&format!("{}: {}\n", time_difference_string, title));
    }
    schedule_string
        .push_str("For additional info check https://dnbradio.com/player/stations/1/schedule/");
    context.send_message(&schedule_string).await;
    Ok(())
}

async fn ratings(context: &Context) -> Result<()> {
    let now_playing_response = api::get_now_playing().await?;
    let song = now_playing_response.now_playing.song;
    let rating_response = api::get_ratings(song.id).await?;
    if rating_response.ratings.is_empty() {
        context
            .send_message(&format!(
                "No ratings for {} - {} yet",
                song.artist, song.title
            ))
            .await;
        return Ok(());
    }
    let message = format!("Ratings for {} - {}: ", song.artist, song.title);

    let message = rating_response.ratings.iter().fold(message, |acc, rating| {
        format!("{}{}: {} - ", acc, rating.nick, rating.rating)
    });
    let message = format!("{}Average: {}", message, rating_response.average_rating);
    context.send_message(&message).await;
    Ok(())
}

async fn rate(channel: &str, nickname: &str, context: &Context, args: Vec<&str>) -> Result<()> {
    if args.is_empty() {
        context
            .send_message(&format!(
                "Usage: {}rate <rating> [<comment>]",
                context.command_prefix
            ))
            .await;
        return Ok(());
    }
    let Ok(rating) = args[0].parse::<f32>() else {
        context.send_message("Invalid rating").await;
        return Ok(());
    };
    if !(0.0..=10.0).contains(&rating) {
        context
            .send_message("Rating must be between 0 and 10")
            .await;
        return Ok(());
    }
    let now_playing_response = api::get_now_playing().await?;
    let is_live = now_playing_response.live.is_live;

    let rate_response = api::set_rating(
        now_playing_response.now_playing.song.id,
        if is_live { 'L' } else { 'S' },
        0,
        rating,
        channel.to_owned(),
        nickname.to_owned(),
        if args.len() > 1 {
            Some(args[1..].join(" "))
        } else {
            None
        },
    )
    .await?;
    context
        .send_message(&format!(
            "Rated {} - {} with {} (new average: {})",
            now_playing_response.now_playing.song.artist,
            now_playing_response.now_playing.song.title,
            rating,
            rate_response.average_rating
        ))
        .await;
    if let Some(comment) = rate_response.comment {
        context.send_message(&format!("Comment: {}", comment)).await;
    }
    Ok(())
}

async fn comments(context: &Context) -> Result<()> {
    let now_playing_response = api::get_now_playing().await?;
    let song = now_playing_response.now_playing.song;
    let comments_response = api::get_comments(song.id).await?;
    if comments_response.comments.is_empty() {
        context
            .send_message(&format!(
                "No comments for {} - {} yet",
                song.artist, song.title
            ))
            .await;
        return Ok(());
    }
    let message = format!("Comments for {} - {}: ", song.artist, song.title);
    let message = comments_response
        .comments
        .iter()
        .fold(message, |acc, comment| {
            format!("{}{}: {} - ", acc, comment.nick, comment.comment)
        });
    context.send_message(&message[0..message.len() - 3]).await;
    Ok(())
}

async fn comment(channel: &str, nickname: &str, context: &Context, args: Vec<&str>) -> Result<()> {
    if args.is_empty() {
        context
            .send_message(&format!(
                "Usage: {}comment <comment>",
                context.command_prefix
            ))
            .await;
        return Ok(());
    }
    let now_playing_response = api::get_now_playing().await?;
    let is_live = now_playing_response.live.is_live;

    let comment = args.join("");

    api::add_comment(
        now_playing_response.now_playing.song.id,
        if is_live { 'L' } else { 'S' },
        0,
        channel.to_owned(),
        nickname.to_owned(),
        comment.clone(),
    )
    .await?;
    context
        .send_message(&format!(
            "Commented added for {} - {}: {}",
            now_playing_response.now_playing.song.artist,
            now_playing_response.now_playing.song.title,
            comment,
        ))
        .await;
    Ok(())
}

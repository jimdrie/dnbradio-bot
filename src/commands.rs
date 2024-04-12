use crate::context::Context;
use crate::{api, shazam};
use anyhow::Result;
use log::{error, warn};

pub(crate) async fn handle_command(
    context: &Context,
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
        "submit" => context.send_message("If you're interested in becoming a DJ on the station, please email submissions@dnbradio.com!").await,
        "rate" => rate(nickname, context, command_args).await?,
        "ratings" => ratings(context).await?,
        "comment" | "comments" => context.send_message("Coming soon!").await,
        "boh" | "bohboh" | "bohbohboh" => boh(context, command_name.matches("boh").count()).await,
        "sched" | "schedule" => schedule(context).await?,
        "queue" => queue(context).await?,
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
            for (i, (artist, title)) in queue.iter().enumerate() {
                context
                    .send_message(&format!("{}) {} - {}", i + 1, artist, title))
                    .await;
            }
        }
        Err(error) => {
            error!("Could not get queue: {:?}", error);
        }
    }
    Ok(())
}

async fn now_playing(context: &Context) -> Result<()> {
    let now_playing_response = api::get_now_playing().await?;

    context
        .send_message(&format!(
            "Now playing: {} - {}{} (Tuned: {})",
            now_playing_response.now_playing.song.artist,
            now_playing_response.now_playing.song.title,
            if now_playing_response.live.is_live {
                " **LIVE**"
            } else {
                ""
            },
            now_playing_response.listeners.current
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

async fn boh(context: &Context, factor: usize) {
    let rating = 10;
    let bohmeter = format!(
        "BOHMETER [{}] ({}%)",
        " ".repeat(20 * factor)
            .replace(' ', "|")
            .chars()
            .take(rating * 2 * factor)
            .collect::<String>(),
        rating * 10 * factor
    );
    context.send_message(&bohmeter).await;
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
    let media_id = now_playing_response.now_playing.song.id;
    let rating_response = api::get_ratings(media_id).await?;
    let message = format!(
        "Ratings for {} - {}:\n",
        now_playing_response.now_playing.song.artist, now_playing_response.now_playing.song.title
    );

    let message = rating_response.ratings.iter().fold(message, |acc, rating| {
        format!("{}{}: {}\n", acc, rating.nick, rating.rating)
    });
    let message = format!("{}Average: {}", message, rating_response.average);
    context.send_message(&message).await;
    Ok(())
}

async fn rate(nickname: &str, context: &Context, args: Vec<&str>) -> Result<()> {
    if args.is_empty() {
        context.send_message("Usage: rate <rating> [comment]").await;
        return Ok(());
    }
    let now_playing_response = api::get_now_playing().await?;
    let is_live = now_playing_response.live.is_live;
    let media_id = if is_live {
        // media_id is not unique for live shows, so we use an MD5 hash of sh_id instead.
        let sh_id = now_playing_response.now_playing.sh_id;
        format!("{:x}", md5::compute(sh_id.to_ne_bytes()))
    } else {
        now_playing_response.now_playing.song.id
    };
    let Ok(rating) = args[0].parse::<f32>() else {
        context.send_message("Invalid rating").await;
        return Ok(());
    };

    let api_response = api::set_rating(
        media_id,
        if is_live { 'L' } else { 'S' },
        0,
        rating,
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
            "Rated {} - {} with a {} (new average {})",
            now_playing_response.now_playing.song.artist,
            now_playing_response.now_playing.song.title,
            rating,
            api_response.average_rating
        ))
        .await;
    Ok(())
}

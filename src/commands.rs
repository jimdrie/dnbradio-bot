use crate::context::Context;
use crate::{shazam, utils};
use anyhow::Result;
use log::warn;

pub(crate) async fn handle_command(
    context: &mut Context,
    command: &str,
    _is_admin: bool,
) -> Result<()> {
    let mut command_parts = command.split(' ');
    let command_name = command_parts.next().unwrap_or("");
    let _command_args = command_parts.collect::<Vec<&str>>();

    match command_name {
        "np" | "ch00n" => now_playing(context).await?,
        "count" | "cunts" => listener_count(context).await?,
        "shazam" => shazam(context).await,
        "submit" => context.send_message("If you're interested in becoming a DJ on the station, please email submissions@dnbradio.com!").await,
        "rate" | "comment" | "ratings" | "comments" => context.send_message("Coming soon!").await,
        "boh" | "bohboh" | "bohbohboh" => boh(context, command_name.matches("boh").count()).await,
        "sched" | "schedule" => schedule(context).await?,
        "incoming" => context.send_message("Incoming TUNE! Get Your Bass Faces On!").await,
        "face" => context.send_message("Cheer up chatroom crew!, at least you don't have a face that looks like ResonantDnB's!").await,
        _ => {
            warn!(
                "Unknown command: {}{}",
                context.command_prefix, command_name
            );
        }
    }
    Ok(())
}
async fn now_playing(context: &mut Context) -> Result<()> {
    let (artist, title, is_live, listeners) = utils::get_now_playing().await?;

    context
        .send_message(&format!(
            "Now playing: {} - {}{} (Tuned: {})",
            artist,
            title,
            if is_live { " **LIVE**" } else { "" },
            listeners
        ))
        .await;
    Ok(())
}

async fn listener_count(context: &mut Context) -> Result<()> {
    let (_, _, _, listeners) = utils::get_now_playing().await?;

    context
        .send_message(&format!("There are {} listeners tuned in!", listeners))
        .await;
    Ok(())
}

async fn shazam(context: &mut Context) {
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

async fn boh(context: &mut Context, factor: usize) {
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

async fn schedule(context: &mut Context) -> Result<()> {
    let schedule = utils::get_schedule().await?;
    let mut schedule_string = String::new();
    for (start, _, title) in schedule {
        let time_difference = (chrono::Utc::now().naive_utc() - start).num_minutes();
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

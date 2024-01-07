use crate::context::Context;
use anyhow::Result;
use chrono::NaiveDateTime;
use dyn_fmt::AsStrFormatExt;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Song {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) artist: String,
    pub(crate) title: String,
    pub(crate) album: String,
    pub(crate) genre: String,
    pub(crate) isrc: String,
    pub(crate) lyrics: String,
    pub(crate) art: String,
    pub(crate) custom_fields: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Listeners {
    pub(crate) total: u64,
    pub(crate) unique: u64,
    pub(crate) current: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Live {
    pub(crate) is_live: bool,
    pub(crate) streamer_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NowPlaying {
    pub(crate) sh_id: u64,
    pub(crate) played_at: u64,
    pub(crate) duration: u64,
    pub(crate) playlist: String,
    pub(crate) streamer: String,
    pub(crate) is_request: bool,
    pub(crate) song: Song,
    pub(crate) elapsed: u64,
    pub(crate) remaining: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NowPlayingResponse {
    pub(crate) now_playing: NowPlaying,
    pub(crate) listeners: Listeners,
    pub(crate) live: Live,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ScheduleResponse {
    pub(crate) id: u64,
    #[serde(rename = "type")]
    pub(crate) type_: String,
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) start_timestamp: u64,
    pub(crate) start: String,
    pub(crate) end_timestamp: u64,
    pub(crate) end: String,
    pub(crate) is_now: bool,
}

pub(crate) async fn get_api_response<T>(path: &str) -> Result<T>
where
    for<'de> T: Deserialize<'de>,
{
    let mut url = env::var("DNBRADIO_API_URL").expect("DNBRADIO_API_URL must be set");
    url.push_str(path);
    let response_text = reqwest::get(url).await?.text().await?;
    Ok(serde_json::from_str(&response_text)?)
}

pub(crate) async fn get_now_playing() -> Result<(String, String, bool, u64)> {
    let api_response = get_api_response::<NowPlayingResponse>("nowplaying/dnbradio").await?;
    Ok((
        api_response.now_playing.song.artist,
        api_response.now_playing.song.title,
        api_response.live.is_live,
        api_response.listeners.current,
    ))
}

pub(crate) async fn get_schedule() -> Result<Vec<(NaiveDateTime, NaiveDateTime, String)>> {
    let api_response =
        get_api_response::<Vec<ScheduleResponse>>("station/dnbradio/schedule").await?;
    Ok(api_response
        .into_iter()
        .map(|schedule| {
            (
                NaiveDateTime::from_timestamp_opt(schedule.start_timestamp as i64, 0).unwrap(),
                NaiveDateTime::from_timestamp_opt(schedule.end_timestamp as i64, 0).unwrap(),
                schedule.title,
            )
        })
        .collect())
}

pub async fn now_playing_loop(context: Context) {
    let now_playing_check_interval = env::var("NOW_PLAYING_CHECK_INTERVAL")
        .expect("NOW_PLAYING_CHECK_INTERVAL must be set")
        .parse()
        .expect("NOW_PLAYING_CHECK_INTERVAL must be a number");
    let now_playing_live_interval = env::var("NOW_PLAYING_LIVE_INTERVAL")
        .expect("NOW_PLAYING_LIVE_INTERVAL must be set")
        .parse()
        .expect("NOW_PLAYING_LIVE_INTERVAL must be a number");

    let irc_default_topic = env::var("IRC_DEFAULT_TOPIC").expect("IRC_DEFAULT_TOPIC must be set");
    let irc_live_topic = env::var("IRC_LIVE_TOPIC").expect("IRC_LIVE_TOPIC must be set");

    let mut last_time_sent = NaiveDateTime::from_timestamp_opt(0, 0).unwrap();
    let mut last_now_playing_string = None;

    loop {
        sleep(Duration::from_secs(now_playing_check_interval)).await;
        if let Ok((artist, title, is_live, listeners)) = get_now_playing().await {
            let now_playing_string = format!(
                "np: {} - {}{}",
                artist,
                title,
                if is_live { " **LIVE**" } else { "" }
            );

            let send_message = match last_now_playing_string {
                Some(ref last) => {
                    now_playing_string != *last
                        || (is_live
                            && chrono::Utc::now().naive_utc() - last_time_sent
                                > chrono::Duration::seconds(now_playing_live_interval))
                }
                None => true,
            };

            if send_message {
                last_time_sent = chrono::Utc::now().naive_utc();
                last_now_playing_string = Some(now_playing_string.clone());
                context
                    .send_message(&format!("{} (Tuned: {})", now_playing_string, listeners))
                    .await;
                _ = context
                    .set_irc_topic(if is_live {
                        irc_live_topic.format(&[artist, title])
                    } else {
                        irc_default_topic.clone()
                    })
                    .await;
            }
        }
    }
}

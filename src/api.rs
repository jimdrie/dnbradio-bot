use crate::context::Context;
use anyhow::Result;
use chrono::{DateTime, Utc};
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

#[derive(Debug, Serialize, Deserialize)]
struct QueueItem {
    pub(crate) song: Song,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Rating {
    pub(crate) media_id: String,
    pub(crate) media_type: char,
    pub(crate) user_id: usize,
    pub(crate) rating: f32,
    pub(crate) channel: String,
    pub(crate) nick: String,
    pub(crate) comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RateResponse {
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) average_rating: f32,
    pub(crate) comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RatingsResponse {
    pub(crate) average_rating: f32,
    pub(crate) ratings: Vec<Rating>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Comment {
    pub(crate) media_id: String,
    pub(crate) media_type: char,
    pub(crate) user_id: usize,
    pub(crate) channel: String,
    pub(crate) comment: String,
    pub(crate) nick: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CommentResponse {
    pub(crate) status: String,
    pub(crate) message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CommentsResponse {
    pub(crate) comments: Vec<Comment>,
}

pub(crate) async fn get_dnbradio_api_response<T>(path: &str) -> Result<T>
where
    for<'de> T: Deserialize<'de>,
{
    let mut url = env::var("DNBRADIO_API_URL").expect("DNBRADIO_API_URL must be set");
    url.push_str(path);
    let client = reqwest::Client::new();
    let response_text = client.get(&url).send().await?.text().await?;
    log::debug!("API response: {}", response_text);
    Ok(serde_json::from_str(&response_text)?)
}

pub(crate) async fn post_dnbradio_api_response<T1, T2>(path: &str, body: T1) -> Result<T2>
where
    T1: Serialize,
    for<'de> T2: Deserialize<'de>,
{
    let mut url = env::var("DNBRADIO_API_URL").expect("DNBRADIO_AZURACAST_API_URL must be set");
    url.push_str(path);
    let client = reqwest::Client::new();
    let response_text = client.post(&url).json(&body).send().await?.text().await?;
    log::debug!("API response: {}", response_text);
    Ok(serde_json::from_str(&response_text)?)
}

pub(crate) async fn get_azuracast_api_response<T>(path: &str) -> Result<T>
where
    for<'de> T: Deserialize<'de>,
{
    let mut url =
        env::var("DNBRADIO_AZURACAST_API_URL").expect("DNBRADIO_AZURACAST_API_URL must be set");
    let api_key =
        env::var("DNBRADIO_AZURACAST_API_KEY").expect("DNBRADIO_AZURACAST_API_KEY must be set");
    url.push_str(path);
    let client = reqwest::Client::new();
    let response_text = client
        .get(&url)
        .header("X-API-Key", api_key)
        .send()
        .await?
        .text()
        .await?;
    Ok(serde_json::from_str(&response_text)?)
}

pub(crate) async fn get_now_playing() -> Result<NowPlayingResponse> {
    let now_playing_response =
        get_azuracast_api_response::<NowPlayingResponse>("nowplaying/dnbradio").await?;
    let is_live = now_playing_response.live.is_live;
    let media_id = if is_live {
        // media_id is not unique for live shows, so we use an MD5 hash of sh_id instead.
        let sh_id = now_playing_response.now_playing.sh_id;
        format!("{:x}", md5::compute(sh_id.to_ne_bytes()))
    } else {
        now_playing_response.now_playing.song.id
    };
    // Replace media_id in the response.
    Ok(NowPlayingResponse {
        now_playing: NowPlaying {
            song: Song {
                id: media_id,
                ..now_playing_response.now_playing.song
            },
            ..now_playing_response.now_playing
        },
        ..now_playing_response
    })
}

pub(crate) async fn get_schedule() -> Result<Vec<(DateTime<Utc>, DateTime<Utc>, String)>> {
    let api_response =
        get_azuracast_api_response::<Vec<ScheduleResponse>>("station/dnbradio/schedule").await?;
    Ok(api_response
        .into_iter()
        .map(|schedule| {
            (
                DateTime::from_timestamp(schedule.start_timestamp as i64, 0).unwrap(),
                DateTime::from_timestamp(schedule.end_timestamp as i64, 0).unwrap(),
                schedule.title,
            )
        })
        .collect())
}

pub(crate) async fn get_queue() -> Result<Vec<(String, String)>> {
    let api_response =
        get_azuracast_api_response::<Vec<QueueItem>>("station/dnbradio/queue").await?;
    Ok(api_response
        .into_iter()
        .map(|song| (song.song.artist, song.song.title))
        .collect())
}

pub(crate) async fn set_rating(
    media_id: String,
    media_type: char,
    user_id: usize,
    rating: f32,
    channel: String,
    nick: String,
    comment: Option<String>,
) -> Result<RateResponse> {
    let api_response = post_dnbradio_api_response::<Rating, RateResponse>(
        &format!("media/{}/rating", media_id),
        Rating {
            media_id,
            media_type,
            user_id,
            rating,
            channel,
            nick,
            comment,
        },
    )
    .await?;
    Ok(api_response)
}

pub(crate) async fn get_ratings(song_id: String) -> Result<RatingsResponse> {
    let api_response =
        get_dnbradio_api_response::<RatingsResponse>(&format!("media/{}/rating", song_id)).await?;
    Ok(api_response)
}

pub(crate) async fn add_comment(
    media_id: String,
    media_type: char,
    user_id: usize,
    channel: String,
    nick: String,
    comment: String,
) -> Result<CommentResponse> {
    let api_response = post_dnbradio_api_response::<Comment, CommentResponse>(
        &format!("media/{}/comment", media_id),
        Comment {
            media_id,
            media_type,
            user_id,
            channel,
            nick,
            comment,
        },
    )
    .await?;
    Ok(api_response)
}

pub(crate) async fn get_comments(song_id: String) -> Result<CommentsResponse> {
    let api_response =
        get_dnbradio_api_response::<CommentsResponse>(&format!("media/{}/comment", song_id))
            .await?;
    Ok(api_response)
}

pub(crate) async fn now_playing_loop(context: Context) {
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

    let mut last_time_sent = DateTime::from_timestamp(0, 0).unwrap();
    let mut last_now_playing_string = None;

    loop {
        sleep(Duration::from_secs(now_playing_check_interval)).await;
        if let Ok(now_playing_response) = get_now_playing().await {
            let NowPlayingResponse {
                now_playing:
                    NowPlaying {
                        song: Song { artist, title, .. },
                        ..
                    },
                listeners: Listeners {
                    current: listeners, ..
                },
                live,
            } = now_playing_response;

            let is_live = live.is_live;
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
                            && chrono::Utc::now() - last_time_sent
                                > chrono::Duration::seconds(now_playing_live_interval))
                }
                None => true,
            };

            if send_message {
                last_time_sent = chrono::Utc::now();
                last_now_playing_string = Some(now_playing_string.clone());
                context
                    .send_action(&format!("{} (Tuned: {})", now_playing_string, listeners))
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

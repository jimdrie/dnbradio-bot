use crate::context::{Context, ShazamTrack}; // Use ShazamTrack from the context module
use anyhow::{anyhow, Result};
use log::info;
use serde::{Deserialize, Serialize};
use std::env;

pub mod fingerprinting {
    pub mod algorithm;
    pub mod communication;
    mod hanning;
    pub mod signature_format;
    mod user_agent;
}

pub use fingerprinting::algorithm::SignatureGenerator;
pub use fingerprinting::communication::recognize_song_from_signature;
pub use fingerprinting::signature_format::DecodedSignature;

pub fn get_last_sent_track(context: &Context) -> Option<(chrono::NaiveDateTime, String)> {
    context.last_track.read().unwrap().clone()
}

pub fn set_last_sent_track(context: &Context, track: Option<(chrono::NaiveDateTime, String)>) {
    let mut last_track = context.last_track.write().unwrap();
    *last_track = track;
}

pub(crate) async fn start(context: Context) {
    let input_url = env::var("SHAZAM_INPUT_URL").expect("SHAZAM_INPUT_URL must be set");
    let mut last_track: Option<ShazamTrack> = None; // Use ShazamTrack from context
    loop {
        let track = match recognize_from_stream(&input_url).await {
            Ok(track) => track,
            Err(e) => {
                info!("Error recognizing song: {:?}", e);
                continue;
            }
        };
        println!("{:?}", track);
        let track_id = format!("{} - {}", track.subtitle, track.title); // This is used for comparison
        let last_sent_track = get_last_sent_track(&context);
        if let Some((_, last_sent_track_id)) = last_sent_track {
            if track_id == last_sent_track_id {
                continue;
            }
        }

        if let Some(last_track) = &last_track {
            let last_track_id = format!("{} - {}", last_track.subtitle, last_track.title);
            if track_id == last_track_id {
                set_last_sent_track(
                    &context,
                    Some((chrono::Utc::now().naive_utc(), track_id.clone())),
                );
                context.send_shazam(track_id.as_str()).await;
                context.send_shazam_to_webhook(last_track).await; // Use ShazamTrack from context
            }
        }
        last_track = Some(track.clone());
    }
}

pub async fn recognize_from_stream(input_url: &str) -> Result<ShazamTrack> {
    match SignatureGenerator::make_signature_from_url(input_url).await {
        Ok(signature) => {
            let response = recognize_song_from_signature(&signature)
                .await
                .map_err(|e| anyhow!("{e:?}"))?;
            let response: ShazamResponse = serde_json::from_value(response)?;
            match response.track {
                Some(track) => Ok(track),
                None => Err(anyhow!("Shazam returned no matches!")),
            }
        }
        Err(e) => Err(anyhow!("Error making signature: {:?}", e)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShazamResponse {
    pub timestamp: u64,
    pub tagid: String,
    pub track: Option<ShazamTrack>,
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

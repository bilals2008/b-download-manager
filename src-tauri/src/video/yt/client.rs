//! The InnerTube player clients: their profiles and the player call.

use super::*;

pub(super) fn ios_profile(visitor: &str) -> PlayerProfile {
    PlayerProfile {
        label: "iOS",
        user_agent: IOS_UA,
        player_client: json!({
            "hl": "en",
            "gl": "US",
            "screenDensityFloat": 1,
            "screenHeightPoints": 1440,
            "screenPixelDensity": 1,
            "screenWidthPoints": 2560,
            "visitorData": visitor,
            "clientName": "iOS",
            "clientVersion": IOS_CLIENT_VERSION,
            "osName": "iOS",
            "osVersion": "16.7.7.20H330",
            "userAgent": IOS_UA,
            "platform": "MOBILE",
            "clientFormFactor": "UNKNOWN_FORM_FACTOR",
            "userInterfaceTheme": "USER_INTERFACE_THEME_LIGHT",
            "deviceMake": "Apple",
            "deviceModel": "iPhone10,4",
            "memoryTotalKbytes": "8000000"
        }),
        client_name: Some("5"),
        client_version: IOS_CLIENT_VERSION,
    }
}

pub(super) fn vr_profile(visitor: &str) -> PlayerProfile {
    PlayerProfile {
        label: "ANDROID_VR",
        user_agent: VR_UA,
        player_client: json!({
            "clientName": "ANDROID_VR",
            "clientVersion": VR_CLIENT_VERSION,
            "deviceMake": "Oculus",
            "deviceModel": "Quest 3",
            "osName": "Android",
            "osVersion": "12L",
            "platform": "MOBILE",
            "visitorData": visitor,
            "hl": "en",
            "gl": "US",
            "utcOffsetMinutes": 0
        }),
        // The VR client is not asked through the named header pair.
        client_name: None,
        client_version: VR_CLIENT_VERSION,
    }
}
/// The player answer of one client.
pub(super) async fn ask_player(
    client: &reqwest::Client,
    id: &str,
    visitor: &str,
    sts: u64,
    profile: &PlayerProfile,
) -> Result<Value, YtError> {
    let body = json!({
        "videoId": id,
        "racyCheckOk": true,
        "contentCheckOk": true,
        "playbackContext": {
            "contentPlaybackContext": {
                "vis": 0,
                "splay": false,
                "lactMilliseconds": "-1",
                "signatureTimestamp": sts
            }
        },
        "context": {
            "client": profile.player_client,
            "user": { "enableSafetyMode": false, "lockedSafetyMode": false },
            "request": { "useSsl": true, "internalExperimentFlags": [] }
        }
    });
    let mut request = client
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false&alt=json")
        .header("user-agent", profile.user_agent)
        .header("accept", "*/*")
        .header("accept-language", "*")
        .header("origin", "https://www.youtube.com")
        .header("x-goog-visitor-id", visitor)
        .header("content-type", "application/json");
    if let Some(name) = profile.client_name {
        request = request
            .header("x-youtube-client-name", name)
            .header("x-youtube-client-version", profile.client_version);
    }
    let response = request
        .body(serde_json::to_vec(&body).map_err(|error| YtError::Client {
            client: profile.label.to_string(),
            reason: error.to_string(),
        })?)
        .send()
        .await
        .map_err(|error| YtError::Transient {
            message: format!("the video could not be asked for: {error}"),
            retry_after: None,
        })?;
    let status = response.status();
    let retry_after = idm_core::http::parse_retry_after(response.headers());
    let text = response.text().await.map_err(|error| YtError::Transient {
        message: format!("the video answer could not be read: {error}"),
        retry_after: None,
    })?;
    if status.as_u16() == 429 || status.is_server_error() {
        return Err(YtError::Transient {
            message: format!("the player answered {}", status.as_u16()),
            retry_after,
        });
    }
    serde_json::from_str(&text).map_err(|error| YtError::Client {
        client: profile.label.to_string(),
        reason: format!("the video answer could not be read: {error}"),
    })
}

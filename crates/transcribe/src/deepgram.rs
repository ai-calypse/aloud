//! Streaming speech-to-text on Deepgram's Flux, for always-listening.
//!
//! Flux decides where a turn ends from the words, not just the silence — "open
//! … um … Calculator" stays one request where a pause detector cuts it in two —
//! so one socket yields both the words as they are said and the finished
//! request. https://developers.deepgram.com/reference/speech-to-text/listen-flux

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

const URL: &str = "wss://api.deepgram.com/v2/listen";
/// Flux accepts up to 100 keyterms; past that the request is refused.
const MAX_KEYTERMS: usize = 100;

/// What the stream heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// The current turn so far, revised as more is said. Empty between turns.
    Partial(String),
    /// A finished turn. Empty when the turn was noise with no words in it.
    Turn(String),
}

/// Streams `audio` (mono PCM16 at `sample_rate`) to Flux, reporting what it
/// hears, until the channel closes (`Ok`) or the socket fails (`Err` — the
/// caller decides whether to reconnect). `vocabulary` becomes keyterms.
pub async fn stream(
    key: &str,
    sample_rate: u32,
    vocabulary: &[String],
    audio: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    mut on_heard: impl FnMut(Heard),
) -> Result<()> {
    let rate = sample_rate.to_string();
    let params = [
        ("model", "flux-general-en"),
        ("encoding", "linear16"),
        ("sample_rate", rate.as_str()),
    ]
    .into_iter()
    .chain(
        vocabulary
            .iter()
            .take(MAX_KEYTERMS)
            .map(|term| ("keyterm", term.as_str())),
    );
    let url = reqwest::Url::parse_with_params(URL, params)?;
    let mut request = url.as_str().into_client_request()?;
    request
        .headers_mut()
        .insert("Authorization", format!("Token {key}").parse()?);

    let (socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .context("connecting to Deepgram")?;
    let (mut tx, mut rx) = socket.split();
    loop {
        tokio::select! {
            chunk = audio.recv() => match chunk {
                Some(pcm) => tx.send(Message::binary(pcm)).await.context("sending audio to Deepgram")?,
                None => {
                    // Lets Deepgram finish the turn in flight rather than drop it.
                    let _ = tx.send(Message::text(r#"{"type":"CloseStream"}"#)).await;
                    return Ok(());
                }
            },
            message = rx.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Some(heard) = parse(&text)? {
                        on_heard(heard);
                    }
                }
                Some(Ok(Message::Close(frame))) => bail!("Deepgram closed the stream: {frame:?}"),
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(e).context("reading from Deepgram"),
                None => bail!("Deepgram closed the stream"),
            },
        }
    }
}

/// One server message. `Err` only for Deepgram's own error message, which ends
/// the stream; anything unrecognised is skipped so a new message type cannot.
fn parse(text: &str) -> Result<Option<Heard>> {
    #[derive(Deserialize)]
    struct Message {
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        event: String,
        #[serde(default)]
        transcript: String,
        #[serde(default)]
        description: String,
    }
    let Ok(message) = serde_json::from_str::<Message>(text) else {
        return Ok(None);
    };
    match message.kind.as_str() {
        "TurnInfo" => {}
        "Error" => bail!("Deepgram: {}", message.description),
        _ => return Ok(None),
    }
    let transcript = message.transcript.trim().to_string();
    Ok(match message.event.as_str() {
        "EndOfTurn" => Some(Heard::Turn(transcript)),
        "StartOfTurn" | "Update" | "EagerEndOfTurn" | "TurnResumed" => {
            Some(Heard::Partial(transcript))
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn_info(event: &str, transcript: &str) -> String {
        serde_json::json!({
            "type": "TurnInfo", "event": event, "turn_index": 0,
            "transcript": transcript, "words": [], "end_of_turn_confidence": 0.9,
        })
        .to_string()
    }

    #[test]
    fn updates_are_partials_and_end_of_turn_is_the_request() {
        assert_eq!(
            parse(&turn_info("Update", " open calc")).unwrap(),
            Some(Heard::Partial("open calc".into()))
        );
        assert_eq!(
            parse(&turn_info("EndOfTurn", "Open Calculator.")).unwrap(),
            Some(Heard::Turn("Open Calculator.".into()))
        );
    }

    /// A cough ends a turn too; it must reach the caller (to clear the live
    /// text) but as nothing to run.
    #[test]
    fn a_wordless_turn_is_an_empty_turn() {
        assert_eq!(
            parse(&turn_info("EndOfTurn", "")).unwrap(),
            Some(Heard::Turn(String::new()))
        );
    }

    #[test]
    fn a_deepgram_error_ends_the_stream() {
        let error = r#"{"type":"Error","code":"INVALID_AUTH","description":"bad key"}"#;
        assert!(parse(error).unwrap_err().to_string().contains("bad key"));
    }

    #[test]
    fn unknown_and_malformed_messages_are_skipped() {
        assert_eq!(parse(r#"{"type":"Connected"}"#).unwrap(), None);
        assert_eq!(parse("not json").unwrap(), None);
    }
}

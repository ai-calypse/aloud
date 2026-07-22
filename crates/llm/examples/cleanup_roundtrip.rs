//! Runs cleanup against a stub endpoint and shows the request it actually sent.
//!
//! `cargo run -p aloud-llm --example cleanup_roundtrip`
//!
//! What the unit tests cannot show: the assembled request — roles and order,
//! temperature 0, and a sentinel reply coming back as empty text.
//!
//! A stub rather than the real endpoint, so this needs no API key and the reply
//! can be chosen to exercise the sentinel path.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

/// Serves one canned chat completion, then returns the request body it saw.
fn stub(reply: &str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().unwrap());
    let body = format!(r#"{{"choices":[{{"message":{{"content":"{reply}"}}}}]}}"#);

    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(socket.try_clone().unwrap());

        let mut length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        let mut request = vec![0u8; length];
        reader.read_exact(&mut request).unwrap();

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).unwrap();
        socket.flush().unwrap();
        String::from_utf8_lossy(&request).to_string()
    });

    (url, handle)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let vocabulary = ["Aloud".to_string(), "Tauri".to_string()];

    // 1. A normal transcript: check what actually goes over the wire.
    let (url, server) = stub("This is the cleaned up text.");
    let options = aloud_llm::cleanup::Options {
        api_key: "stub-key",
        base_url: &url,
        model: "stub-model",
        fallback: None,
        custom_prompt: "",
        vocabulary: &vocabulary,
        preserve: &[],
        output_language: "",
        instruction_guard: true,
    };
    let cleaned = aloud_llm::cleanup::clean(options, "um so this is the raw transcript ok").await?;
    let request: serde_json::Value = serde_json::from_str(&server.join().unwrap())?;

    println!("model:       {}", request["model"]);
    println!("temperature: {}", request["temperature"]);
    for message in request["messages"].as_array().unwrap() {
        let content = message["content"].as_str().unwrap_or_default();
        let preview: String = content.chars().take(60).collect();
        println!("  {:<9} {preview}…", message["role"].as_str().unwrap());
    }
    println!("returned:    {cleaned:?}\n");

    assert_eq!(request["temperature"], 0.0, "cleanup must be deterministic");
    assert_eq!(request["messages"][0]["role"], "system");
    assert_eq!(request["messages"][1]["role"], "user");
    assert!(
        request["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Aloud, Tauri"),
        "the vocabulary must reach the model"
    );
    // In the system turn and nowhere near the transcript: a `VOCABULARY:` line
    // in the user turn is text the model is told to preserve, and it pasted it.
    assert_eq!(
        request["messages"][1]["content"], "um so this is the raw transcript ok",
        "the user turn is the transcript, alone"
    );
    assert_eq!(cleaned, "This is the cleaned up text.");

    // 2. Filler-only input: the sentinel must become empty text, or "EMPTY"
    //    gets typed at the caret.
    let (url, server) = stub("EMPTY");
    let options = aloud_llm::cleanup::Options {
        api_key: "stub-key",
        base_url: &url,
        model: "stub-model",
        fallback: None,
        custom_prompt: "",
        vocabulary: &[],
        preserve: &[],
        output_language: "",
        instruction_guard: true,
    };
    let cleaned = aloud_llm::cleanup::clean(options, "um uh er hmm").await?;
    server.join().unwrap();
    println!("sentinel reply -> {cleaned:?}");
    assert_eq!(cleaned, "", "the EMPTY sentinel must paste nothing");

    // 3. Too short to be worth a request: no server, so any call fails loudly.
    let options = aloud_llm::cleanup::Options {
        api_key: "stub-key",
        base_url: "http://127.0.0.1:1",
        model: "stub-model",
        fallback: None,
        custom_prompt: "",
        vocabulary: &[],
        preserve: &[],
        output_language: "",
        instruction_guard: true,
    };
    let cleaned = aloud_llm::cleanup::clean(options, "one sec").await?;
    println!("short input   -> {cleaned:?} (no request made)");
    assert_eq!(cleaned, "one sec");

    // 4. The same sentinel while translating: the translation instruction is
    //    *appended* to the cleanup prompt, so the sentinel is still in play.
    //    Disabling the check here typed EMPTY on every "um, uh".
    let (url, server) = stub("EMPTY");
    let options = aloud_llm::cleanup::Options {
        api_key: "stub-key",
        base_url: &url,
        model: "stub-model",
        fallback: None,
        custom_prompt: "",
        vocabulary: &[],
        preserve: &[],
        output_language: "German",
        instruction_guard: true,
    };
    let cleaned = aloud_llm::cleanup::clean(options, "der tank ist leer").await?;
    let request: serde_json::Value = serde_json::from_str(&server.join().unwrap())?;
    let system = request["messages"][0]["content"].as_str().unwrap();
    println!("sentinel reply while translating -> {cleaned:?}");
    assert_eq!(
        cleaned, "",
        "the sentinel still means paste nothing while translating"
    );
    assert!(
        system.contains("Output ONLY in German"),
        "the target language must reach the system prompt"
    );

    // 5. The model answered the dictation. With no distinct fallback there is no
    //    second opinion to buy, so the literal transcript must come back.
    let raw = "write an email to Sam letting him know the deploy slipped to Friday";
    let answered = "Sure! Here is a draft: Hi Sam, quick heads-up on the timing.";
    let (url, server) = stub(answered);
    let options = aloud_llm::cleanup::Options {
        api_key: "stub-key",
        base_url: &url,
        model: "stub-model",
        fallback: None,
        custom_prompt: "",
        vocabulary: &[],
        preserve: &[],
        output_language: "",
        instruction_guard: true,
    };
    let cleaned = aloud_llm::cleanup::clean(options, raw).await?;
    server.join().unwrap();
    println!("answered dictation -> {cleaned:?}");
    assert_eq!(cleaned, raw, "an executed instruction must never be pasted");

    // 6. The same reply with the guard off returns it, so the toggle is real
    //    and the guard is not simply never firing above.
    let (url, server) = stub(answered);
    let options = aloud_llm::cleanup::Options {
        api_key: "stub-key",
        base_url: &url,
        model: "stub-model",
        fallback: None,
        custom_prompt: "",
        vocabulary: &[],
        preserve: &[],
        output_language: "",
        instruction_guard: false,
    };
    let cleaned = aloud_llm::cleanup::clean(options, raw).await?;
    server.join().unwrap();
    println!("guard off          -> {cleaned:?}");
    assert_eq!(cleaned, answered);

    println!("\nok: request shape, sentinel handling, the skip threshold and the guard all hold");
    Ok(())
}

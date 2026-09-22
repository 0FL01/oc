//! Strict ancillary title requests never consume the main-turn script.
use std::{io::Write, net::TcpStream};

pub fn is_title(body: &serde_json::Value) -> bool {
    body["max_output_tokens"] == 256 && body["tools"].as_array().is_none_or(|v| v.is_empty())
}

pub fn respond(socket: &mut TcpStream, body: &serde_json::Value) -> bool {
    if !is_title(body) {
        return false;
    }
    assert!(body["model"].as_str().is_some_and(|id| !id.is_empty()));
    let input = body["input"].as_array().expect("title input");
    assert_eq!(input.len(), 2);
    assert_eq!(input[0]["role"], "developer");
    assert_eq!(input[1]["role"], "user");
    let reply = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Fixture session title\"}]}]}}\n\n";
    // Quit/cancel may close an ancillary request after the main answer delta.
    // Request shape is still asserted; a peer disconnect is expected there.
    let _ = write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
    true
}

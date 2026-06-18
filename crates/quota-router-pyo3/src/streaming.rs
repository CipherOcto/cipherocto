// Streaming support for PyO3 bindings
// Provides SSE parsing and chunk-based streaming responses

use crate::types::{ChatCompletionChunk, ChunkChoice, Message};
use pyo3::prelude::*;
use pyo3::types::PyList;

/// SSE event parsed from provider streaming response
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SSEEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}

/// Parse OpenAI-style SSE format: `data: {...}\n\n` + `data: [DONE]\n\n`
///
/// # Arguments
/// * `raw` - Raw SSE bytes from OpenAI-compatible streaming response
///
/// # Returns
/// Vector of parsed SSE events with event_type="message" and data as JSON value
///
/// # Example
/// ```rust
/// let raw = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\ndata: [DONE]\n\n";
/// let events = parse_openai_sse(raw);
/// ```
#[allow(dead_code)]
pub fn parse_openai_sse(raw: &[u8]) -> Vec<SSEEvent> {
    let mut events = Vec::new();
    let text = match std::str::from_utf8(raw) {
        Ok(s) => s,
        Err(_) => return events,
    };

    for line in text.split("\n") {
        let trimmed = line.trim();
        if !trimmed.starts_with("data:") {
            continue;
        }
        let data_str = trimmed[5..].trim();
        if data_str.is_empty() {
            continue;
        }
        if data_str == "[DONE]" {
            events.push(SSEEvent {
                event_type: "done".to_string(),
                data: serde_json::Value::Null,
            });
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(data_str) {
            Ok(value) => {
                events.push(SSEEvent {
                    event_type: "message".to_string(),
                    data: value,
                });
            }
            Err(_) => continue,
        }
    }
    events
}

/// Parse Anthropic-style SSE format: `event: type\n data: {...}\n\n`
///
/// # Arguments
/// * `raw` - Raw SSE bytes from Anthropic streaming response
///
/// # Returns
/// Vector of parsed SSE events with proper event_type from `event:` line
///
/// # Example
/// ```rust
/// let raw = b"event: message_delta\ndata: {\"delta\":{\"text\":\"Hello\"}}\n\nevent: done\ndata: {}\n\n";
/// let events = parse_anthropic_sse(raw);
/// ```
#[allow(dead_code)]
pub fn parse_anthropic_sse(raw: &[u8]) -> Vec<SSEEvent> {
    let mut events = Vec::new();
    let text = match std::str::from_utf8(raw) {
        Ok(s) => s,
        Err(_) => return events,
    };

    let mut current_event_type: Option<String> = None;
    let mut current_data: Option<String> = None;

    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // Empty line marks end of event
            if let (Some(event_type), Some(data_str)) =
                (current_event_type.take(), current_data.take())
            {
                if data_str == "{}" {
                    events.push(SSEEvent {
                        event_type,
                        data: serde_json::Value::Null,
                    });
                } else if let Ok(value) = serde_json::from_str::<serde_json::Value>(&data_str) {
                    events.push(SSEEvent {
                        event_type,
                        data: value,
                    });
                }
            }
            continue;
        }

        if let Some(stripped) = trimmed.strip_prefix("event:") {
            current_event_type = Some(stripped.trim().to_string());
        } else if let Some(stripped) = trimmed.strip_prefix("data:") {
            current_data = Some(stripped.trim().to_string());
        }
    }

    events
}

/// Create a list of streaming chunks for a completion response
///
/// In non-mock implementations, this would yield chunks as they arrive
/// from the provider. This simplified version returns all chunks at once.
pub fn create_chunk_list(model: String, content: String) -> Vec<ChatCompletionChunk> {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());

    // Create chunks simulating word-by-word streaming
    let words: Vec<&str> = content.split_whitespace().collect();
    let mut chunks = Vec::new();

    for (i, word) in words.iter().enumerate() {
        let is_last = i == words.len() - 1;
        let delta = Message::new(
            "assistant",
            if is_last {
                word.to_string()
            } else {
                format!("{} ", word)
            },
        );
        let choice = if is_last {
            ChunkChoice::with_finish_reason(0, delta, "stop")
        } else {
            ChunkChoice::new(0, delta)
        };
        chunks.push(ChatCompletionChunk::new(id.clone(), model.clone(), choice));
    }

    // If no chunks created (empty content), return a single finish chunk
    if chunks.is_empty() {
        let delta = Message::new("assistant", "");
        let choice = ChunkChoice::with_finish_reason(0, delta, "stop");
        chunks.push(ChatCompletionChunk::new(id, model, choice));
    }

    chunks
}

/// Create chunks from parsed SSE events (OpenAI format)
///
/// # Arguments
/// * `events` - Parsed SSE events from parse_openai_sse()
/// * `model` - Model name for chunk metadata
///
/// # Returns
/// Vector of ChatCompletionChunk ready for Python serialization
#[allow(dead_code)]
pub fn chunks_from_openai_events(events: Vec<SSEEvent>, model: String) -> Vec<ChatCompletionChunk> {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let mut chunks = Vec::new();
    let mut index = 0u32;

    for event in events {
        if event.event_type == "done" {
            // Send final chunk with finish_reason
            let delta = Message::new("assistant", "");
            let choice = ChunkChoice::with_finish_reason(index, delta, "stop");
            chunks.push(ChatCompletionChunk::new(id.clone(), model.clone(), choice));
            break;
        }

        // Extract delta content from OpenAI chunk format
        let content = extract_delta_content(&event.data);
        if content.is_empty() {
            continue;
        }

        let delta = Message::new("assistant", content);
        let choice = ChunkChoice::new(index, delta);
        chunks.push(ChatCompletionChunk::new(id.clone(), model.clone(), choice));
        index += 1;
    }

    // If no chunks created, return a single finish chunk
    if chunks.is_empty() {
        let delta = Message::new("assistant", "");
        let choice = ChunkChoice::with_finish_reason(0, delta, "stop");
        chunks.push(ChatCompletionChunk::new(id, model, choice));
    }

    chunks
}

/// Create chunks from parsed SSE events (Anthropic format)
///
/// # Arguments
/// * `events` - Parsed SSE events from parse_anthropic_sse()
/// * `model` - Model name for chunk metadata
///
/// # Returns
/// Vector of ChatCompletionChunk with Anthropic delta extracted
#[allow(dead_code)]
pub fn chunks_from_anthropic_events(
    events: Vec<SSEEvent>,
    model: String,
) -> Vec<ChatCompletionChunk> {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let mut chunks = Vec::new();
    let mut index = 0u32;

    for event in events {
        match event.event_type.as_str() {
            "done" => {
                let delta = Message::new("assistant", "");
                let choice = ChunkChoice::with_finish_reason(index, delta, "stop");
                chunks.push(ChatCompletionChunk::new(id.clone(), model.clone(), choice));
                break;
            }
            "message_delta" => {
                if let Some(content) = extract_anthropic_delta(&event.data) {
                    let delta = Message::new("assistant", content);
                    let choice = ChunkChoice::new(index, delta);
                    chunks.push(ChatCompletionChunk::new(id.clone(), model.clone(), choice));
                    index += 1;
                }
            }
            "content_block_start" | "content_block_delta" | "message_start" => {
                if let Some(content) = extract_anthropic_delta(&event.data) {
                    let delta = Message::new("assistant", content);
                    let choice = ChunkChoice::new(index, delta);
                    chunks.push(ChatCompletionChunk::new(id.clone(), model.clone(), choice));
                    index += 1;
                }
            }
            _ => continue,
        }
    }

    // If no chunks created, return a single finish chunk
    if chunks.is_empty() {
        let delta = Message::new("assistant", "");
        let choice = ChunkChoice::with_finish_reason(0, delta, "stop");
        chunks.push(ChatCompletionChunk::new(id, model, choice));
    }

    chunks
}

/// Extract delta content from OpenAI streaming chunk
#[allow(dead_code)]
fn extract_delta_content(data: &serde_json::Value) -> String {
    data.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("delta"))
        .and_then(|d| d.get("content"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default()
}

/// Extract delta content from Anthropic streaming event
#[allow(dead_code)]
fn extract_anthropic_delta(data: &serde_json::Value) -> Option<String> {
    data.get("delta")
        .and_then(|d| d.get("text"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Convert a list of chunks to Python list of dicts
pub fn chunks_to_pylist(chunks: Vec<ChatCompletionChunk>, py: Python<'_>) -> PyResult<Py<PyAny>> {
    let list = PyList::new(py, Vec::<&PyAny>::new());
    for chunk in chunks {
        list.append(chunk.to_dict(py)?)?;
    }
    Ok(list.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openai_sse_basic() {
        let raw = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\ndata: [DONE]\n\n";
        let events = parse_openai_sse(raw);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "message");
        assert_eq!(events[1].event_type, "done");
    }

    #[test]
    fn test_parse_openai_sse_multiple() {
        let raw = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\ndata: [DONE]\n\n";
        let events = parse_openai_sse(raw);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_parse_openai_sse_empty() {
        let raw = b"";
        let events = parse_openai_sse(raw);
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_anthropic_sse_basic() {
        let raw = b"event: message_delta\ndata: {\"delta\":{\"text\":\"Hello\"}}\n\nevent: done\ndata: {}\n\n";
        let events = parse_anthropic_sse(raw);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "message_delta");
        assert_eq!(events[1].event_type, "done");
    }

    #[test]
    fn test_parse_anthropic_sse_multiple() {
        let raw = b"event: content_block_start\ndata: {\"index\":0,\"type\":\"content_block\"}\n\nevent: content_block_delta\ndata: {\"index\":0,\"type\":\"content_block\",\"delta\":{\"text\":\"Hello\"}}\n\nevent: done\ndata: {}\n\n";
        let events = parse_anthropic_sse(raw);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_chunks_from_openai_events() {
        let raw = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\ndata: [DONE]\n\n";
        let events = parse_openai_sse(raw);
        let chunks = chunks_from_openai_events(events, "gpt-4".to_string());
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunks_from_anthropic_events() {
        let raw = b"event: content_block_delta\ndata: {\"delta\":{\"text\":\"Hello\"}}\n\nevent: done\ndata: {}\n\n";
        let events = parse_anthropic_sse(raw);
        let chunks = chunks_from_anthropic_events(events, "claude-3".to_string());
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_list() {
        let chunks = create_chunk_list("gpt-4".to_string(), "Hello world".to_string());
        assert_eq!(chunks.len(), 2); // "Hello" and "world"
    }

    #[test]
    fn test_chunk_list_empty() {
        let chunks = create_chunk_list("gpt-4".to_string(), "".to_string());
        assert_eq!(chunks.len(), 1); // Single finish chunk
    }
}

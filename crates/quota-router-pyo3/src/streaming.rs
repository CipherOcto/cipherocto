// Streaming support for PyO3 bindings
// Provides chunk-based streaming responses

use crate::types::{ChatCompletionChunk, ChunkChoice, Message};
use pyo3::prelude::*;
use pyo3::types::PyList;

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

// Completion functions for PyO3 bindings

#![allow(clippy::too_many_arguments)]

use crate::types::{ChatCompletion, Choice, Message};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// completion - Sync completion call
#[pyfunction]
#[pyo3(name = "completion", text_signature = "(model, messages, **kwargs)")]
pub fn completion(
    model: String,
    messages: Vec<Message>,
    // Optional parameters (match LiteLLM)
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _top_p: Option<f64>,
    _n: Option<i32>,
    _stream: Option<bool>,
    _stop: Option<String>,
    _presence_penalty: Option<f64>,
    _frequency_penalty: Option<f64>,
    _user: Option<String>,
    // quota-router specific
    _api_key: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Log the request parameters (for debugging)
    println!(
        "completion called: model={}, messages={}",
        model,
        messages.len()
    );

    // Convert messages to response choices
    let choices: Vec<Choice> = messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            Choice::new(
                i as u32,
                Message::new("assistant", format!("Echo: {}", msg.content)),
                "stop",
            )
        })
        .collect();

    let response = ChatCompletion::new(
        format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        model,
        choices,
    );

    // Convert to Python dict
    let result = Python::with_gil(|py| response.to_dict(py))?;

    Ok(result)
}

/// embedding - Sync embedding call
#[pyfunction]
#[pyo3(name = "embedding", text_signature = "(input, model, **kwargs)")]
pub fn embedding(
    input: Vec<String>,
    model: String,
) -> PyResult<Py<PyAny>> {
    println!("embedding called: model={}, input={}", model, input.len());

    // Mock embedding response
    let embeddings: Vec<crate::types::Embedding> = input
        .iter()
        .enumerate()
        .map(|(i, _)| {
            // Generate a simple mock embedding (in production, call the model)
            let embedding: Vec<f32> = (0..384).map(|_| 0.1).collect();
            crate::types::Embedding::new(i as u32, embedding)
        })
        .collect();

    let response = crate::types::EmbeddingsResponse::new(model, embeddings);

    // Convert to dict
    let result = Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("object", "list")?;

        let data_list = PyList::new(
            py,
            response.data.iter().map(|emb| {
                let emb_dict = PyDict::new(py);
                emb_dict.set_item("object", "embedding").unwrap();
                emb_dict.set_item("embedding", &emb.embedding).unwrap();
                emb_dict.set_item("index", emb.index).unwrap();
                emb_dict.to_object(py)
            }),
        );
        for (i, emb) in response.data.iter().enumerate() {
            let emb_dict = PyDict::new(py);
            emb_dict.set_item("object", "embedding")?;
            emb_dict.set_item("embedding", &emb.embedding)?;
            emb_dict.set_item("index", emb.index)?;
            data_list.set_item(i, emb_dict)?;
        }
        dict.set_item("data", data_list)?;
        dict.set_item("model", &response.model)?;

        let usage_dict = PyDict::new(py);
        usage_dict.set_item("prompt_tokens", 0)?;
        usage_dict.set_item("total_tokens", 0)?;
        dict.set_item("usage", usage_dict)?;

        Ok::<_, PyErr>(dict.into())
    })?;

    Ok(result)
}

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::error::AppError;

/// A knowledge item returned from a semantic search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryRecallResult {
    pub text: String,
    pub kb_type: String,
    pub score: f32,
    pub metadata: String,
}

/// Compute cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut norm_a, mut norm_b) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Fetch embeddings from the LLM API's /embeddings endpoint.
pub async fn fetch_embedding(
    api_endpoint: &str,
    api_key: &str,
    model: &str,
    text: &str,
) -> Result<Vec<f32>, AppError> {
    let url = format!("{}/embeddings", api_endpoint.trim_end_matches('/'));
    let body = json!({
        "model": model,
        "input": [text]
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", api_key),
        )
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| AppError::LlmService(format!("Embedding request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(AppError::LlmService(format!(
            "Embedding API error {}: {}",
            status, body_text
        )));
    }

    let resp_body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::LlmService(format!("Failed to parse embedding response: {}", e)))?;

    let embedding = resp_body["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| {
            AppError::LlmService("Embedding response missing 'data[0].embedding'".to_string())
        })?;

    let vec: Result<Vec<f32>, _> = embedding
        .iter()
        .map(|v| {
            v.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| AppError::LlmService("Invalid embedding value".to_string()))
        })
        .collect();

    vec
}

/// Semantic search over story knowledge items.
/// Returns top-k results sorted by cosine similarity.
pub fn semantic_search(
    items: &[crate::core::models::StoryKnowledgeItem],
    query_embedding: &[f32],
    top_k: usize,
) -> Vec<StoryRecallResult> {
    let mut scored: Vec<StoryRecallResult> = items
        .iter()
        .filter_map(|item| {
            let stored_embedding: Vec<f32> = serde_json::from_str(&item.embedding).ok()?;
            let score = cosine_similarity(query_embedding, &stored_embedding);
            Some(StoryRecallResult {
                text: item.text.clone(),
                kb_type: item.kb_type.clone(),
                score,
                metadata: item.metadata.clone(),
            })
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(top_k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty_returns_zero() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths_returns_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }
}

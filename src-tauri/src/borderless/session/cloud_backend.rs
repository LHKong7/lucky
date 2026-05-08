//! S3-backed session storage (optional, requires `s3` feature).

use async_trait::async_trait;

use super::store::{SessionData, SessionStore, SessionSummary};

/// S3-backed session store.
pub struct S3SessionStore {
    client: aws_sdk_s3::Client,
    bucket: String,
    prefix: String,
}

impl S3SessionStore {
    /// Create a new S3 session store.
    pub async fn new(bucket: impl Into<String>, prefix: impl Into<String>) -> Result<Self, String> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = aws_sdk_s3::Client::new(&config);

        Ok(Self {
            client,
            bucket: bucket.into(),
            prefix: prefix.into(),
        })
    }

    fn key(&self, session_id: &str) -> String {
        format!("{}{}.json", self.prefix, session_id)
    }
}

#[async_trait]
impl SessionStore for S3SessionStore {
    async fn get(&self, session_id: &str) -> Result<Option<SessionData>, String> {
        let key = self.key(session_id);
        let result = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match result {
            Ok(output) => {
                let body = output
                    .body
                    .collect()
                    .await
                    .map_err(|e| format!("Failed to read S3 body: {}", e))?;
                let data: SessionData = serde_json::from_slice(&body.into_bytes())
                    .map_err(|e| format!("Failed to parse session: {}", e))?;
                Ok(Some(data))
            }
            Err(e) => {
                let service_err = e.into_service_error();
                if service_err.is_no_such_key() {
                    Ok(None)
                } else {
                    Err(format!("S3 get error: {}", service_err))
                }
            }
        }
    }

    async fn put(&self, session_id: &str, data: &SessionData) -> Result<(), String> {
        let key = self.key(session_id);
        let body = serde_json::to_vec_pretty(data)
            .map_err(|e| format!("Failed to serialize session: {}", e))?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body.into())
            .content_type("application/json")
            .send()
            .await
            .map_err(|e| format!("S3 put error: {}", e))?;

        Ok(())
    }

    async fn list_ids(&self) -> Result<Vec<String>, String> {
        let result = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&self.prefix)
            .send()
            .await
            .map_err(|e| format!("S3 list error: {}", e))?;

        let ids = result
            .contents()
            .iter()
            .filter_map(|obj| {
                obj.key().and_then(|k| {
                    k.strip_prefix(&self.prefix)
                        .and_then(|s| s.strip_suffix(".json"))
                        .map(String::from)
                })
            })
            .collect();

        Ok(ids)
    }

    async fn list_summaries(&self, limit: usize) -> Result<Vec<SessionSummary>, String> {
        let ids = self.list_ids().await?;
        let mut summaries = Vec::new();

        for id in ids.iter().take(limit) {
            if let Ok(Some(data)) = self.get(id).await {
                summaries.push(SessionSummary {
                    id: data.id,
                    state: data.state,
                    message_count: data.history.len(),
                    created_at: data.created_at,
                    updated_at: data.updated_at,
                });
            }
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }
}

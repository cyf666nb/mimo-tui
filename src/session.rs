/// Session persistence — save/restore conversations.
/// Sessions stored as JSON in ~/.mimo-tui/sessions/
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::api::Message;

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub messages: Vec<Message>,
    pub model: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

impl Session {
    pub fn new(messages: Vec<Message>, model: String, input_tokens: u64, output_tokens: u64) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let title = messages.iter()
            .find(|m| m.role == "user")
            .and_then(|m| m.content.as_ref())
            .map(|c| {
                let preview: String = c.chars().take(60).collect();
                if c.len() > 60 { format!("{}...", preview) } else { preview }
            })
            .unwrap_or_else(|| "Untitled".into());

        Self {
            id,
            title,
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            messages,
            model,
            total_input_tokens: input_tokens,
            total_output_tokens: output_tokens,
        }
    }

    fn sessions_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_default().join(".mimo-tui").join("sessions")
    }

    pub fn save(&self) -> Result<PathBuf> {
        let dir = Self::sessions_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", self.id));
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    pub fn load(id: &str) -> Result<Self> {
        // Sanitize: only allow alphanumeric, hyphens, underscores
        if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(anyhow::anyhow!("Invalid session ID"));
        }
        let path = Self::sessions_dir().join(format!("{}.json", id));
        let content = std::fs::read_to_string(path)?;
        let session: Self = serde_json::from_str(&content)?;
        Ok(session)
    }

    pub fn list() -> Result<Vec<SessionSummary>> {
        let dir = Self::sessions_dir();
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(session) = serde_json::from_str::<Session>(&content) {
                        sessions.push(SessionSummary {
                            id: session.id,
                            title: session.title,
                            created_at: session.created_at,
                            model: session.model,
                            message_count: session.messages.len(),
                        });
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }

    #[allow(dead_code)]
    pub fn delete(id: &str) -> Result<()> {
        let path = Self::sessions_dir().join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub model: String,
    pub message_count: usize,
}

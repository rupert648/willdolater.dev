use blame_finder::TodoItem;
use leaderboard::SharedLeaderboard;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
use strum_macros::{Display, EnumString};
use tokio::sync::{Mutex, broadcast};

#[derive(Display, EnumString, Serialize, Clone, Debug, PartialEq)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Init,
    Clone,
    Scan,
    Error,
    Complete,
}

// Define a status update message structure for WebSocket communication
#[derive(Clone, Serialize, Debug)]
// TODO: FSM ??
pub struct StatusUpdate {
    // TODO: we can encode these into the stage enum
    pub message: String,
    pub stage: Stage, // e.g., "clone", "analysis", "complete", "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<u8>, // Optional progress percentage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<String>,
}

// Define a structure to store processing results
#[derive(Clone)]
pub struct ProcessingResult {
    pub todo_item: Option<TodoItem>,
    pub error: Option<String>,
    pub completed: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub numb_active_jobs: Arc<Mutex<u32>>,
    pub active_repo_paths: Arc<Mutex<HashSet<PathBuf>>>,
    pub leaderboard: SharedLeaderboard<TodoItem>,

    pub status_channels: Arc<Mutex<HashMap<String, broadcast::Sender<StatusUpdate>>>>,
    // Store results of processing for later retrieval by request ID
    pub results: Arc<Mutex<HashMap<String, ProcessingResult>>>,
    // Store past status updates for late-connecting clients
    pub status_history: Arc<Mutex<HashMap<String, Vec<StatusUpdate>>>>,
    // Optional: for cleanup of old results
    pub result_timestamps: Arc<Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>>>,
}

impl AppState {
    pub fn new(leaderboard: SharedLeaderboard<TodoItem>) -> Self {
        AppState {
            numb_active_jobs: Arc::new(Mutex::new(0)),
            active_repo_paths: Arc::new(Mutex::new(HashSet::new())),
            leaderboard,
            status_channels: Arc::new(Mutex::new(HashMap::new())),
            results: Arc::new(Mutex::new(HashMap::new())),
            status_history: Arc::new(Mutex::new(HashMap::new())),
            result_timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl AppState {
    pub async fn register_request(&self, request_id: &str) -> broadcast::Receiver<StatusUpdate> {
        let mut channels = self.status_channels.lock().await;
        let (tx, rx) = broadcast::channel(100); // Buffer size of 100 messages
        channels.insert(request_id.to_string(), tx);

        // Initialize results entry
        let mut results = self.results.lock().await;
        results.insert(
            request_id.to_string(),
            ProcessingResult {
                todo_item: None,
                error: None,
                completed: false,
            },
        );

        // Initialize status history
        let mut history = self.status_history.lock().await;
        history.insert(request_id.to_string(), Vec::new());

        // Set timestamp for this request
        let mut timestamps = self.result_timestamps.lock().await;
        timestamps.insert(request_id.to_string(), chrono::Utc::now());

        rx
    }

    pub async fn send_status(&self, request_id: &str, update: StatusUpdate) {
        // First, store the status update in history
        {
            let mut history = self.status_history.lock().await;
            if let Some(updates) = history.get_mut(request_id) {
                updates.push(update.clone());
            }
        }

        // Then try to broadcast to any connected clients
        let channels = self.status_channels.lock().await;
        dbg!(&update);
        if let Some(sender) = channels.get(request_id) {
            // Ignore send errors - this just means no receivers are listening
            let _ = sender
                .send(update)
                .inspect_err(|e| {
                    dbg!("Broadcasting error");
                    dbg!(e.to_string());
                })
                .inspect(|_| {
                    dbg!("Broadcasting success");
                });
        }
    }

    // Add a method to get past status updates
    pub async fn get_status_history(&self, request_id: &str) -> Vec<StatusUpdate> {
        let history = self.status_history.lock().await;
        history.get(request_id).cloned().unwrap_or_default()
    }

    pub async fn store_result(
        &self,
        request_id: &str,
        todo_item: Option<TodoItem>,
        error: Option<String>,
    ) {
        let mut results = self.results.lock().await;
        if let Some(result) = results.get_mut(request_id) {
            result.todo_item = todo_item;
            result.error = error;
            result.completed = true;
        }
    }

    pub async fn get_result(&self, request_id: &str) -> Option<ProcessingResult> {
        let results = self.results.lock().await;
        results.get(request_id).cloned()
    }

    pub async fn cleanup_old_requests(&self, max_age_hours: i64) {
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::hours(max_age_hours);

        let mut timestamps = self.result_timestamps.lock().await;
        let mut results = self.results.lock().await;
        let mut channels = self.status_channels.lock().await;
        let mut history = self.status_history.lock().await;

        // Identify old request IDs
        let old_ids: Vec<String> = timestamps
            .iter()
            .filter(|(_, timestamp)| **timestamp < cutoff)
            .map(|(id, _)| id.clone())
            .collect();

        // Clean up each old request
        for id in old_ids {
            timestamps.remove(&id);
            results.remove(&id);
            channels.remove(&id);
            history.remove(&id);
        }
    }
}

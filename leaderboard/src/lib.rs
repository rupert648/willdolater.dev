use serde::{Deserialize, Serialize};
use std::cmp::Ord;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

// Trait that defines all requirements for an item that can be stored in a leaderboard
pub trait Leaderboardable:
    Clone + Serialize + for<'de> Deserialize<'de> + Ord + Send + Sync + 'static
{
}

// Implement Leaderboardable for any type that satisfies all the required traits
impl<T> Leaderboardable for T where
    T: Clone + Serialize + for<'de> Deserialize<'de> + Ord + Send + Sync + 'static
{
}

#[derive(Error, Debug)]
pub enum LeaderboardError {
    #[error("Failed to read leaderboard file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse leaderboard data: {0}")]
    ParseError(#[from] serde_json::Error),
}

pub struct Leaderboard<T>
where
    T: Leaderboardable,
{
    items: BTreeSet<T>,
    max_items: usize,
    storage_path: String,
}

#[derive(Clone)]
pub struct SharedLeaderboard<T>
where
    T: Leaderboardable,
{
    inner: Arc<RwLock<Leaderboard<T>>>,
}

impl<T> SharedLeaderboard<T>
where
    T: Leaderboardable,
{
    pub async fn new(storage_path: String, max_items: usize) -> Result<Self, LeaderboardError> {
        let leaderboard = Leaderboard::new(storage_path, max_items)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(leaderboard)),
        })
    }

    pub async fn try_add(&self, item: T) -> bool {
        let mut leaderboard = self.inner.write().await;
        leaderboard.try_add(item)
    }

    pub async fn get_items(&self) -> Vec<T> {
        let leaderboard = self.inner.read().await;
        // Convert BTreeSet to Vec - items will already be sorted based on Ord implementation
        // For a leaderboard, we typically want highest scores first, so we reverse
        leaderboard.items.iter().cloned().rev().collect()
    }

    // For convenience when you want to clone the shared instance
    pub fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Leaderboard<T>
where
    T: Leaderboardable,
{
    pub fn new(storage_path: String, max_items: usize) -> Result<Self, LeaderboardError> {
        let items_vec = if Path::new(&storage_path).exists() {
            let file_content = fs::read_to_string(&storage_path)?;
            serde_json::from_str::<Vec<T>>(&file_content)?
        } else {
            Vec::new()
        };

        // Convert Vec to BTreeSet
        let items: BTreeSet<T> = items_vec.into_iter().collect();

        Ok(Self {
            items,
            max_items,
            storage_path,
        })
    }

    pub fn try_add(&mut self, item: T) -> bool {
        // If we already have this exact item, return false
        if self.items.contains(&item) {
            return false;
        }

        // If we have space, just add it
        if self.items.len() < self.max_items {
            self.items.insert(item);
            self.save().unwrap_or_else(|e| {
                eprintln!("Failed to save leaderboard: {}", e);
            });
            return true;
        }

        // Otherwise, we need to check if this item is better than the worst item
        // Since BTreeSet is ordered, the first item is the lowest/worst
        if let Some(worst_item) = self.items.iter().next().cloned() {
            if &item > &worst_item {
                // Remove the worst item
                self.items.remove(&worst_item);
                // Add the new item
                self.items.insert(item);

                self.save().unwrap_or_else(|e| {
                    eprintln!("Failed to save leaderboard: {}", e);
                });
                return true;
            }
        }

        false
    }

    fn save(&self) -> Result<(), LeaderboardError> {
        // Convert BTreeSet to Vec for serialization
        let items_vec: Vec<T> = self.items.iter().cloned().collect();
        let json = serde_json::to_string_pretty(&items_vec)?;
        fs::write(&self.storage_path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    struct TestScore {
        // Put score first so that Ord implementation sorts by score
        score: u32,
        name: String,
    }

    impl TestScore {
        fn new(name: &str, score: u32) -> Self {
            Self {
                name: name.to_string(),
                score,
            }
        }
    }

    #[test]
    fn test_leaderboard_new_empty() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("test_leaderboard.json")
            .to_str()
            .unwrap()
            .to_string();

        let leaderboard = Leaderboard::<TestScore>::new(path, 5).unwrap();

        assert_eq!(leaderboard.items.len(), 0);
        assert_eq!(leaderboard.max_items, 5);
    }

    #[test]
    fn test_leaderboard_add_item() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("test_leaderboard.json")
            .to_str()
            .unwrap()
            .to_string();

        let mut leaderboard = Leaderboard::<TestScore>::new(path, 5).unwrap();

        let item = TestScore::new("Test", 100);
        let added = leaderboard.try_add(item.clone());

        assert!(added);
        assert_eq!(leaderboard.items.len(), 1);
        assert!(leaderboard.items.contains(&item));
    }

    #[test]
    fn test_leaderboard_add_duplicate() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("test_leaderboard.json")
            .to_str()
            .unwrap()
            .to_string();

        let mut leaderboard = Leaderboard::<TestScore>::new(path, 5).unwrap();

        let item = TestScore::new("Test", 100);

        // Add the first time
        let added = leaderboard.try_add(item.clone());
        assert!(added);

        // Try to add the same item again
        let added_again = leaderboard.try_add(item.clone());
        assert!(!added_again);

        // Verify we still only have one item
        assert_eq!(leaderboard.items.len(), 1);
    }

    #[test]
    fn test_leaderboard_add_max_items() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("test_leaderboard.json")
            .to_str()
            .unwrap()
            .to_string();

        let mut leaderboard = Leaderboard::<TestScore>::new(path, 3).unwrap();

        // Add 3 items (max capacity)
        leaderboard.try_add(TestScore::new("Alice", 60));
        leaderboard.try_add(TestScore::new("Bob", 80));
        leaderboard.try_add(TestScore::new("Charlie", 100));

        assert_eq!(leaderboard.items.len(), 3);

        // Try to add a worse score (should fail)
        let added = leaderboard.try_add(TestScore::new("Dave", 40));
        assert!(!added);
        assert_eq!(leaderboard.items.len(), 3);

        // Try to add a better score (should succeed, replacing the lowest score)
        let added = leaderboard.try_add(TestScore::new("Eve", 120));
        assert!(added);
        assert_eq!(leaderboard.items.len(), 3);

        // Get items and verify (lowest score was removed)
        let items: Vec<TestScore> = leaderboard.items.iter().cloned().collect();
        let has_alice = items.iter().any(|s| s.name == "Alice");
        let has_bob = items.iter().any(|s| s.name == "Bob");
        let has_charlie = items.iter().any(|s| s.name == "Charlie");
        let has_eve = items.iter().any(|s| s.name == "Eve");

        assert!(!has_alice); // Alice (60) should be removed
        assert!(has_bob); // Bob (80) should remain
        assert!(has_charlie); // Charlie (100) should remain
        assert!(has_eve); // Eve (120) should be added
    }

    #[test]
    fn test_leaderboard_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("test_leaderboard.json")
            .to_str()
            .unwrap()
            .to_string();

        // Create and populate leaderboard
        {
            let mut leaderboard = Leaderboard::<TestScore>::new(path.clone(), 3).unwrap();
            leaderboard.try_add(TestScore::new("Alice", 100));
            leaderboard.try_add(TestScore::new("Bob", 80));
            // This implicitly calls save()
        }

        // Load the leaderboard from disk
        let loaded_leaderboard = Leaderboard::<TestScore>::new(path.clone(), 3).unwrap();

        assert_eq!(loaded_leaderboard.items.len(), 2);
        assert!(
            loaded_leaderboard
                .items
                .contains(&TestScore::new("Alice", 100))
        );
        assert!(
            loaded_leaderboard
                .items
                .contains(&TestScore::new("Bob", 80))
        );
    }

    #[tokio::test]
    async fn test_shared_leaderboard() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("test_shared_leaderboard.json")
            .to_str()
            .unwrap()
            .to_string();

        let shared_leaderboard = SharedLeaderboard::<TestScore>::new(path, 3).await.unwrap();

        // Add items
        shared_leaderboard
            .try_add(TestScore::new("Alice", 100))
            .await;
        shared_leaderboard.try_add(TestScore::new("Bob", 80)).await;

        // Get items and verify
        let items = shared_leaderboard.get_items().await;
        assert_eq!(items.len(), 2);

        // Since get_items returns a reversed vector (highest first)
        assert_eq!(items[0].name, "Alice");
        assert_eq!(items[1].name, "Bob");

        // Create a clone of the shared leaderboard
        let shared_leaderboard_clone = shared_leaderboard.clone();

        // Add an item via the clone
        shared_leaderboard_clone
            .try_add(TestScore::new("Charlie", 120))
            .await;

        // Verify the item is visible from the original instance
        let updated_items = shared_leaderboard.get_items().await;
        assert_eq!(updated_items.len(), 3);
        assert_eq!(updated_items[0].name, "Charlie");
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let dir = tempdir().unwrap();
        let path = dir
            .path()
            .join("test_concurrent.json")
            .to_str()
            .unwrap()
            .to_string();

        let shared_leaderboard = SharedLeaderboard::<TestScore>::new(path, 10).await.unwrap();

        // Spawn 10 tasks that each add a unique item
        let mut handles = Vec::new();

        for i in 0..10 {
            let leaderboard_clone = shared_leaderboard.clone();
            let handle = tokio::spawn(async move {
                leaderboard_clone
                    .try_add(TestScore::new(&format!("Player_{}", i), i * 10))
                    .await
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let _ = handle.await.unwrap();
        }

        // Verify all items were added
        let items = shared_leaderboard.get_items().await;
        assert_eq!(items.len(), 10);

        // Verify they're sorted correctly (highest first)
        for i in 0..9 {
            assert!(items[i].score > items[i + 1].score);
        }
    }
}

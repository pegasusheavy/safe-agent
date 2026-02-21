use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::{info, warn};

/// Two-factor authentication manager for dangerous operations.
///
/// When a tool in the `require_2fa` list is about to execute, instead of
/// executing directly, a challenge is created. The user must confirm via
/// a second channel (dashboard confirmation, Telegram reply, etc.) within
/// a time window.
pub struct TwoFactorManager {
    /// Tools that require 2FA.
    required_tools: HashSet<String>,
    /// Pending challenges: challenge_id -> Challenge.
    challenges: Mutex<HashMap<String, Challenge>>,
    /// How long a challenge is valid.
    challenge_ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct Challenge {
    /// Unique challenge ID.
    pub id: String,
    /// The tool that requires 2FA.
    pub tool: String,
    /// Parameters of the proposed action.
    pub params: serde_json::Value,
    /// Human-readable description of the action.
    pub description: String,
    /// When the challenge was created.
    pub created_at: Instant,
    /// Whether the challenge has been confirmed.
    pub confirmed: bool,
    /// Source that created the challenge (agent, cron, goal, etc.).
    pub source: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChallengeInfo {
    pub id: String,
    pub tool: String,
    pub description: String,
    pub source: String,
    pub age_secs: u64,
    pub confirmed: bool,
}

/// Result of a 2FA check.
#[derive(Debug)]
pub enum TwoFactorVerdict {
    /// Tool does not require 2FA — proceed normally.
    NotRequired,
    /// A 2FA challenge was created — must be confirmed before execution.
    ChallengeCreated(String),
    /// A valid, confirmed challenge exists — proceed with execution.
    Confirmed,
}

impl TwoFactorManager {
    pub fn new(required_tools: Vec<String>) -> Self {
        Self {
            required_tools: required_tools.into_iter().collect(),
            challenges: Mutex::new(HashMap::new()),
            challenge_ttl: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Check whether a tool requires 2FA. If so, create a challenge.
    ///
    /// Returns the verdict:
    /// - `NotRequired`: tool doesn't need 2FA
    /// - `ChallengeCreated(id)`: challenge created, needs confirmation
    /// - `Confirmed`: a confirmed challenge exists
    pub fn check(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
        description: &str,
        source: &str,
    ) -> TwoFactorVerdict {
        if !self.required_tools.contains(tool_name) {
            return TwoFactorVerdict::NotRequired;
        }

        let mut challenges = self.challenges.lock().unwrap();

        // Prune expired challenges
        let now = Instant::now();
        challenges.retain(|_, c| now.duration_since(c.created_at) < self.challenge_ttl);

        // Check if there's a confirmed challenge for this exact tool+params
        for (id, challenge) in challenges.iter() {
            if challenge.tool == tool_name
                && challenge.params == *params
                && challenge.confirmed
            {
                info!(tool = %tool_name, challenge_id = %id, "2FA challenge confirmed — proceeding");
                let id = id.clone();
                challenges.remove(&id);
                return TwoFactorVerdict::Confirmed;
            }
        }

        // Check if there's an existing unconfirmed challenge for this tool
        for (id, challenge) in challenges.iter() {
            if challenge.tool == tool_name && challenge.params == *params && !challenge.confirmed {
                return TwoFactorVerdict::ChallengeCreated(id.clone());
            }
        }

        // Create a new challenge
        let id = uuid::Uuid::new_v4().to_string();
        let challenge = Challenge {
            id: id.clone(),
            tool: tool_name.to_string(),
            params: params.clone(),
            description: description.to_string(),
            created_at: now,
            confirmed: false,
            source: source.to_string(),
        };

        warn!(
            tool = %tool_name,
            challenge_id = %id,
            "2FA challenge created for dangerous operation"
        );

        challenges.insert(id.clone(), challenge);
        TwoFactorVerdict::ChallengeCreated(id)
    }

    /// Confirm a pending challenge.
    pub fn confirm(&self, challenge_id: &str) -> bool {
        let mut challenges = self.challenges.lock().unwrap();
        if let Some(challenge) = challenges.get_mut(challenge_id) {
            if !challenge.confirmed {
                challenge.confirmed = true;
                info!(challenge_id, tool = %challenge.tool, "2FA challenge confirmed");
                return true;
            }
        }
        warn!(challenge_id, "2FA challenge not found or already confirmed");
        false
    }

    /// Reject and remove a pending challenge.
    pub fn reject(&self, challenge_id: &str) -> bool {
        let mut challenges = self.challenges.lock().unwrap();
        if challenges.remove(challenge_id).is_some() {
            info!(challenge_id, "2FA challenge rejected");
            true
        } else {
            false
        }
    }

    /// List all pending (unconfirmed) challenges.
    pub fn pending(&self) -> Vec<ChallengeInfo> {
        let challenges = self.challenges.lock().unwrap();
        let now = Instant::now();
        challenges
            .values()
            .filter(|c| !c.confirmed && now.duration_since(c.created_at) < self.challenge_ttl)
            .map(|c| ChallengeInfo {
                id: c.id.clone(),
                tool: c.tool.clone(),
                description: c.description.clone(),
                source: c.source.clone(),
                age_secs: now.duration_since(c.created_at).as_secs(),
                confirmed: c.confirmed,
            })
            .collect()
    }

    /// Check if a tool requires 2FA.
    pub fn requires_2fa(&self, tool_name: &str) -> bool {
        self.required_tools.contains(tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_required() {
        let mgr = TwoFactorManager::new(vec!["exec".to_string()]);
        match mgr.check("web_search", &serde_json::json!({}), "search", "agent") {
            TwoFactorVerdict::NotRequired => {}
            _ => panic!("expected NotRequired"),
        }
    }

    #[test]
    fn test_challenge_created() {
        let mgr = TwoFactorManager::new(vec!["exec".to_string()]);
        match mgr.check("exec", &serde_json::json!({"command": "rm -rf /"}), "delete all", "agent") {
            TwoFactorVerdict::ChallengeCreated(id) => {
                assert!(!id.is_empty());
            }
            _ => panic!("expected ChallengeCreated"),
        }
    }

    #[test]
    fn test_confirm_and_proceed() {
        let mgr = TwoFactorManager::new(vec!["exec".to_string()]);
        let params = serde_json::json!({"command": "rm -rf /"});

        // Create challenge
        let id = match mgr.check("exec", &params, "delete", "agent") {
            TwoFactorVerdict::ChallengeCreated(id) => id,
            _ => panic!("expected ChallengeCreated"),
        };

        // Confirm
        assert!(mgr.confirm(&id));

        // Now check again — should be Confirmed
        match mgr.check("exec", &params, "delete", "agent") {
            TwoFactorVerdict::Confirmed => {}
            _ => panic!("expected Confirmed"),
        }
    }

    #[test]
    fn test_reject() {
        let mgr = TwoFactorManager::new(vec!["exec".to_string()]);
        let id = match mgr.check("exec", &serde_json::json!({}), "test", "agent") {
            TwoFactorVerdict::ChallengeCreated(id) => id,
            _ => panic!("expected ChallengeCreated"),
        };

        assert!(mgr.reject(&id));
        assert!(!mgr.reject(&id)); // already removed
    }

    #[test]
    fn test_pending() {
        let mgr = TwoFactorManager::new(vec!["exec".to_string()]);
        mgr.check("exec", &serde_json::json!({"a": 1}), "test1", "agent");
        mgr.check("exec", &serde_json::json!({"b": 2}), "test2", "cron");

        let pending = mgr.pending();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_requires_2fa() {
        let mgr = TwoFactorManager::new(vec!["exec".to_string(), "delete_file".to_string()]);
        assert!(mgr.requires_2fa("exec"));
        assert!(mgr.requires_2fa("delete_file"));
        assert!(!mgr.requires_2fa("web_search"));
    }

    #[test]
    fn test_duplicate_challenge_reuses_existing() {
        let mgr = TwoFactorManager::new(vec!["exec".to_string()]);
        let params = serde_json::json!({"command": "dangerous"});

        let id1 = match mgr.check("exec", &params, "test", "agent") {
            TwoFactorVerdict::ChallengeCreated(id) => id,
            _ => panic!("expected ChallengeCreated"),
        };

        let id2 = match mgr.check("exec", &params, "test", "agent") {
            TwoFactorVerdict::ChallengeCreated(id) => id,
            _ => panic!("expected ChallengeCreated"),
        };

        assert_eq!(id1, id2);
    }
}

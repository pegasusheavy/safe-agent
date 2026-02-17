use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: String,
    pub action: serde_json::Value,
    pub reasoning: String,
    pub context: String,
    pub status: ApprovalStatus,
    pub proposed_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Executed,
    Failed,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Executed => "executed",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_as_str() {
        assert_eq!(ApprovalStatus::Pending.as_str(), "pending");
        assert_eq!(ApprovalStatus::Approved.as_str(), "approved");
        assert_eq!(ApprovalStatus::Rejected.as_str(), "rejected");
        assert_eq!(ApprovalStatus::Expired.as_str(), "expired");
        assert_eq!(ApprovalStatus::Executed.as_str(), "executed");
        assert_eq!(ApprovalStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn status_display() {
        assert_eq!(format!("{}", ApprovalStatus::Pending), "pending");
        assert_eq!(format!("{}", ApprovalStatus::Executed), "executed");
    }

    #[test]
    fn status_equality() {
        assert_eq!(ApprovalStatus::Pending, ApprovalStatus::Pending);
        assert_ne!(ApprovalStatus::Pending, ApprovalStatus::Approved);
    }

    #[test]
    fn status_clone() {
        let s = ApprovalStatus::Approved;
        let s2 = s.clone();
        assert_eq!(s, s2);
    }

    #[test]
    fn status_debug() {
        let dbg = format!("{:?}", ApprovalStatus::Failed);
        assert_eq!(dbg, "Failed");
    }

    #[test]
    fn status_serde_roundtrip() {
        let statuses = vec![
            ApprovalStatus::Pending,
            ApprovalStatus::Approved,
            ApprovalStatus::Rejected,
            ApprovalStatus::Expired,
            ApprovalStatus::Executed,
            ApprovalStatus::Failed,
        ];
        for s in statuses {
            let json = serde_json::to_string(&s).unwrap();
            let deser: ApprovalStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, deser);
        }
    }

    #[test]
    fn pending_action_serde_roundtrip() {
        let action = PendingAction {
            id: "test-123".into(),
            action: serde_json::json!({"tool": "exec", "params": {}}),
            reasoning: "because".into(),
            context: "user asked".into(),
            status: ApprovalStatus::Pending,
            proposed_at: "2026-01-01T00:00:00Z".into(),
            resolved_at: None,
        };
        let json = serde_json::to_string(&action).unwrap();
        let deser: PendingAction = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.id, "test-123");
        assert_eq!(deser.status, ApprovalStatus::Pending);
        assert!(deser.resolved_at.is_none());
    }

    #[test]
    fn pending_action_with_resolved_at() {
        let action = PendingAction {
            id: "a".into(),
            action: serde_json::Value::Null,
            reasoning: String::new(),
            context: String::new(),
            status: ApprovalStatus::Executed,
            proposed_at: "2026-01-01".into(),
            resolved_at: Some("2026-01-02".into()),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("2026-01-02"));
    }
}

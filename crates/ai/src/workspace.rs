use std::path::PathBuf;

use chrono::{DateTime, Days, Utc};

/// Public-facing metadata persisted in SQLite
#[derive(Debug, Default, Clone)]
pub struct WorkspaceMetadata {
    pub path: PathBuf,
    pub navigated_ts: Option<DateTime<Utc>>,
    pub modified_ts: Option<DateTime<Utc>>,
    pub queried_ts: Option<DateTime<Utc>>,
    /// Wall-clock time the index was last successfully synced (post-flush).
    pub synced_at: Option<DateTime<Utc>>,
    /// Total bytes of fragment payload from the most recent successful flush.
    pub index_bytes: Option<u64>,
    /// Number of indexed source files (Merkle leaf nodes) at last flush.
    pub file_count: Option<u32>,
    /// Number of embedded fragments produced at last flush.
    pub fragment_count: Option<u32>,
    /// Cumulative count of `Queried` events for this workspace.
    pub query_count: u32,
}

impl WorkspaceMetadata {
    /// Surface most recently navigated first
    pub fn most_recently_navigated(a: &Self, b: &Self) -> std::cmp::Ordering {
        match (a.navigated_ts, b.navigated_ts) {
            (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.path.cmp(&b.path),
        }
    }

    /// Surface most recently touched first
    pub fn most_recently_touched(a: &Self, b: &Self) -> std::cmp::Ordering {
        match (a.last_touched(), b.last_touched()) {
            (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.path.cmp(&b.path),
        }
    }

    /// The most recent time this codebase index was navigated to, queried or modified.
    pub fn last_touched(&self) -> Option<DateTime<Utc>> {
        let mut last_access_time: Option<DateTime<Utc>> = None;
        if let Some(nav_ts) = self.navigated_ts {
            last_access_time = last_access_time
                .map(|old_time| old_time.max(nav_ts))
                .or(Some(nav_ts));
        }
        if let Some(mod_ts) = self.modified_ts {
            last_access_time = last_access_time
                .map(|old_time| old_time.max(mod_ts))
                .or(Some(mod_ts));
        }
        if let Some(query_ts) = self.queried_ts {
            last_access_time = last_access_time
                .map(|old_time| old_time.max(query_ts))
                .or(Some(query_ts));
        }
        last_access_time
    }

    pub fn is_expired(&self, current_time: DateTime<Utc>, shelf_life_days: u64) -> bool {
        let Some(last_touch) = self.last_touched() else {
            return true;
        };
        last_touch
            .checked_add_days(Days::new(shelf_life_days))
            .unwrap_or_default()
            < current_time
    }
}

/// An event to update the workspace metadata.
#[derive(Debug, Clone, Copy)]
pub enum WorkspaceMetadataEvent {
    Queried,
    Modified,
    Created,
    /// Emitted after a successful sync flush. Carries the freshly-counted
    /// stats so the persistence layer can surface them in settings UI.
    Flushed {
        index_bytes: u64,
        file_count: u32,
        fragment_count: u32,
    },
}

impl From<WorkspaceMetadata> for persistence::model::NewWorkspaceMetadata {
    fn from(value: WorkspaceMetadata) -> Self {
        Self {
            repo_path: value.path.to_string_lossy().into_owned(),
            navigated_ts: value.navigated_ts.map(|utc_dt| utc_dt.naive_utc()),
            modified_ts: value.modified_ts.map(|utc_dt| utc_dt.naive_utc()),
            queried_ts: value.queried_ts.map(|utc_dt| utc_dt.naive_utc()),
            synced_at: value.synced_at.map(|utc_dt| utc_dt.naive_utc()),
            index_bytes: value.index_bytes.map(|b| b as i64),
            file_count: value.file_count.map(|c| c as i32),
            fragment_count: value.fragment_count.map(|c| c as i32),
            query_count: value.query_count as i32,
        }
    }
}

impl From<persistence::model::WorkspaceMetadata> for WorkspaceMetadata {
    fn from(value: persistence::model::WorkspaceMetadata) -> Self {
        Self {
            path: PathBuf::from(value.repo_path),
            navigated_ts: value.navigated_ts.map(|naive_ts| naive_ts.and_utc()),
            modified_ts: value.modified_ts.map(|naive_ts| naive_ts.and_utc()),
            queried_ts: value.queried_ts.map(|naive_ts| naive_ts.and_utc()),
            synced_at: value.synced_at.map(|naive_ts| naive_ts.and_utc()),
            index_bytes: value.index_bytes.map(|b| b.max(0) as u64),
            file_count: value.file_count.map(|c| c.max(0) as u32),
            fragment_count: value.fragment_count.map(|c| c.max(0) as u32),
            query_count: value.query_count.max(0) as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    /// Round-trip every persisted field through `WorkspaceMetadata` →
    /// `NewWorkspaceMetadata` → `persistence::model::WorkspaceMetadata` →
    /// `WorkspaceMetadata`. Any future field added to one direction but not
    /// the other will fail this test.
    #[test]
    fn workspace_metadata_persistence_roundtrip_preserves_all_fields() {
        let ts = Utc.with_ymd_and_hms(2026, 5, 6, 12, 30, 45).unwrap();
        let original = WorkspaceMetadata {
            path: PathBuf::from("/tmp/roundtrip-repo"),
            navigated_ts: Some(ts),
            modified_ts: Some(ts + chrono::Duration::seconds(10)),
            queried_ts: Some(ts + chrono::Duration::seconds(20)),
            synced_at: Some(ts + chrono::Duration::seconds(30)),
            index_bytes: Some(7 * 1024 * 1024),
            file_count: Some(2_500),
            fragment_count: Some(18_900),
            query_count: 42,
        };

        // Forward: ai → persistence::Insertable
        let new_row: persistence::model::NewWorkspaceMetadata = original.clone().into();
        // Simulate SQLite assigning a primary key on insert by promoting to the
        // Queryable model while preserving every other column verbatim.
        let queried = persistence::model::WorkspaceMetadata {
            id: 1,
            repo_path: new_row.repo_path,
            navigated_ts: new_row.navigated_ts,
            modified_ts: new_row.modified_ts,
            queried_ts: new_row.queried_ts,
            synced_at: new_row.synced_at,
            index_bytes: new_row.index_bytes,
            file_count: new_row.file_count,
            fragment_count: new_row.fragment_count,
            query_count: new_row.query_count,
        };

        // Reverse: persistence::Queryable → ai
        let restored: WorkspaceMetadata = queried.into();

        assert_eq!(restored.path, original.path);
        assert_eq!(restored.navigated_ts, original.navigated_ts);
        assert_eq!(restored.modified_ts, original.modified_ts);
        assert_eq!(restored.queried_ts, original.queried_ts);
        assert_eq!(restored.synced_at, original.synced_at);
        assert_eq!(restored.index_bytes, original.index_bytes);
        assert_eq!(restored.file_count, original.file_count);
        assert_eq!(restored.fragment_count, original.fragment_count);
        assert_eq!(restored.query_count, original.query_count);
    }

    #[test]
    fn workspace_metadata_roundtrip_preserves_none_options() {
        let original = WorkspaceMetadata {
            path: PathBuf::from("/tmp/empty-repo"),
            ..Default::default()
        };
        let new_row: persistence::model::NewWorkspaceMetadata = original.clone().into();
        let queried = persistence::model::WorkspaceMetadata {
            id: 2,
            repo_path: new_row.repo_path,
            navigated_ts: new_row.navigated_ts,
            modified_ts: new_row.modified_ts,
            queried_ts: new_row.queried_ts,
            synced_at: new_row.synced_at,
            index_bytes: new_row.index_bytes,
            file_count: new_row.file_count,
            fragment_count: new_row.fragment_count,
            query_count: new_row.query_count,
        };
        let restored: WorkspaceMetadata = queried.into();
        assert!(restored.navigated_ts.is_none());
        assert!(restored.modified_ts.is_none());
        assert!(restored.queried_ts.is_none());
        assert!(restored.synced_at.is_none());
        assert!(restored.index_bytes.is_none());
        assert!(restored.file_count.is_none());
        assert!(restored.fragment_count.is_none());
        assert_eq!(restored.query_count, 0);
    }

    /// Negative i32/i64 values from SQLite (corrupted rows) should clamp to 0
    /// rather than wrap into huge unsigned numbers.
    #[test]
    fn workspace_metadata_negative_counters_clamp_to_zero() {
        let queried = persistence::model::WorkspaceMetadata {
            id: 3,
            repo_path: "/tmp/neg".to_string(),
            navigated_ts: None,
            modified_ts: None,
            queried_ts: None,
            synced_at: None,
            index_bytes: Some(-100),
            file_count: Some(-5),
            fragment_count: Some(-1),
            query_count: -7,
        };
        let restored: WorkspaceMetadata = queried.into();
        assert_eq!(restored.index_bytes, Some(0));
        assert_eq!(restored.file_count, Some(0));
        assert_eq!(restored.fragment_count, Some(0));
        assert_eq!(restored.query_count, 0);
    }
}

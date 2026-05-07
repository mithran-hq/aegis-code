use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use chrono::DateTime;
use chrono::Utc;
use codex_protocol::ThreadId;
use codex_protocol::method_state::METHOD_STATE_SCHEMA_VERSION;
use codex_protocol::method_state::MethodState;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use super::epoch_millis_to_datetime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMethodStateRecord {
    pub thread_id: ThreadId,
    pub schema_version: u32,
    pub state: MethodState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub(crate) struct ThreadMethodStateRow {
    pub thread_id: String,
    pub schema_version: i64,
    pub state_json: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl ThreadMethodStateRow {
    pub(crate) fn try_from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Self {
            thread_id: row.try_get("thread_id")?,
            schema_version: row.try_get("schema_version")?,
            state_json: row.try_get("state_json")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
}

impl TryFrom<ThreadMethodStateRow> for ThreadMethodStateRecord {
    type Error = anyhow::Error;

    fn try_from(row: ThreadMethodStateRow) -> Result<Self> {
        let thread_id = ThreadId::from_string(&row.thread_id)
            .with_context(|| format!("invalid method state thread id {}", row.thread_id))?;
        let schema_version = u32::try_from(row.schema_version).with_context(|| {
            format!(
                "invalid method state schema version for thread {thread_id}: {}",
                row.schema_version
            )
        })?;
        if schema_version != METHOD_STATE_SCHEMA_VERSION {
            return Err(anyhow!(
                "incompatible method state schema version for thread {thread_id}: persisted {schema_version}, expected {METHOD_STATE_SCHEMA_VERSION}"
            ));
        }

        let state: MethodState = serde_json::from_str(&row.state_json)
            .with_context(|| format!("corrupt method state JSON for thread {thread_id}"))?;
        if state.schema_version != schema_version {
            return Err(anyhow!(
                "method state schema version mismatch for thread {thread_id}: row {schema_version}, payload {}",
                state.schema_version
            ));
        }

        Ok(Self {
            thread_id,
            schema_version,
            state,
            created_at: epoch_millis_to_datetime(row.created_at_ms)?,
            updated_at: epoch_millis_to_datetime(row.updated_at_ms)?,
        })
    }
}

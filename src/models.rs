#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
pub enum ConfessionStatus {
    Pending,
    Approved,
    Rejected,
    Posted,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Confession {
    pub id: i64,
    pub message_id: Option<i64>,
    pub reviewer_id: Option<i64>,
    pub content: String,
    pub status: ConfessionStatus,
    pub rejected_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

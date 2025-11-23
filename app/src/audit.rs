// ==============================================================================
// audit.rs - Audit Logging for Genetic Data Operations
// ==============================================================================
// Description: Comprehensive audit trail for all genetic data operations
// Author: Matt Barham
// Created: 2025-10-31
// Modified: 2025-10-31
// Version: 1.0.0
// Compliance: HIPAA § 164.312(b), GDPR Article 30
// ==============================================================================

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    // Authentication events
    AuthSuccess,
    AuthFailure,
    TwoFactorSuccess,
    TwoFactorFailure,
    SessionCreated,
    SessionExpired,
    SessionTerminated,

    // File operations
    FileUploaded,
    FileValidated,
    FileRejected,
    FileDownloaded,
    FileDeleted,

    // Processing events
    JobCreated,
    JobStarted,
    JobCompleted,
    JobFailed,

    // Security events
    AccessDenied,
    RateLimitExceeded,
    MalwareDetected,
    InvalidInput,
    UnusualActivity,

    // Administrative events
    UserCreated,
    UserDeleted,
    PermissionChanged,
    ConfigurationChanged,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum LogSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub resource: Option<String>,
    pub action: String,
    pub result: String,
    pub details: serde_json::Value,
    pub severity: LogSeverity,
}

impl AuditEvent {
    pub fn new(
        event_type: AuditEventType,
        user_id: Option<String>,
        resource: Option<String>,
        details: serde_json::Value,
    ) -> Self {
        let severity = match event_type {
            AuditEventType::AuthFailure
            | AuditEventType::AccessDenied
            | AuditEventType::MalwareDetected => LogSeverity::Warning,

            AuditEventType::JobFailed
            | AuditEventType::InvalidInput => LogSeverity::Error,

            AuditEventType::UnusualActivity
            | AuditEventType::FileRejected => LogSeverity::Critical,

            _ => LogSeverity::Info,
        };

        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            user_id,
            session_id: None,
            ip_address: None,
            user_agent: None,
            resource,
            action: String::new(),
            result: String::from("success"),
            details,
            severity,
        }
    }

    pub async fn log(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        // Note: Using sqlx::query instead of query! macro to avoid compile-time
        // database checking during development. Switch to query! later for
        // compile-time SQL validation.
        sqlx::query(
            r#"
            INSERT INTO genetics_audit (
                id, timestamp, event_type, user_id, session_id,
                ip_address, user_agent, resource, action, result,
                details, severity
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(self.id)
        .bind(self.timestamp)
        .bind(serde_json::to_string(&self.event_type).unwrap())
        .bind(&self.user_id)
        .bind(&self.session_id)
        .bind(&self.ip_address)
        .bind(&self.user_agent)
        .bind(&self.resource)
        .bind(&self.action)
        .bind(&self.result)
        .bind(&self.details)
        .bind(serde_json::to_string(&self.severity).unwrap())
        .execute(pool)
        .await?;

        Ok(())
    }
}

/// Convenience function to log an audit event
pub async fn log_event(
    pool: &PgPool,
    event_type: AuditEventType,
    user_id: &str,
    resource: Option<String>,
    details: serde_json::Value,
) -> Result<(), sqlx::Error> {
    let event = AuditEvent::new(
        event_type,
        Some(user_id.to_string()),
        resource,
        details,
    );

    event.log(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_creation() {
        let event = AuditEvent::new(
            AuditEventType::FileUploaded,
            Some("user123".to_string()),
            Some("genome.txt".to_string()),
            serde_json::json!({
                "size": 5242880,
                "hash": "abc123"
            }),
        );

        assert_eq!(event.user_id, Some("user123".to_string()));
        assert_eq!(event.resource, Some("genome.txt".to_string()));
        assert!(matches!(event.severity, LogSeverity::Info));
    }

    #[test]
    fn test_security_event_severity() {
        let event = AuditEvent::new(
            AuditEventType::MalwareDetected,
            Some("user123".to_string()),
            None,
            serde_json::json!({}),
        );

        assert!(matches!(event.severity, LogSeverity::Warning));
    }
}

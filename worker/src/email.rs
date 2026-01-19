// ==============================================================================
// email.rs - Email Notification System
// ==============================================================================
// Description: Send email notifications for completed genetics processing jobs
// Author: Matt Barham
// Created: 2025-11-18
// Modified: 2025-11-18
// Version: 1.0.0
// Phase: Phase 5 - Email Sending
// ==============================================================================

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use lettre::{
    message::{header::ContentType, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{error, info, warn};
use uuid::Uuid;

// ==============================================================================
// TEMPLATE RENDERING
// ==============================================================================

/// Replace template variables in a string
///
/// Variables are in the format {{variable_name}} and are replaced with values
/// from the provided HashMap.
///
/// # Arguments
///
/// * `template` - The template string containing {{variables}}
/// * `variables` - HashMap of variable names to replacement values
///
/// # Returns
///
/// The template string with all variables replaced
fn render_template(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }

    result
}

/// Format a DateTime for display in emails
fn format_datetime(dt: &DateTime<Utc>) -> String {
    dt.format("%B %d, %Y at %I:%M %p UTC").to_string()
}

// ==============================================================================
// EMAIL CONFIGURATION
// ==============================================================================

/// Email configuration from environment variables
pub struct EmailConfig {
    smtp_host: String,
    smtp_port: u16,
    smtp_username: String,
    smtp_password: String,
    smtp_use_tls: bool,
    smtp_use_ssl: bool,
    from_email: String,
    from_name: String,
    download_base_url: String,
    template_dir: String,
}

impl EmailConfig {
    /// Load email configuration from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            smtp_host: std::env::var("SMTP_HOST")
                .context("SMTP_HOST not set")?,
            smtp_port: std::env::var("SMTP_PORT")
                .context("SMTP_PORT not set")?
                .parse()
                .context("SMTP_PORT must be a valid port number")?,
            smtp_username: std::env::var("SMTP_USERNAME")
                .context("SMTP_USERNAME not set")?,
            smtp_password: Self::read_smtp_password()?,
            smtp_use_tls: std::env::var("SMTP_USE_TLS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            smtp_use_ssl: std::env::var("SMTP_USE_SSL")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            from_email: std::env::var("SMTP_FROM_EMAIL")
                .context("SMTP_FROM_EMAIL not set")?,
            from_name: std::env::var("SMTP_FROM_NAME")
                .context("SMTP_FROM_NAME not set")?,
            download_base_url: std::env::var("GENETICS_DOWNLOAD_BASE_URL")
                .context("GENETICS_DOWNLOAD_BASE_URL not set")?,
            template_dir: std::env::var("GENETICS_EMAIL_TEMPLATE_DIR")
                .context("GENETICS_EMAIL_TEMPLATE_DIR not set")?,
        })
    }

    /// Read SMTP password from secret file
    fn read_smtp_password() -> Result<String> {
        let password_file = std::env::var("SMTP_PASSWORD_FILE")
            .context("SMTP_PASSWORD_FILE not set")?;

        fs::read_to_string(&password_file)
            .with_context(|| format!("Failed to read SMTP password from {}", password_file))
            .map(|s| s.trim().to_string())
    }
}

// ==============================================================================
// EMAIL SENDER
// ==============================================================================

/// Email sender for genetics processing notifications
pub struct EmailSender {
    config: EmailConfig,
}

impl EmailSender {
    /// Create a new email sender
    pub fn new(config: EmailConfig) -> Self {
        Self { config }
    }

    /// Send download notification email
    ///
    /// # Arguments
    ///
    /// * `job_id` - The job UUID
    /// * `user_email` - Recipient email address
    /// * `download_token` - The secure download token
    /// * `download_password` - The download password (plain text, to be sent to user)
    /// * `completed_at` - When the job completed
    /// * `expires_at` - When the download link expires
    pub fn send_download_notification(
        &self,
        job_id: Uuid,
        user_email: &str,
        download_token: &str,
        download_password: &str,
        completed_at: &DateTime<Utc>,
        expires_at: &DateTime<Utc>,
    ) -> Result<()> {
        info!("Sending download notification email to {}", user_email);

        // Build template variables
        let mut variables = HashMap::new();
        variables.insert("job_id".to_string(), job_id.to_string());
        variables.insert("completed_at".to_string(), format_datetime(completed_at));
        variables.insert("expires_at".to_string(), format_datetime(expires_at));
        variables.insert("download_password".to_string(), download_password.to_string());
        variables.insert(
            "download_url".to_string(),
            format!("{}?token={}", self.config.download_base_url, download_token),
        );

        // Load and render templates
        let html_template_path = Path::new(&self.config.template_dir).join("download_ready.html");
        let text_template_path = Path::new(&self.config.template_dir).join("download_ready.txt");

        let html_template = fs::read_to_string(&html_template_path)
            .with_context(|| format!("Failed to read HTML template from {:?}", html_template_path))?;

        let text_template = fs::read_to_string(&text_template_path)
            .with_context(|| format!("Failed to read text template from {:?}", text_template_path))?;

        let html_body = render_template(&html_template, &variables);
        let text_body = render_template(&text_template, &variables);

        // Build email message
        let from_mailbox = format!("{} <{}>", self.config.from_name, self.config.from_email)
            .parse()
            .context("Failed to parse from address")?;

        let to_mailbox = user_email
            .parse()
            .context("Failed to parse recipient address")?;

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject("Your Genetic Data Processing Results are Ready")
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text_body),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html_body),
                    ),
            )
            .context("Failed to build email message")?;

        // Send email via SMTP
        let credentials = Credentials::new(
            self.config.smtp_username.clone(),
            self.config.smtp_password.clone(),
        );

        let mailer = if self.config.smtp_use_tls || self.config.smtp_use_ssl {
            SmtpTransport::relay(&self.config.smtp_host)?
                .credentials(credentials)
                .port(self.config.smtp_port)
                .build()
        } else {
            // No TLS for internal SMTP relay (e.g., local mail bridge)
            SmtpTransport::builder_dangerous(&self.config.smtp_host)
                .credentials(credentials)
                .port(self.config.smtp_port)
                .build()
        };

        match mailer.send(&email) {
            Ok(_) => {
                info!("Email sent successfully to {} for job {}", user_email, job_id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to send email to {} for job {}: {}", user_email, job_id, e);
                Err(anyhow::anyhow!("SMTP send failed: {}", e))
            }
        }
    }
}

// ==============================================================================
// TESTS
// ==============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template() {
        let template = "Hello {{name}}, your job {{job_id}} is complete!";
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "Alice".to_string());
        variables.insert("job_id".to_string(), "12345".to_string());

        let result = render_template(template, &variables);
        assert_eq!(result, "Hello Alice, your job 12345 is complete!");
    }

    #[test]
    fn test_render_template_missing_variable() {
        let template = "Hello {{name}}, your job {{job_id}} is complete!";
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "Alice".to_string());
        // job_id intentionally missing

        let result = render_template(template, &variables);
        // Missing variables are left as-is
        assert_eq!(result, "Hello Alice, your job {{job_id}} is complete!");
    }

    #[test]
    fn test_format_datetime() {
        let dt = DateTime::parse_from_rfc3339("2025-11-18T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let formatted = format_datetime(&dt);
        assert_eq!(formatted, "November 18, 2025 at 10:30 AM UTC");
    }
}

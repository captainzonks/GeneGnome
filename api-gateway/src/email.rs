// ==============================================================================
// email.rs - Email Notification System (API Gateway)
// ==============================================================================
// Description: Send email notifications for resend requests
// Author: Matt Barham
// Created: 2026-04-02
// Modified: 2026-04-02
// Version: 1.0.0
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
use tracing::{error, info};
use uuid::Uuid;

fn render_template(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

fn format_datetime(dt: &DateTime<Utc>) -> String {
    dt.format("%B %d, %Y at %I:%M %p UTC").to_string()
}

struct EmailConfig {
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
    fn from_env() -> Result<Self> {
        Ok(Self {
            smtp_host: std::env::var("SMTP_HOST").context("SMTP_HOST not set")?,
            smtp_port: std::env::var("SMTP_PORT")
                .context("SMTP_PORT not set")?
                .parse()
                .context("SMTP_PORT must be a valid port number")?,
            smtp_username: std::env::var("SMTP_USERNAME").context("SMTP_USERNAME not set")?,
            smtp_password: Self::read_smtp_password()?,
            smtp_use_tls: std::env::var("SMTP_USE_TLS")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            smtp_use_ssl: std::env::var("SMTP_USE_SSL")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            from_email: std::env::var("SMTP_FROM_EMAIL").context("SMTP_FROM_EMAIL not set")?,
            from_name: std::env::var("SMTP_FROM_NAME").context("SMTP_FROM_NAME not set")?,
            download_base_url: std::env::var("GENETICS_DOWNLOAD_BASE_URL")
                .context("GENETICS_DOWNLOAD_BASE_URL not set")?,
            template_dir: std::env::var("GENETICS_EMAIL_TEMPLATE_DIR")
                .context("GENETICS_EMAIL_TEMPLATE_DIR not set")?,
        })
    }

    fn read_smtp_password() -> Result<String> {
        let password_file = std::env::var("SMTP_PASSWORD_FILE")
            .context("SMTP_PASSWORD_FILE not set")?;
        fs::read_to_string(&password_file)
            .with_context(|| format!("Failed to read SMTP password from {}", password_file))
            .map(|s| s.trim().to_string())
    }
}

pub struct EmailSender {
    config: EmailConfig,
}

impl EmailSender {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            config: EmailConfig::from_env()?,
        })
    }

    /// Send download notification email (async wrapper around blocking SMTP)
    pub async fn send_download_notification(
        &self,
        job_id: Uuid,
        user_email: &str,
        download_token: &str,
        download_password: &str,
        completed_at: &DateTime<Utc>,
        expires_at: &DateTime<Utc>,
    ) -> Result<()> {
        info!("Sending download notification email to {}", user_email);

        let mut variables = HashMap::new();
        variables.insert("job_id".to_string(), job_id.to_string());
        variables.insert("completed_at".to_string(), format_datetime(completed_at));
        variables.insert("expires_at".to_string(), format_datetime(expires_at));
        variables.insert("download_password".to_string(), download_password.to_string());
        variables.insert(
            "download_url".to_string(),
            format!("{}?token={}", self.config.download_base_url, download_token),
        );
        let visualize_base = self.config.download_base_url.replace("/download", "/visualize");
        variables.insert(
            "visualize_url".to_string(),
            format!("{}?token={}", visualize_base, download_token),
        );

        let html_template_path = Path::new(&self.config.template_dir).join("download_ready.html");
        let text_template_path = Path::new(&self.config.template_dir).join("download_ready.txt");

        let html_template = fs::read_to_string(&html_template_path)
            .with_context(|| format!("Failed to read HTML template from {:?}", html_template_path))?;
        let text_template = fs::read_to_string(&text_template_path)
            .with_context(|| format!("Failed to read text template from {:?}", text_template_path))?;

        let html_body = render_template(&html_template, &variables);
        let text_body = render_template(&text_template, &variables);

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

        // Build SMTP transport config
        let smtp_host = self.config.smtp_host.clone();
        let smtp_port = self.config.smtp_port;
        let smtp_use_tls = self.config.smtp_use_tls;
        let smtp_use_ssl = self.config.smtp_use_ssl;
        let credentials = Credentials::new(
            self.config.smtp_username.clone(),
            self.config.smtp_password.clone(),
        );

        let user_email_owned = user_email.to_string();

        // Run blocking SMTP send in spawn_blocking
        tokio::task::spawn_blocking(move || {
            let mailer = if smtp_use_tls || smtp_use_ssl {
                SmtpTransport::relay(&smtp_host)?
                    .credentials(credentials)
                    .port(smtp_port)
                    .build()
            } else {
                SmtpTransport::builder_dangerous(&smtp_host)
                    .credentials(credentials)
                    .port(smtp_port)
                    .build()
            };

            match mailer.send(&email) {
                Ok(_) => {
                    info!("Email sent successfully to {} for job {}", user_email_owned, job_id);
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to send email to {} for job {}: {}", user_email_owned, job_id, e);
                    Err(anyhow::anyhow!("SMTP send failed: {}", e))
                }
            }
        })
        .await
        .context("Email send task panicked")?
    }
}

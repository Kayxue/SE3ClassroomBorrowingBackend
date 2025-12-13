use std::sync::OnceLock;

use mail_send::{SmtpClientBuilder, mail_builder::MessageBuilder};

static GLOBAL_EMAIL_CONFIG: OnceLock<EmailClientConfig> = OnceLock::new();

#[derive(Clone)]
pub struct EmailClientConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
}

pub fn set_email_client_config(config: EmailClientConfig) {
    let _ = GLOBAL_EMAIL_CONFIG.set(config);
}

pub async fn send_email(
    to: impl AsRef<str>,
    subject: impl AsRef<str>,
    body: impl AsRef<str>,
) -> Result<(), mail_send::Error> {
    let config = GLOBAL_EMAIL_CONFIG
        .get()
        .expect("Email client config not set");

    let message = MessageBuilder::new()
        .from(config.username.as_ref())
        .to(to.as_ref())
        .subject(subject.as_ref())
        .text_body(body.as_ref());

    SmtpClientBuilder::new(config.smtp_server.as_ref(), config.smtp_port)
        .implicit_tls(false)
        .credentials((config.username.as_ref(), config.password.as_ref()))
        .connect()
        .await?
        .send(message)
        .await?;

    Ok(())
}

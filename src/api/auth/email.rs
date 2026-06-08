use anyhow::Context;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::Config;

pub async fn send_verification_email(config: &Config, to_email: &str, link: &str) {
    let body = format!(
        "Welcome to {name}!\n\n\
         Please verify your email address by opening:\n\n\
         {link}\n\n\
         If you did not create an account you can ignore this message.\n",
        name = config.rp_name,
    );
    deliver(
        config,
        to_email,
        "Verify your email address",
        &body,
        "verification link",
        link,
    )
    .await;
}

pub async fn send_password_reset_email(config: &Config, to_email: &str, link: &str) {
    let body = format!(
        "We received a request to reset the password for your {name} account.\n\n\
         Open the link below to choose a new password:\n\n\
         {link}\n\n\
         This link expires soon and can be used only once. If you did not \
         request a password reset you can safely ignore this message; your \
         password will not change.\n",
        name = config.rp_name,
    );
    deliver(
        config,
        to_email,
        "Reset your password",
        &body,
        "password reset link",
        link,
    )
    .await;
}

async fn deliver(
    config: &Config,
    to_email: &str,
    subject: &str,
    body: &str,
    dev_label: &str,
    dev_link: &str,
) {
    match &config.smtp {
        None => {
            println!("[dev-email] {dev_label} for {to_email}:\n            {dev_link}");
        }
        Some(_) => {
            if let Err(err) = send_via_smtp(config, to_email, subject, body).await {
                tracing::error!(error = %err, "failed to send email");
            }
        }
    }
}

async fn send_via_smtp(
    config: &Config,
    to_email: &str,
    subject: &str,
    body: &str,
) -> anyhow::Result<()> {
    let smtp = config.smtp.as_ref().context("smtp configuration missing")?;

    let from = Mailbox::new(
        Some(config.rp_name.clone()),
        smtp.from.parse().context("invalid SMTP_FROM address")?,
    );
    let to = Mailbox::new(None, to_email.parse().context("invalid recipient address")?);

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .body(body.to_string())
        .context("building email")?;

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)
        .context("configuring SMTP transport")?
        .port(smtp.port)
        .credentials(Credentials::new(smtp.user.clone(), smtp.pass.clone()))
        .build();

    mailer.send(email).await.context("sending email")?;
    Ok(())
}

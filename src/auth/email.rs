use anyhow::Context;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::Config;

pub async fn send_verification_email(config: &Config, to_email: &str, link: &str) {
    match &config.smtp {
        None => {
            println!(
                "[dev-email] verification link for {to_email}:\n            {link}"
            );
        }
        Some(_) => {
            if let Err(err) = send_via_smtp(config, to_email, link).await {
                tracing::error!(error = %err, "failed to send verification email");
            }
        }
    }
}

async fn send_via_smtp(config: &Config, to_email: &str, link: &str) -> anyhow::Result<()> {
    let smtp = config
        .smtp
        .as_ref()
        .context("smtp configuration missing")?;

    let from = Mailbox::new(
        Some(config.rp_name.clone()),
        smtp.from.parse().context("invalid SMTP_FROM address")?,
    );
    let to = Mailbox::new(None, to_email.parse().context("invalid recipient address")?);
    let body = format!(
        "Welcome to {name}!\n\n\
         Please verify your email address by opening:\n\n\
         {link}\n\n\
         If you did not create an account you can ignore this message.\n",
        name = config.rp_name,
    );

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject("Verify your email address")
        .body(body)
        .context("building email")?;

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)
        .context("configuring SMTP transport")?
        .port(smtp.port)
        .credentials(Credentials::new(smtp.user.clone(), smtp.pass.clone()))
        .build();

    mailer.send(email).await.context("sending email")?;
    Ok(())
}

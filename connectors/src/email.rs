use brain::bus::{MessageBus, OutboundMessage};
use brain::config::EmailConfig;
use brain::error::{Error, Result};
use async_trait::async_trait;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct EmailConnector {
    config: EmailConfig,
}

impl EmailConnector {
    pub fn new(config: EmailConfig) -> Self {
        Self { config }
    }

    async fn run_imap_polling(&self, _bus: Arc<MessageBus>) -> Result<()> {
        info!("Email IMAP polling started for {}", self.config.imap_user);
        
        // Simple polling approach
        loop {
            // TODO: Implement actual IMAP fetching. 
            // For now, this is a placeholder loop as per roadmap Phase 2.1
            // Real implementation would use imap-next or similar to check for new messages.
            
            sleep(Duration::from_secs(60)).await;
        }
    }
}

#[async_trait]
impl super::Connector for EmailConnector {
    fn name(&self) -> &str {
        "email"
    }

    fn metadata() -> super::ChannelMetadata {
        super::ChannelMetadata {
            id: "email".to_string(),
            name: "Email".to_string(),
            description: "SMTP/IMAP communication for long-term persistence".to_string(),
            icon: "📧".to_string(),
            fields: vec![
                super::ChannelField {
                    key: "SMTP_SERVER".to_string(),
                    label: "SMTP Server".to_string(),
                    field_type: "text".to_string(),
                    description: "e.g., smtp.gmail.com".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "SMTP_PORT".to_string(),
                    label: "SMTP Port".to_string(),
                    field_type: "number".to_string(),
                    description: "e.g., 587 or 465".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "SMTP_USER".to_string(),
                    label: "SMTP Username".to_string(),
                    field_type: "text".to_string(),
                    description: "Your email address".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "SMTP_PASS".to_string(),
                    label: "SMTP Password".to_string(),
                    field_type: "password".to_string(),
                    description: "App-specific password recommended".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "FROM_ADDRESS".to_string(),
                    label: "From Address".to_string(),
                    field_type: "text".to_string(),
                    description: "The 'From' email address".to_string(),
                    required: true,
                },
            ],
        }
    }

    async fn start(&self, bus: Arc<MessageBus>) -> Result<()> {
        let bus_clone = bus.clone();

        // Handle outbound messages
        let mut outbound_rx = bus.subscribe_outbound();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let connector = EmailConnector::new(config);
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel == "email" || msg.channel == "broadcast" {
                    if let Err(e) = connector.send(msg).await {
                        error!("Email send error: {}", e);
                    }
                }
            }
        });

        // Start IMAP polling in background
        self.run_imap_polling(bus_clone).await?;
        
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        info!("Sending email to {}", message.chat_id);

        let email = Message::builder()
            .from(self.config.from_address.parse().map_err(|e| Error::Internal(format!("Invalid from address: {}", e)))?)
            .to(message.chat_id.parse().map_err(|e| Error::Internal(format!("Invalid to address: {}", e)))?)
            .subject("AIMAXXING Notification")
            .body(message.content)
            .map_err(|e| Error::Internal(format!("Failed to build email: {}", e)))?;

        let creds = Credentials::new(self.config.smtp_user.clone(), self.config.smtp_pass.clone());

        let mailer = SmtpTransport::relay(&self.config.smtp_server)
            .map_err(|e| Error::Internal(format!("Failed to create SMTP transport: {}", e)))?
            .port(self.config.smtp_port)
            .credentials(creds)
            .build();

        // Send the email
        match mailer.send(&email) {
            Ok(_) => {
                info!("Email sent successfully!");
                Ok(())
            }
            Err(e) => {
                error!("Could not send email: {:?}", e);
                Err(Error::Internal(format!("SMTP error: {}", e)))
            }
        }
    }
}

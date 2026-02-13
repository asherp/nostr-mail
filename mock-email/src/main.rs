mod config;
mod imap;
mod smtp;
mod store;
mod test_utils;
mod types;

use anyhow::Result;
use clap::Parser;
use config::{Config, write_emails_to_file};
use log::{info, warn};
use store::EmailStore;
use std::sync::Arc;
use test_utils::{generate_fake_email_addresses_with_seed, generate_fake_emails_with_pool};
use tokio::signal;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::parse();

    // Initialize logger
    let mut logger_builder = env_logger::Builder::from_default_env();
    logger_builder.filter_level(
        config
            .log_level
            .parse()
            .unwrap_or(log::LevelFilter::Info),
    );
    
    // Configure file logging (default to email.log if not specified)
    let log_file_path = config.log_file.clone().unwrap_or_else(|| {
        std::path::PathBuf::from("email.log")
    });
    
    use std::fs::OpenOptions;
    
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log_file_path)
        .map_err(|e| anyhow::anyhow!("Failed to open log file {:?}: {}", log_file_path, e))?;
    
    // Create a writer that writes to both stdout and file
    struct DualWriter {
        file: std::fs::File,
    }
    
    impl std::io::Write for DualWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            // Write to stdout
            std::io::stdout().write_all(buf)?;
            // Write to file
            self.file.write_all(buf)?;
            Ok(buf.len())
        }
        
        fn flush(&mut self) -> std::io::Result<()> {
            std::io::stdout().flush()?;
            self.file.flush()
        }
    }
    
    logger_builder.target(env_logger::Target::Pipe(Box::new(DualWriter { file })));
    println!("Logging to file: {:?}", log_file_path);
    
    logger_builder.init();

    info!("Starting mock-email server");
    info!("Configuration: {:?}", config);

    // Create email store and initialize default mailboxes
    let store = Arc::new(EmailStore::new());
    store.init().await;

    // Generate fake emails if requested
    let mut preload_emails: Vec<(crate::types::Email, String)> = Vec::new();
    if let Some(count) = config.generate_fake_emails {
        info!("Generating {} fake emails with seed {}", count, config.seed);
        
        // Generate email address pool
        let email_pool = generate_fake_email_addresses_with_seed(count, Some(config.seed));
        info!("Created email pool with {} addresses", email_pool.len());
        
        // Generate emails using the pool
        let emails_with_metadata = generate_fake_emails_with_pool(count, Some(config.seed), &email_pool);
        preload_emails = emails_with_metadata
            .iter()
            .map(|ewm| (ewm.email.clone(), ewm.mailbox.clone()))
            .collect();
        
        info!("Generated {} total fake emails", preload_emails.len());

        // Write emails to file (default to emails.json if not specified)
        let output_path = config.output_emails.clone().unwrap_or_else(|| {
            std::path::PathBuf::from("emails.json")
        });
        info!("Writing {} emails to {:?}", preload_emails.len(), output_path);
        match write_emails_to_file(&preload_emails, output_path.clone()) {
            Ok(_) => {
                info!("Successfully wrote emails to {:?}", output_path);
            }
            Err(e) => {
                warn!("Failed to write emails to file: {}", e);
            }
        }
    }

    // Preload emails if specified
    if let Some(preload_path) = &config.preload_emails {
        info!("Preloading emails from {:?}", preload_path);
        match config::load_preload_emails(preload_path.to_path_buf()) {
            Ok(emails) => {
                info!("Loaded {} emails from file", emails.len());
                preload_emails.extend(emails);
            }
            Err(e) => {
                warn!("Failed to load preload emails: {}", e);
            }
        }
    }

    // Preload emails into store
    if !preload_emails.is_empty() {
        info!("Preloading {} emails into store", preload_emails.len());
        for (email, mailbox) in &preload_emails {
            store.add_email(email.clone(), mailbox).await;
        }
        info!("Preloaded {} emails", preload_emails.len());
    }

    // Start SMTP server
    let smtp_addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.smtp_port));
    let smtp_server = smtp::SmtpServer::new(smtp_addr, store.clone());
    let smtp_handle = tokio::spawn(async move {
        if let Err(e) = smtp_server.start().await {
            eprintln!("SMTP server error: {}", e);
        }
    });

    // Start IMAP server
    let imap_addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.imap_port));
    let imap_server = imap::ImapServer::new(imap_addr, store.clone());
    let imap_handle = tokio::spawn(async move {
        if let Err(e) = imap_server.start().await {
            eprintln!("IMAP server error: {}", e);
        }
    });

    info!("Mock email server running:");
    info!("  SMTP: smtp://0.0.0.0:{}", config.smtp_port);
    info!("  IMAP: imap://0.0.0.0:{}", config.imap_port);
    info!("Press Ctrl+C to stop.");

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Received shutdown signal");
        }
        Err(err) => {
            warn!("Unable to listen for shutdown signal: {}", err);
        }
    }

    // Cancel server tasks
    smtp_handle.abort();
    imap_handle.abort();
    
    info!("Shutdown complete");

    Ok(())
}

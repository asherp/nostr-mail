mod config;
mod protocol;
mod relay;
mod server;
mod store;
mod test_utils;
mod types;

use anyhow::Result;
use clap::Parser;
use config::{Config, write_events_with_keys_to_file};
use log::{info, warn};
use relay::RelayManager;
use test_utils::{generate_fake_events_with_seed, generate_fake_events_with_pool};
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
    
    // Configure file logging (default to relay.log if not specified)
    let log_file_path = config.log_file.clone().unwrap_or_else(|| {
        std::path::PathBuf::from("relay.log")
    });
    
    use std::fs::OpenOptions;
    use std::io::Write;
    
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
    
    impl Write for DualWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            use std::io::Write;
            // Write to stdout
            std::io::stdout().write_all(buf)?;
            // Write to file
            self.file.write_all(buf)?;
            Ok(buf.len())
        }
        
        fn flush(&mut self) -> std::io::Result<()> {
            use std::io::Write;
            std::io::stdout().flush()?;
            self.file.flush()
        }
    }
    
    logger_builder.target(env_logger::Target::Pipe(Box::new(DualWriter { file })));
    println!("Logging to file: {:?}", log_file_path);
    
    logger_builder.init();

    info!("Starting mock-relay server");
    info!("Configuration: {:?}", config);

    // Create relay manager
    let mut manager = RelayManager::new();

    // Generate fake events if requested
    let mut preload_events = Vec::new();
    let mut events_with_keys = Vec::new();
    if let Some(count) = config.generate_fake_events {
        info!("Generating {} fake events per kind with seed {}", count, config.seed);
        
        // First, generate kind 0 events (profiles) to create the pubkey pool
        let profile_events = generate_fake_events_with_seed(0, count, Some(config.seed));
        events_with_keys.extend(profile_events.clone());
        
        // Extract pubkey pool from profiles: Vec<(pubkey, private_key)>
        let pubkey_pool: Vec<(String, String)> = profile_events.iter()
            .map(|ewk| (ewk.event.pubkey.clone(), ewk.private_key.clone()))
            .collect();
        
        info!("Created pubkey pool with {} pubkeys", pubkey_pool.len());
        
        // Validate N is even and >= 4 for graph-based DM generation
        if pubkey_pool.len() < 4 || pubkey_pool.len() % 2 != 0 {
            warn!("For graph-based DM generation, number of users must be even and >= 4. Got {}. DM generation may fail.", pubkey_pool.len());
        }
        
        // Generate kind 3 and kind 4 events using the same pubkey pool
        events_with_keys.extend(generate_fake_events_with_pool(3, count, Some(config.seed), &pubkey_pool)); // Contacts
        
        // Generate DMs using graph structure
        // Each user participates in at least 3 conversations, each conversation has 4 messages
        let dm_events = generate_fake_events_with_pool(4, count, Some(config.seed), &pubkey_pool);
        let num_conversations = dm_events.len() / 4;
        info!("Generated {} DM events ({} conversations, 4 messages each). Each user participates in at least 3 conversations.", dm_events.len(), num_conversations);
        events_with_keys.extend(dm_events);
        
        info!("Generated {} total fake events", events_with_keys.len());

        // Extract events for preloading (without keys)
        preload_events = events_with_keys.iter().map(|ewk| ewk.event.clone()).collect();

        // Collect relay URLs before starting relays
        let mut relay_urls = Vec::new();
        if config.relays == 1 {
            relay_urls.push(format!("ws://127.0.0.1:{}", config.port));
        } else {
            for i in 0..config.relays {
                relay_urls.push(format!("ws://127.0.0.1:{}", config.start_port + i as u16));
            }
        }

        // Write events with keys to file (default to events.json if not specified)
        let output_path = config.output_events.clone().unwrap_or_else(|| {
            std::path::PathBuf::from("events.json")
        });
        info!("Writing {} events (with private keys) to {:?}", events_with_keys.len(), output_path);
        match write_events_with_keys_to_file(&events_with_keys, output_path.clone(), Some(relay_urls.clone())) {
            Ok(_) => {
                info!("Successfully wrote events to {:?}", output_path);
            }
            Err(e) => {
                warn!("Failed to write events to file: {}", e);
            }
        }
    }

    // Preload events if specified
    if let Some(preload_path) = &config.preload_events {
        info!("Preloading events from {:?}", preload_path);
        match config::load_preload_events(preload_path.to_path_buf()) {
            Ok(events) => {
                info!("Loaded {} events from file", events.len());
                preload_events.extend(events);
            }
            Err(e) => {
                warn!("Failed to load preload events: {}", e);
            }
        }
    }

    // Start relays
    if config.relays == 1 {
        info!("Starting single relay on port {}", config.port);
        manager.start_relay(config.port).await?;
    } else {
        info!(
            "Starting {} relays starting from port {}",
            config.relays, config.start_port
        );
        let addrs = manager.start_relays(config.relays, config.start_port).await?;
        info!("Started relays on: {:?}", addrs);
    }

    // Preload events into relays after they're started
    if !preload_events.is_empty() {
        info!("Preloading {} events into relays", preload_events.len());
        for i in 0..manager.count() {
            if let Some(store) = manager.get_store(i) {
                for event in &preload_events {
                    store.add_event(event.clone()).await;
                }
                info!("Preloaded {} events into relay {}", preload_events.len(), i);
            }
        }
    }

    info!("Mock relay server(s) running. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Received shutdown signal");
        }
        Err(err) => {
            warn!("Unable to listen for shutdown signal: {}", err);
        }
    }

    // Stop all relays
    manager.stop_all().await;
    info!("Shutdown complete");

    Ok(())
}

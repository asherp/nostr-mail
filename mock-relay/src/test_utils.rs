use crate::types::Event;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use nostr_sdk::prelude::*;
use bip39::Language;

/// Event with its associated private key (for testing)
#[derive(Debug, Clone)]
pub struct EventWithKey {
    pub event: Event,
    pub private_key: String, // nsec format
}


/// Generate a real secp256k1 keypair using nostr-sdk
/// 
/// Returns (private_key_nsec, public_key_npub)
pub fn generate_real_keypair() -> anyhow::Result<(String, String)> {
    let keys = Keys::generate();
    Ok((
        keys.secret_key().to_bech32()?,
        keys.public_key().to_bech32()?,
    ))
}

/// Generate a real secp256k1 keypair using nostr-sdk with a seeded RNG
/// 
/// Returns (private_key_nsec, public_key_npub)
pub fn generate_real_keypair_with_rng(rng: &mut impl Rng) -> anyhow::Result<(String, String)> {
    // Generate 32 random bytes for the secret key
    let mut secret_bytes = [0u8; 32];
    rng.fill(&mut secret_bytes);
    
    let secret_key = SecretKey::from_slice(&secret_bytes)?;
    let keys = Keys::new(secret_key);
    Ok((
        keys.secret_key().to_bech32()?,
        keys.public_key().to_bech32()?,
    ))
}


/// Generate a properly signed event using real keys
pub fn generate_signed_event(
    nsec: &str,
    kind: u16,
    content: &str,
    tags: Vec<Vec<String>>,
) -> anyhow::Result<Event> {
    use nostr_sdk::prelude::*;
    
    let secret_key = SecretKey::from_bech32(nsec)?;
    let keys = Keys::new(secret_key);
    
    let kind_enum = Kind::from(kind);
    let mut builder = EventBuilder::new(kind_enum, content);
    
    for tag_vec in tags {
        if let Ok(tag) = Tag::parse(tag_vec) {
            builder = builder.tag(tag);
        }
    }
    
    let nostr_event = builder.build(keys.public_key()).sign_with_keys(&keys)?;
    
    // Convert nostr-sdk Event to our Event type
    let tags: Vec<Vec<String>> = nostr_event.tags.into_iter()
        .map(|tag| tag.to_vec())
        .collect();
    
    Ok(crate::types::Event {
        id: nostr_event.id.to_hex(),
        pubkey: nostr_event.pubkey.to_bech32()
            .unwrap_or_else(|_| nostr_event.pubkey.to_hex()),
        created_at: nostr_event.created_at.as_u64() as i64,
        kind: nostr_event.kind.as_u16(),
        tags,
        content: nostr_event.content,
        sig: nostr_event.sig.to_string(),
    })
}


/// Generate a real profile event with random data and proper signature
pub fn generate_fake_profile_event() -> EventWithKey {
    generate_fake_profile_event_with_rng(&mut rand::thread_rng(), None)
}

/// Generate a real profile event with random data and proper signature (with seeded RNG)
/// If `name` is provided, use it; otherwise select randomly from the names array
pub fn generate_fake_profile_event_with_rng(rng: &mut impl Rng, name: Option<&str>) -> EventWithKey {
    let names = [
        "Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Henry",
        "Iris", "Jack", "Kate", "Liam", "Mia", "Noah", "Olivia", "Paul",
        "Quinn", "Ruby", "Sam", "Tina"
    ];
    let domains = ["example.com", "test.org", "demo.net", "fake.io"];
    
    // Use provided name or select randomly
    let name = name.unwrap_or_else(|| names[rng.gen_range(0..names.len())]);
    let email = format!("{}@{}", name.to_lowercase(), domains[rng.gen_range(0..domains.len())]);
    
    // Generate real keypair with seeded RNG
    let (nsec, _) = generate_real_keypair_with_rng(rng).expect("Failed to generate keypair");
    
    // Create properly signed event
    let mut metadata = serde_json::Map::new();
    metadata.insert("name".to_string(), serde_json::Value::String(name.to_lowercase()));
    metadata.insert("display_name".to_string(), serde_json::Value::String(name.to_string()));
    metadata.insert("about".to_string(), serde_json::Value::String(format!("About {}", name)));
    metadata.insert("picture".to_string(), serde_json::Value::String(format!("https://example.com/avatars/{}.jpg", name.to_lowercase())));
    metadata.insert("email".to_string(), serde_json::Value::String(email));
    
    let event = generate_signed_event(&nsec, 0, &serde_json::to_string(&metadata).unwrap(), vec![])
        .expect("Failed to generate signed event");
    
    EventWithKey {
        event,
        private_key: nsec,
    }
}

/// Generate a real contact list event with random contacts and proper signature
pub fn generate_fake_contact_list_event(num_contacts: usize) -> EventWithKey {
    generate_fake_contact_list_event_with_rng(num_contacts, &mut rand::thread_rng())
}

/// Generate a real contact list event with random contacts and proper signature (with seeded RNG)
pub fn generate_fake_contact_list_event_with_rng(num_contacts: usize, rng: &mut impl Rng) -> EventWithKey {
    // Generate real keypair for the event author first
    let (nsec, author_npub) = generate_real_keypair_with_rng(rng).expect("Failed to generate keypair");
    
    // Generate real keypairs for contacts, ensuring the author is not included
    let mut contacts: Vec<String> = Vec::new();
    while contacts.len() < num_contacts {
        let (_, contact_pubkey) = generate_real_keypair_with_rng(rng).expect("Failed to generate keypair");
        // Ensure the author is not in their own contact list
        if contact_pubkey != author_npub {
            contacts.push(contact_pubkey);
        }
    }
    
    // Create tags
    let tags: Vec<Vec<String>> = contacts
        .iter()
        .map(|contact| vec!["p".to_string(), contact.clone()])
        .collect();
    
    let event = generate_signed_event(&nsec, 3, "", tags)
        .expect("Failed to generate signed event");
    
    EventWithKey {
        event,
        private_key: nsec,
    }
}

/// Generate random plaintext message using BIP-39 words
fn generate_random_plaintext(rng: &mut impl Rng) -> String {
    // Get the English BIP-39 word list (returns a fixed-size array of 2048 words)
    let wordlist = Language::English.word_list();
    
    // Generate a random number of words (between 5 and 20 for realistic messages)
    let num_words = rng.gen_range(5..20);
    
    // Select random words from the BIP-39 word list
    let words: Vec<&str> = (0..num_words)
        .map(|_| {
            let idx = rng.gen_range(0..wordlist.len());
            wordlist[idx]
        })
        .collect();
    
    // Join words with spaces and capitalize first letter
    let mut message = words.join(" ");
    if let Some(first_char) = message.chars().next() {
        let first_upper: String = first_char.to_uppercase().collect();
        message = first_upper + &message[1..];
    }
    
    // Add punctuation randomly
    if rng.gen_bool(0.7) {
        if rng.gen_bool(0.5) {
            message.push('.');
        } else {
            message.push('!');
        }
    }
    
    message
}

/// Encrypt a plaintext message using NIP-44 or NIP-04 encryption
fn encrypt_dm_content(
    sender_secret_key: &SecretKey,
    recipient_public_key: &PublicKey,
    plaintext: &str,
    rng: &mut impl Rng,
) -> anyhow::Result<String> {
    // Use NIP-44 encryption (default, more secure)
    // Randomly choose between NIP-44 and NIP-04 for variety (70% NIP-44, 30% NIP-04)
    if rng.gen_bool(0.7) {
        nip44::encrypt(sender_secret_key, recipient_public_key, plaintext, nip44::Version::default())
            .map_err(|e| anyhow::anyhow!("NIP-44 encryption failed: {}", e))
    } else {
        nip04::encrypt(sender_secret_key, recipient_public_key, plaintext)
            .map_err(|e| anyhow::anyhow!("NIP-04 encryption failed: {}", e))
    }
}

/// Generate a real DM event with random encrypted content and proper signature
pub fn generate_fake_dm_event() -> EventWithKey {
    generate_fake_dm_event_with_rng(&mut rand::thread_rng())
}

/// Generate a real DM event with random encrypted content and proper signature (with seeded RNG)
pub fn generate_fake_dm_event_with_rng(rng: &mut impl Rng) -> EventWithKey {
    // Generate random plaintext using BIP-39 words
    let plaintext = generate_random_plaintext(rng);
    
    // Generate real keypairs with seeded RNG
    let (sender_nsec, _) = generate_real_keypair_with_rng(rng).expect("Failed to generate keypair");
    let (_, recipient_npub) = generate_real_keypair_with_rng(rng).expect("Failed to generate keypair");
    
    // Parse keys for encryption
    let sender_secret_key = SecretKey::from_bech32(&sender_nsec).expect("Failed to parse sender secret key");
    let recipient_public_key = PublicKey::from_bech32(&recipient_npub).expect("Failed to parse recipient public key");
    
    // Encrypt the plaintext message
    let encrypted_content = encrypt_dm_content(&sender_secret_key, &recipient_public_key, &plaintext, rng)
        .expect("Failed to encrypt DM content");
    
    // Create tags with recipient pubkey
    let tags = vec![vec!["p".to_string(), recipient_npub]];
    
    let event = generate_signed_event(&sender_nsec, 4, &encrypted_content, tags)
        .expect("Failed to generate signed event");
    
    EventWithKey {
        event,
        private_key: sender_nsec,
    }
}

/// Generate a real AUTH event with proper signature
pub fn generate_fake_auth_event() -> EventWithKey {
    generate_fake_auth_event_with_rng(&mut rand::thread_rng())
}

/// Generate a real AUTH event with proper signature (with seeded RNG)
pub fn generate_fake_auth_event_with_rng(rng: &mut impl Rng) -> EventWithKey {
    let relay_urls = [
        "wss://relay.example.com",
        "wss://relay.test.org",
        "wss://relay.demo.net",
    ];
    
    let challenge: String = (0..32)
        .map(|_| {
            let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
            chars[rng.gen_range(0..chars.len())] as char
        })
        .collect();
    
    // Generate real keypair with seeded RNG
    let (nsec, _) = generate_real_keypair_with_rng(rng).expect("Failed to generate keypair");
    let relay_url = relay_urls[rng.gen_range(0..relay_urls.len())];
    
    // Create tags
    let tags = vec![
        vec!["relay".to_string(), relay_url.to_string()],
        vec!["challenge".to_string(), challenge],
    ];
    
    let event = generate_signed_event(&nsec, 22242, "", tags)
        .expect("Failed to generate signed event");
    
    EventWithKey {
        event,
        private_key: nsec,
    }
}

/// Generate multiple fake events of a specific kind
pub fn generate_fake_events(kind: u16, count: usize) -> Vec<EventWithKey> {
    generate_fake_events_with_seed(kind, count, None)
}

/// Generate a real contact list event with contacts from a pool of pubkeys
/// This function includes ALL other profiles in the contact list (excluding the author)
/// The author is explicitly excluded from their own contact list
pub fn generate_fake_contact_list_event_with_pool(
    _num_contacts: usize, // Ignored - we include all other profiles
    _rng: &mut impl Rng,  // Ignored - we include all other profiles
    pubkey_pool: &[String],
    author_pubkey: &str,
    author_nsec: &str,
) -> EventWithKey {
    // Include ALL other profiles in the contact list (excluding the author)
    // Filter out the author to ensure they are not in their own contact list
    let contacts: Vec<String> = pubkey_pool.iter()
        .filter(|pk| *pk != author_pubkey)
        .cloned()
        .collect();
    
    // Create tags for all contacts
    let tags: Vec<Vec<String>> = contacts
        .iter()
        .map(|contact| vec!["p".to_string(), contact.clone()])
        .collect();
    
    let event = generate_signed_event(author_nsec, 3, "", tags)
        .expect("Failed to generate signed event");
    
    EventWithKey {
        event,
        private_key: author_nsec.to_string(),
    }
}

/// Generate a real DM event with recipient from a pool of pubkeys
pub fn generate_fake_dm_event_with_pool(
    rng: &mut impl Rng,
    pubkey_pool: &[String],
    sender_pubkey: &str,
    sender_nsec: &str,
) -> EventWithKey {
    // Generate random plaintext using BIP-39 words
    let plaintext = generate_random_plaintext(rng);
    
    // Select a random recipient from the pool (excluding the sender)
    let available_recipients: Vec<String> = pubkey_pool.iter()
        .filter(|pk| *pk != sender_pubkey)
        .cloned()
        .collect();
    
    let recipient_npub = if available_recipients.is_empty() {
        // Fallback if pool is too small (shouldn't happen with at least 2 pubkeys)
        pubkey_pool[rng.gen_range(0..pubkey_pool.len())].clone()
    } else {
        available_recipients[rng.gen_range(0..available_recipients.len())].clone()
    };
    
    // Parse keys for encryption
    let sender_secret_key = SecretKey::from_bech32(sender_nsec).expect("Failed to parse sender secret key");
    let recipient_public_key = PublicKey::from_bech32(&recipient_npub).expect("Failed to parse recipient public key");
    
    // Encrypt the plaintext message
    let encrypted_content = encrypt_dm_content(&sender_secret_key, &recipient_public_key, &plaintext, rng)
        .expect("Failed to encrypt DM content");
    
    // Create tags with recipient pubkey
    let tags = vec![vec!["p".to_string(), recipient_npub]];
    
    let event = generate_signed_event(sender_nsec, 4, &encrypted_content, tags)
        .expect("Failed to generate signed event");
    
    EventWithKey {
        event,
        private_key: sender_nsec.to_string(),
    }
}

/// Generate multiple fake events of a specific kind with an optional seed
pub fn generate_fake_events_with_seed(kind: u16, count: usize, seed: Option<u64>) -> Vec<EventWithKey> {
    let names = [
        "Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Henry",
        "Iris", "Jack", "Kate", "Liam", "Mia", "Noah", "Olivia", "Paul",
        "Quinn", "Ruby", "Sam", "Tina"
    ];
    
    (0..count)
        .map(|i| {
            // For seeded RNG, we create a new RNG for each event based on the seed + index
            // to ensure deterministic generation
            let mut event_rng: StdRng = if let Some(seed_value) = seed {
                StdRng::seed_from_u64(seed_value.wrapping_add(i as u64))
            } else {
                // For non-seeded case, use thread_rng to seed a new StdRng
                StdRng::from_rng(rand::thread_rng()).unwrap_or_else(|_| StdRng::seed_from_u64(i as u64))
            };
            
            match kind {
                0 => {
                    // For profiles, cycle through names deterministically to ensure unique names
                    let name_idx = i % names.len();
                    let base_name = names[name_idx];
                    let name = if i >= names.len() {
                        // If we've used all names, append last name initial matching the name (e.g., "Alice A", "Bob B")
                        let initial = (name_idx as u8 + b'A') as char;
                        format!("{} {}", base_name, initial)
                    } else {
                        base_name.to_string()
                    };
                    generate_fake_profile_event_with_rng(&mut event_rng, Some(&name))
                },
                3 => {
                    let num_contacts = if seed.is_some() {
                        // Deterministic: use index to determine number of contacts
                        (i % 9) + 1
                    } else {
                        event_rng.gen_range(1..10)
                    };
                    generate_fake_contact_list_event_with_rng(num_contacts, &mut event_rng)
                },
                4 => generate_fake_dm_event_with_rng(&mut event_rng),
                22242 => generate_fake_auth_event_with_rng(&mut event_rng),
                _ => panic!("Unsupported event kind: {}. Only kinds 0, 3, 4, and 22242 are supported.", kind),
            }
        })
        .collect()
}

/// Build a graph structure for conversations where each user participates in at least 3 conversations
/// Returns a vector of (user_i, user_j) pairs representing conversations
/// Requires: N must be even and N >= 4
/// 
/// Construction: For each user i, connect to:
/// - (i + 1) mod N
/// - (i + 2) mod N  
/// - (i + N/2) mod N
/// Then normalize edges to (min, max) format and deduplicate
/// 
/// This ensures each user has at least 3 conversations (some may have more due to symmetry).
fn build_3_regular_graph_edges(n: usize) -> Vec<(usize, usize)> {
    if n < 4 || n % 2 != 0 {
        panic!("For graph construction, N must be even and N >= 4, got N = {}", n);
    }
    
    let mut edges = std::collections::HashSet::new();
    
    // For each user i, connect to the three specified neighbors
    // Normalize edges to (min, max) format to handle wrap-around and deduplicate
    for i in 0..n {
        let neighbor1 = (i + 1) % n;
        let neighbor2 = (i + 2) % n;
        let neighbor3 = (i + n / 2) % n;
        
        // Add edges normalized to (min, max) format
        edges.insert((i.min(neighbor1), i.max(neighbor1)));
        edges.insert((i.min(neighbor2), i.max(neighbor2)));
        edges.insert((i.min(neighbor3), i.max(neighbor3)));
    }
    
    // Verify each node has at least degree 3
    let mut degrees = vec![0; n];
    for (u, v) in &edges {
        degrees[*u] += 1;
        degrees[*v] += 1;
    }
    for (i, &deg) in degrees.iter().enumerate() {
        if deg < 3 {
            panic!("Graph construction failed: node {} has degree {}, expected at least 3", i, deg);
        }
    }
    
    // Convert to sorted vector
    let mut edges_vec: Vec<(usize, usize)> = edges.into_iter().collect();
    edges_vec.sort();
    
    edges_vec
}

/// Generate 4 messages for a conversation following the alternating pattern:
/// Message 1: u → v
/// Message 2: v → u
/// Message 3: u → v
/// Message 4: v → u
fn generate_conversation_messages(
    u_idx: usize,
    v_idx: usize,
    pubkey_pool: &[(String, String)],
    seed: Option<u64>,
    conversation_idx: usize,
) -> Vec<EventWithKey> {
    let (u_pubkey, u_nsec) = &pubkey_pool[u_idx];
    let (v_pubkey, v_nsec) = &pubkey_pool[v_idx];
    
    let mut messages = Vec::new();
    
    // Generate 4 messages alternating between u and v
    for msg_num in 0..4 {
        // Determine sender and recipient for this message
        let (sender_idx, sender_nsec, recipient_pubkey) = if msg_num % 2 == 0 {
            // Messages 0 and 2: u → v
            (u_idx, u_nsec, v_pubkey)
        } else {
            // Messages 1 and 3: v → u
            (v_idx, v_nsec, u_pubkey)
        };
        
        // Create a seeded RNG for this specific message
        // Use conversation_idx and msg_num to ensure determinism
        let seed_value = seed.unwrap_or(0);
        let message_seed = seed_value
            .wrapping_add(conversation_idx as u64 * 1000)
            .wrapping_add(msg_num as u64);
        let mut rng = StdRng::seed_from_u64(message_seed);
        
        // Generate random plaintext using BIP-39 words
        let plaintext = generate_random_plaintext(&mut rng);
        
        // Parse keys for encryption
        let sender_secret_key = SecretKey::from_bech32(sender_nsec)
            .expect("Failed to parse sender secret key");
        let recipient_public_key = PublicKey::from_bech32(recipient_pubkey)
            .expect("Failed to parse recipient public key");
        
        // Encrypt the plaintext message
        let encrypted_content = encrypt_dm_content(&sender_secret_key, &recipient_public_key, &plaintext, &mut rng)
            .expect("Failed to encrypt DM content");
        
        // Create tags with recipient pubkey
        let tags = vec![vec!["p".to_string(), recipient_pubkey.clone()]];
        
        let event = generate_signed_event(sender_nsec, 4, &encrypted_content, tags)
            .expect("Failed to generate signed event");
        
        messages.push(EventWithKey {
            event,
            private_key: sender_nsec.clone(),
        });
    }
    
    messages
}

/// Generate multiple fake events of a specific kind using a pool of pubkeys
/// pubkey_pool is a Vec of (pubkey, private_key) tuples
/// For kind 4 (DMs), uses a 3-regular graph structure where:
/// - Each user participates in exactly 3 conversations
/// - Each conversation has 4 messages (2 from each user, alternating)
pub fn generate_fake_events_with_pool(
    kind: u16, 
    count: usize, 
    seed: Option<u64>,
    pubkey_pool: &[(String, String)],
) -> Vec<EventWithKey> {
    if pubkey_pool.is_empty() {
        panic!("pubkey_pool cannot be empty");
    }
    
    match kind {
        4 => {
            // For DMs, use 3-regular graph structure
            let n = pubkey_pool.len();
            
            // Validate N is even and >= 4
            if n < 4 || n % 2 != 0 {
                panic!("For 3-regular graph DM generation, number of users must be even and >= 4, got {}", n);
            }
            
            // Build the graph edges (conversations)
            let edges = build_3_regular_graph_edges(n);
            
            // Generate 4 messages for each conversation
            let mut all_messages = Vec::new();
            for (conversation_idx, (u_idx, v_idx)) in edges.iter().enumerate() {
                let conversation_messages = generate_conversation_messages(
                    *u_idx,
                    *v_idx,
                    pubkey_pool,
                    seed,
                    conversation_idx,
                );
                all_messages.extend(conversation_messages);
            }
            
            all_messages
        },
        _ => {
            // For other kinds, use the original approach
            (0..count)
                .map(|i| {
                    // For seeded RNG, we create a new RNG for each event based on the seed + index
                    let mut event_rng: StdRng = if let Some(seed_value) = seed {
                        StdRng::seed_from_u64(seed_value.wrapping_add(i as u64))
                    } else {
                        StdRng::from_rng(rand::thread_rng()).unwrap_or_else(|_| StdRng::seed_from_u64(i as u64))
                    };
                    
                    match kind {
                        0 => {
                            // Kind 0 should already be generated, but if called, generate normally
                            generate_fake_profile_event_with_rng(&mut event_rng, None)
                        },
                        3 => {
                            // Select an author from the pool - cycle through all profiles to ensure each gets a contact list
                            let author_idx = i % pubkey_pool.len();
                            let (author_pubkey, author_nsec) = &pubkey_pool[author_idx];
                            
                            // Generate contact list with all other profiles (num_contacts parameter is ignored)
                            let pubkeys: Vec<String> = pubkey_pool.iter().map(|(pk, _)| pk.clone()).collect();
                            generate_fake_contact_list_event_with_pool(
                                0, // Ignored - we include all other profiles
                                &mut event_rng,
                                &pubkeys,
                                author_pubkey,
                                author_nsec,
                            )
                        },
                        22242 => generate_fake_auth_event_with_rng(&mut event_rng),
                        _ => panic!("Unsupported event kind: {}. Only kinds 0, 3, 4, and 22242 are supported.", kind),
                    }
                })
                .collect()
        }
    }
}


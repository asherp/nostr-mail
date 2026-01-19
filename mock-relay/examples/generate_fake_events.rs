// Example: Generating fake events for testing

use mock_relay::test_utils::*;

fn main() {
    println!("Generating fake Nostr events for testing...\n");

    // Generate individual fake events
    println!("1. Individual fake events:");
    let profile = generate_fake_profile_event();
    println!("   Profile: pubkey={}, name in content: {}", 
        &profile.pubkey[..20], 
        profile.content.contains("name"));

    let contacts = generate_fake_contact_list_event(5);
    println!("   Contact list: {} contacts", contacts.tags.len());

    let dm = generate_fake_dm_event();
    println!("   DM: content length={}", dm.content.len());

    // Generate multiple events of a specific kind
    println!("\n2. Generating 5 fake profile events:");
    let profiles = generate_fake_events(0, 5);
    println!("   Generated {} profile events", profiles.len());
    for (i, event) in profiles.iter().enumerate() {
        println!("   {}. pubkey: {}", i + 1, &event.pubkey[..20]);
    }

    // Generate a mix of events
    println!("\n3. Generating mixed events (3 profiles, 2 contacts, 5 DMs):");
    let mix = generate_fake_event_mix(3, 2, 5);
    println!("   Generated {} total events", mix.len());
    
    let profile_count = mix.iter().filter(|e| e.kind == 0).count();
    let contact_count = mix.iter().filter(|e| e.kind == 3).count();
    let dm_count = mix.iter().filter(|e| e.kind == 4).count();
    
    println!("   - {} profile events (kind 0)", profile_count);
    println!("   - {} contact list events (kind 3)", contact_count);
    println!("   - {} DM events (kind 4)", dm_count);

    // Generate fake identifiers
    println!("\n4. Fake identifiers:");
    println!("   Pubkey: {}", generate_fake_pubkey());
    println!("   Event ID: {}", generate_fake_event_id());
    println!("   Signature: {}", generate_fake_sig());

    println!("\nDone! These events can be used for testing your nostr client.");
}

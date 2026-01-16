// Binary to verify Nostr signatures from command line
// Usage: cargo run --bin verify_sig -- <public_key> <signature> <data>
use nostr_mail_lib::crypto;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() != 4 {
        eprintln!("Usage: verify_sig <public_key> <signature> <data>");
        eprintln!("Example: verify_sig npub1... 5d827217... 'encrypted body content'");
        std::process::exit(1);
    }
    
    let public_key = &args[1];
    let signature = &args[2];
    let data = &args[3];
    
    println!("Verifying signature...");
    println!("Public Key: {}", public_key);
    println!("Signature: {}", signature);
    println!("Data length: {} bytes", data.len());
    println!("Data (first 100 chars): {}", &data.chars().take(100).collect::<String>());
    
    match crypto::verify_signature(public_key, signature, data) {
        Ok(valid) => {
            if valid {
                println!("✓ Signature is VALID");
                std::process::exit(0);
            } else {
                println!("✗ Signature is INVALID");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error verifying signature: {}", e);
            std::process::exit(1);
        }
    }
}

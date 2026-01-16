use nostr_mail_lib::crypto::validate_public_key;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: validate_pubkey <npub...>");
        std::process::exit(1);
    }
    let pubkey = &args[1];
    match validate_public_key(pubkey) {
        Ok(true) => {
            println!("Valid Nostr public key.");
            std::process::exit(0);
        }
        Ok(false) => {
            println!("Invalid Nostr public key.");
            std::process::exit(2);
        }
        Err(e) => {
            println!("Error: {:?}", e);
            std::process::exit(3);
        }
    }
} 
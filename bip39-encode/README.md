# BIP39 Encode

A proof-of-concept Rust program that generates natural-looking sentences using a context-free grammar (CFG), while embedding BIP39 mnemonic words in-order within the generated text.

## Overview

This tool uses a small, controlled grammar to generate sentences that embed BIP39 words in their natural positions based on part-of-speech (POS) tags. The cover lexicon (non-payload words) is carefully constructed to avoid any BIP39 words, making decoding trivial: simply filter out all words that are not in the BIP39 word list.

## Features

- **CFG-based sentence generation**: Uses a small context-free grammar to generate grammatically correct sentences
- **POS-tagged payload embedding**: Embeds BIP39 words based on their part-of-speech tags
- **SFW cover lexicon**: Uses a safe-for-work lexicon that excludes all BIP39 words
- **Adaptive sentence length**: Adjusts sentence complexity based on remaining payload tokens

## Usage

```bash
cargo run
```

## How It Works

1. **Payload tokens**: BIP39 words are tagged with allowed POS categories (Noun, Verb, Adjective, etc.)
2. **Grammar expansion**: The CFG generates a stream of POS slots
3. **Slot filling**: Payload tokens are embedded when they fit a slot's POS, otherwise cover words are used
4. **Decoding**: Extract BIP39 words by filtering the output against the BIP39 word list

## Example Output

```
A abandon ability able wallet checks this simple clear note about the above absent steady wallet. A steady bright quiet user sends some steady bright clear server. That clear bright simple wallet holds each bright simple quiet secure wallet to this user.
```

The embedded BIP39 words in this example are: `abandon`, `ability`, `able`, `about`, `above`, `absent`.

## Project Structure

- `src/main.rs`: Main implementation with CFG grammar, lexicon, and generation logic
- `Cargo.toml`: Rust project configuration with dependencies

## Dependencies

- `rand = "0.8"`: For random selection of cover words and grammar productions

## Notes

- The current lexicon is minimal and should be expanded for production use
- The grammar is intentionally simple and can be extended for more variety
- Payload words that don't fit available slots will trigger a warning

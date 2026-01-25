# BIP39 Encode

A proof-of-concept Rust program that generates natural-looking sentences using a context-free grammar (CFG), while embedding BIP39 mnemonic words in-order within the generated text.

## Overview

This tool uses a small, controlled grammar to generate sentences that embed BIP39 words in their natural positions based on part-of-speech (POS) tags. The cover lexicon (non-payload words) is carefully constructed to avoid any BIP39 words, making decoding trivial: simply filter out all words that are not in the BIP39 word list.

## Features

- **CFG-based sentence generation**: Uses a small context-free grammar to generate grammatically correct sentences
- **POS-tagged payload embedding**: Embeds BIP39 words based on their part-of-speech tags
- **SFW cover lexicon**: Uses a safe-for-work lexicon that excludes all BIP39 words
- **Adaptive sentence length**: Adjusts sentence complexity based on remaining payload tokens
- **Word frequency analysis**: Tool to generate word lists from frequency data with POS tags

## Usage

### Main Program

```bash
cargo run
```

### Word Frequency Tool

Generate word lists from frequency data:

```bash
# Download and use COCA word frequency data (recommended)
cargo run --bin get_top_words -- -n 1000 --download-coca -o output.txt

# Use cached file (faster on subsequent runs)
cargo run --bin get_top_words -- -n 1000 --download-coca -o output.txt

# Use a local wordfrequency.info format file
cargo run --bin get_top_words -- -n 1000 --wordfreq lemmas_60k.txt -o output.txt

# Use a CSV frequency file
cargo run --bin get_top_words -- -n 1000 --csv word-freq.csv -o output.txt

# Force re-download of cached data
cargo run --bin get_top_words -- -n 1000 --download-coca --force-download -o output.txt
```

### POS Tagging Tool

Assign parts of speech to words using nlprule:

```bash
# Tag a word list (one word per line)
cargo run --bin tag_words -- -i input_words.txt -o output_POS.txt

# Use alternative (faster) tagging method
cargo run --bin tag_words -- -i input_words.txt -o output_POS.txt --alternative
```

**Workflow: Generate shortest words and tag them:**

```bash
# Step 1: Generate shortest words (3-4 characters) from frequency data
cargo run --bin get_top_words -- \
  -n 500 \
  --download-coca \
  --min-length 3 \
  --max-length 4 \
  --words-only \
  -o shortest_words.txt

# Step 2: Tag those words with POS using nlprule
cargo run --bin tag_words -- \
  -i shortest_words.txt \
  -o shortest_words_POS.txt
```

**Using Docker:**

```bash
# Build the Docker image
docker build -t bip39-encode ./bip39-encode

# Generate shortest words and tag them (all in one workflow)
docker compose run --rm bip39-encode sh -c "
  cargo run --bin get_top_words -- -n 500 --download-coca --min-length 3 --max-length 4 --words-only -o shortest_words.txt &&
  cargo run --bin tag_words -- -i shortest_words.txt -o shortest_words_POS.txt
"

# Or run individually
docker compose run --rm bip39-encode cargo run --bin get_top_words -- \
  -n 500 --download-coca --min-length 3 --max-length 4 --words-only -o shortest_words.txt

docker compose run --rm bip39-encode cargo run --bin tag_words -- \
  -i shortest_words.txt -o shortest_words_POS.txt
```

The `get_top_words` tool:
- Downloads word frequency data from [wordfrequency.info](https://www.wordfrequency.info/samples.asp) (COCA corpus)
- Caches the download locally (reuses for 7 days)
- Filters for words with 6 or fewer characters (no punctuation)
- Extracts POS tags when available
- Outputs in `cover_POS.txt` format: `word|POS1,POS2` (or just `word` if no POS tags)
- Sorts by frequency and returns the top N most common words

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
- `src/bin/get_top_words.rs`: Word frequency analysis tool for generating word lists
- `cover_POS.txt`: Cover lexicon with POS tags (format: `word|POS1,POS2`)
- `bip39_POS.txt`: BIP39 word list with POS tags
- `Cargo.toml`: Rust project configuration with dependencies

## Dependencies

- `rand = "0.8"`: For random selection of cover words and grammar productions
- `nlprule = "0.6"`: For natural language processing
- `anyhow = "1.0"`: For error handling
- `reqwest = "0.11"`: For downloading word frequency data (get_top_words)
- `clap = "4.4"`: For command-line argument parsing (get_top_words)
- `regex = "1.10"`: For POS tag parsing (get_top_words)
- `flate2 = "1.0"`: For reading gzipped Ngram files (get_top_words)
- `csv = "1.3"`: For parsing CSV frequency files (get_top_words)

## Data Sources

The `get_top_words` tool uses word frequency data from:
- **COCA (Corpus of Contemporary American English)**: Available from [wordfrequency.info](https://www.wordfrequency.info/samples.asp)
- **Google Books Ngram**: Can process downloaded Ngram 1-gram files

The downloaded data is cached locally as `lemmas_60k.txt` in the current directory (or temp directory) and reused for 7 days before automatically refreshing.

## Notes

- The current lexicon is minimal and should be expanded for production use
- The grammar is intentionally simple and can be extended for more variety
- Payload words that don't fit available slots will trigger a warning
- Word frequency data is cached locally to avoid repeated downloads
- The `get_top_words` tool outputs words in the same format as `cover_POS.txt` for easy integration
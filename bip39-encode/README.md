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
# Default: subject grammar + compact length-mode
cargo run -- --random 12

# Body grammar defaults to natural length-mode (sample from grammar length distribution)
cargo run -- --random 12 --grammar body

# You can always override explicitly
cargo run -- --random 12 --grammar body --length-mode compact

# Provide words directly instead of random selection
cargo run -- abandon ability able about above absent

# Use a specific language (default: english)
cargo run -- --random 12 --language english

# Generate multiple variations and select the most compact
cargo run -- --random 12 --variations 5

# Use a seed for reproducible output
cargo run -- --random 12 --seed 12345

# Show grammar rules
cargo run -- --show-grammar --grammar body

# Verbose output for debugging
cargo run -- --random 12 --verbose
```

#### Command-Line Options

- `--random <N>`: Generate sentences from N random BIP39 words
- `--grammar <grammar>`: Grammar to use: `subject` (default) or `body`
  - `subject`: Short sentences, may include prefixes (Re:, Fwd:, etc.)
  - `body`: Longer sentences, no prefixes
- `--highlight <mode>`: Highlight BIP39 words: `none`, `bars` (default), or `highlight`
- `--seed <N>`: Seed for deterministic random generation
- `--variations <N>`: Generate N variations and select the most compact (default: 1)
- `--language, -l <lang>`: Language for wordlist: `english` (default), `french`, `german`
- `--k-min <N>`: Minimum sentence length in POS slots including Dot (default: 3)
- `--k-max <N>`: Maximum sentence length in POS slots including Dot (default: 20)
- `--length-mode <mode>`: Sentence length selection: `compact` or `natural`
  - Default: `subject` → `compact`, `body` → `natural`
  - `compact`: Try k from k_min to k_max, shortest first
  - `natural`: Sample k from grammar's length distribution
- `--show-grammar`: Display the grammar rules (then continue execution)
- `--verbose, -v`: Show detailed debugging information
- `--help, -h`: Show help message

### Word Frequency Tool

Generate word lists from frequency data:

```bash
# Download and use COCA word frequency data (recommended)
cargo run --bin get_top_words -- -n 1000 --download-coca -o output.txt

# Re-running the same command will reuse the cached download automatically
# (use --force-download to refresh)
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

## Compact vs Natural (sentence length strategy)

There are (at least) two reasonable ways to drive sentence generation, depending on the UX you want:

- **Compact mode (minimize length)**:
  - Generate the *shortest possible* sentence that can fit the payload.
  - Practical approach: try \(k = k_{\min}, k_{\min}+1, \dots\) and stop at the first feasible \(k\).
  - Within that fixed \(k\), pick the highest-probability sequence (tie-break by probability for readability).

- **Natural mode (maximize grammatical “naturalness”)**:
  - Prefer sentence lengths and POS sequences that are more probable under the grammar.
  - Practical approach: sample (or choose) \(k\) from the grammar’s length distribution, then sample a POS sequence proportionally to its probability.

If you want a single continuous “compactness ↔ naturalness” knob, a simple scoring function works well:

\[
\text{score} = \log P(\text{sequence}) - \lambda \cdot \text{length}
\]

Higher \(\lambda\) yields shorter, more compact text; lower \(\lambda\) yields more natural (but sometimes longer) text.

**CLI default behavior:** if you don't pass `--length-mode`, the program defaults to:
- `--grammar subject` → `compact`
- `--grammar body` → `natural`

## Grammar Files

Grammars are defined in CFG format files located in `languages/{language}/` directories:
- `subject.cfg`: Grammar for email subject lines (shorter, may include prefixes)
- `body.cfg`: Grammar for email body text (longer, more natural sentences)

The grammar parser uses the Pest parser generator to parse these CFG files. Grammar rules support weighted productions for probabilistic selection.

### Grammar note: unbounded PP chaining via VP

The English grammar (`languages/english/body.cfg`) is set up so that **VP can optionally end with one-or-more PPs** (e.g., “… in the house on the hill …"). This removes the previous hard maximum sentence length (formerly capped at 13 POS terminals) while keeping `PP` itself as a simple unit (`Prep NP`).

## Example Output

```
A abandon ability able wallet checks this simple clear note about the above absent steady wallet. A steady bright quiet user sends some steady bright clear server. That clear bright simple wallet holds each bright simple quiet secure wallet to this user.
```

The embedded BIP39 words in this example are: `abandon`, `ability`, `able`, `about`, `above`, `absent`.

## Project Structure

- `src/main.rs`: Main implementation with CFG grammar, lexicon, and generation logic
- `src/lib.rs`: Library module providing `GrammarChecker` for nlprule integration
- `src/grammar.rs`: Grammar parser and CFG implementation using pest
- `src/grammar_parser.pest`: Pest grammar definition for parsing CFG files
- `src/bin/get_top_words.rs`: Word frequency analysis tool for generating word lists
- `src/bin/tag_words.rs`: POS tagging tool using nlprule
- `languages/english/subject.cfg`: CFG grammar definition for subject lines
- `languages/english/body.cfg`: CFG grammar definition for body text
- `languages/english/cover_POS.txt`: Cover lexicon with POS tags (format: `word|POS1,POS2`)
- `languages/english/english_bip39_POS.txt`: BIP39 word list with POS tags
- `Cargo.toml`: Rust project configuration with dependencies

## Dependencies

- `rand = "0.8"`: For random selection of cover words and grammar productions
- `nlprule = "0.6"`: For natural language processing and POS tagging
- `anyhow = "1.0"`: For error handling
- `pest = "2.7"`: For parsing CFG grammar files
- `pest_derive = "2.7"`: Derive macro for pest parser
- `serde = "1.0"`: Serialization framework
- `serde_json = "1.0"`: JSON support for serde
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

## Language Support

The program supports multiple languages via the `--language` flag. Each language requires:
- A grammar file: `languages/{language}/{subject,body}.cfg`
- A POS-tagged BIP39 wordlist: `languages/{language}/{language}_bip39_POS.txt`
- A cover lexicon: `languages/{language}/cover_POS.txt`

Currently supported languages:
- `english` (default)
- `french` (requires POS-tagged wordlist)
- `german` (requires POS-tagged wordlist)

## Notes

- The current lexicon is minimal and should be expanded for production use
- The grammar is intentionally simple and can be extended for more variety
- Payload words that don't fit available slots will trigger a warning
- Word frequency data is cached locally to avoid repeated downloads
- The `get_top_words` tool outputs words in the same format as `cover_POS.txt` for easy integration
- Grammar files use Pest parser syntax - see `src/grammar_parser.pest` for the grammar definition
- The `--mode` flag is deprecated in favor of `--grammar`
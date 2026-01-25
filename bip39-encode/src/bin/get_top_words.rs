//! Script to get the top N most common words with 6 or fewer characters
//! from word frequency data, formatted for BIP39 encode usage.
//!
//! Output format: word|POS1,POS2 (matching cover_POS.txt format)
//!
//! Supports multiple data sources:
//! 1. COCA word frequency data from wordfrequency.info (recommended)
//! 2. Google Books Ngram 1-gram files
//! 3. CSV frequency files
//!
//! Data source: https://www.wordfrequency.info/samples.asp

use clap::Parser;
use flate2::read::GzDecoder;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct WordData {
    freq: f64,
    pos: HashSet<String>,
}

/// Normalize POS tags to simplified format used in cover_POS.txt
fn normalize_pos(pos_str: &str) -> HashSet<String> {
    let mut pos_tags = HashSet::new();
    let pos_lower = pos_str.trim().to_lowercase();
    
    // Noun
    if Regex::new(r"\bn\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("noun") {
        pos_tags.insert("N".to_string());
    }
    
    // Verb
    if Regex::new(r"\bv\.?\s*(t\.?|i\.?)?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("verb") {
        pos_tags.insert("V".to_string());
    }
    
    // Adjective
    if Regex::new(r"\ba\.?\b").unwrap().is_match(&pos_lower)
        || Regex::new(r"\badj\.?\b").unwrap().is_match(&pos_lower)
        || pos_lower.contains("adjective")
    {
        pos_tags.insert("Adj".to_string());
    }
    
    // Adverb
    if Regex::new(r"\badv\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("adverb") {
        pos_tags.insert("Adv".to_string());
    }
    
    // Preposition
    if Regex::new(r"\bprep\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("preposition") {
        pos_tags.insert("Prep".to_string());
    }
    
    // Conjunction
    if Regex::new(r"\bconj\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("conjunction") {
        pos_tags.insert("Conj".to_string());
    }
    
    // Pronoun
    if Regex::new(r"\bpron\.?\b").unwrap().is_match(&pos_lower) || pos_lower.contains("pronoun") {
        pos_tags.insert("Pron".to_string());
    }
    
    // Determiner
    if pos_lower.contains("def. art.") || pos_lower.contains("definite article") || pos_lower.contains("det.") {
        pos_tags.insert("Det".to_string());
    }
    
    pos_tags
}

fn parse_ngram_line(line: &str) -> Option<(String, i64, HashSet<String>)> {
    let parts: Vec<&str> = line.trim().split('\t').collect();
    if parts.len() >= 3 {
        let mut word = parts[0].to_lowercase();
        let mut pos_tags = HashSet::new();
        
        // Extract POS tags if present (word_POS format)
        if let Some(underscore_pos) = word.find('_') {
            let pos_part = word[underscore_pos + 1..].to_string();
            word = word[..underscore_pos].to_string();
            pos_tags = normalize_pos(&pos_part);
        }
        
        if let Ok(match_count) = parts[2].parse::<i64>() {
            return Some((word, match_count, pos_tags));
        }
    }
    None
}

fn process_ngram_file(file_path: &PathBuf) -> anyhow::Result<HashMap<String, WordData>> {
    let mut word_data: HashMap<String, WordData> = HashMap::new();
    
    eprintln!("Processing {:?}...", file_path);
    
    let file = File::open(file_path)?;
    let reader: Box<dyn BufRead> = if file_path.extension().and_then(|s| s.to_str()) == Some("gz") {
        Box::new(BufReader::new(GzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    
    let mut line_count = 0;
    for line_result in reader.lines() {
        let line = line_result?;
        if let Some((word, freq, pos_tags)) = parse_ngram_line(&line) {
            if word.len() <= 6 && word.chars().all(|c| c.is_alphabetic()) {
                let entry = word_data.entry(word).or_insert_with(|| WordData {
                    freq: 0.0,
                    pos: HashSet::new(),
                });
                entry.freq += freq as f64;
                entry.pos.extend(pos_tags);
            }
        }
        
        line_count += 1;
        if line_count % 1_000_000 == 0 {
            eprintln!("  Processed {} lines...", line_count);
        }
    }
    
    eprintln!("Found {} unique words with 6 or fewer characters", word_data.len());
    Ok(word_data)
}

fn download_wordfrequency_data(force_download: bool) -> anyhow::Result<PathBuf> {
    let txt_url = "https://www.wordfrequency.info/samples/lemmas_60k.txt";
    
    // Use a persistent cache file in the current directory or temp dir
    let cache_file = std::env::current_dir()
        .ok()
        .and_then(|dir| {
            let cache = dir.join("lemmas_60k.txt");
            if cache.exists() {
                Some(cache)
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            std::env::temp_dir().join("lemmas_60k.txt")
        });
    
    // Check if cached file exists and is recent (less than 7 days old)
    if !force_download && cache_file.exists() {
        if let Ok(metadata) = std::fs::metadata(&cache_file) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = modified.elapsed() {
                    if age.as_secs() < 7 * 24 * 60 * 60 {
                        eprintln!("Using cached file: {:?}", cache_file);
                        eprintln!("Cache age: {} days", age.as_secs() / (24 * 60 * 60));
                        return Ok(cache_file);
                    }
                }
            }
        }
    }
    
    // Download the file
    eprintln!("Downloading word frequency data from wordfrequency.info...");
    eprintln!("Source: https://www.wordfrequency.info/samples.asp");
    
    let response = reqwest::blocking::get(txt_url)?;
    let content = response.text()?;
    
    std::fs::write(&cache_file, content)?;
    
    eprintln!("Downloaded and cached to {:?}", cache_file);
    eprintln!("This file will be reused for future runs (cache expires after 7 days)");
    Ok(cache_file)
}

fn parse_wordfrequency_line(line: &str) -> Option<(String, f64, HashSet<String>)> {
    // Try different separators
    for sep in &['\t', '|', ','] {
        if line.contains(*sep) {
            let parts: Vec<&str> = line.trim().split(*sep).collect();
            if parts.len() >= 4 {
                if let Ok(_rank) = parts[0].parse::<i32>() {
                    let mut word = parts[1].trim().to_lowercase();
                    let mut pos_str = parts.get(2).map(|s| s.trim().to_string()).unwrap_or_default();
                    
                    // Remove POS tags if present in word (word_POS format)
                    if let Some(underscore_pos) = word.find('_') {
                        let pos_part = word[underscore_pos + 1..].to_string();
                        word = word[..underscore_pos].to_string();
                        if pos_str.is_empty() {
                            pos_str = pos_part;
                        }
                    }
                    
                    // Parse frequency
                    let mut freq = None;
                    for part in parts.iter().skip(3) {
                        if let Ok(f) = part.trim().parse::<f64>() {
                            freq = Some(f);
                            break;
                        }
                    }
                    
                    if let Some(f) = freq {
                        let pos_tags = normalize_pos(&pos_str);
                        return Some((word, f, pos_tags));
                    }
                }
            }
        }
    }
    None
}

fn get_top_words_from_wordfrequency(file_path: &PathBuf) -> anyhow::Result<HashMap<String, WordData>> {
    let mut word_data: HashMap<String, WordData> = HashMap::new();
    
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut header_skipped = false;
    
    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        
        // Skip header lines
        if !header_skipped {
            let line_lower = line.to_lowercase();
            if line_lower.contains("rank") || line_lower.contains("lemma") || line_num < 2 {
                header_skipped = true;
                continue;
            }
        }
        
        if let Some((word, freq, pos_tags)) = parse_wordfrequency_line(&line) {
            if word.len() <= 6 && word.chars().all(|c| c.is_alphabetic()) {
                let entry = word_data.entry(word).or_insert_with(|| WordData {
                    freq: 0.0,
                    pos: HashSet::new(),
                });
                if freq > entry.freq {
                    entry.freq = freq;
                }
                entry.pos.extend(pos_tags);
            }
        }
    }
    
    Ok(word_data)
}

fn get_top_words_from_csv(csv_file: &PathBuf) -> anyhow::Result<HashMap<String, WordData>> {
    let mut word_data: HashMap<String, WordData> = HashMap::new();
    
    let file = File::open(csv_file)?;
    let mut reader = csv::Reader::from_reader(file);
    let mut header_skipped = false;
    
    for result in reader.records() {
        let record: csv::StringRecord = result?;
        
        if !header_skipped {
            header_skipped = true;
            continue;
        }
        
        if record.len() >= 2 {
            let word: String = record.get(0).unwrap().trim().to_lowercase();
            if let Ok(freq) = record.get(1).unwrap().trim().parse::<f64>() {
                if word.len() <= 6 && word.chars().all(|c| c.is_alphabetic()) {
                    let entry = word_data.entry(word).or_insert_with(|| WordData {
                        freq: 0.0,
                        pos: HashSet::new(),
                    });
                    if freq > entry.freq {
                        entry.freq = freq;
                    }
                }
            }
        }
    }
    
    Ok(word_data)
}

#[derive(Parser)]
#[command(
    name = "get_top_words",
    about = "Get top N most common words with 6 or fewer characters from word frequency data",
    long_about = None
)]
struct Args {
    /// Number of top words to return
    #[arg(short = 'n', long = "top-n", default_value_t = 1000)]
    top_n: usize,

    /// Output file path (default: stdout)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Path(s) to Google Books Ngram 1-gram file(s) (.gz or plain text)
    #[arg(long = "ngram")]
    ngram: Option<Vec<PathBuf>>,

    /// Path to CSV frequency file (format: word,frequency)
    #[arg(long = "csv")]
    csv: Option<PathBuf>,

    /// Path to wordfrequency.info format file (lemmas_60k.txt format)
    #[arg(long = "wordfreq")]
    wordfreq: Option<PathBuf>,

    /// Download free COCA word frequency data from wordfrequency.info
    #[arg(long = "download-coca")]
    download_coca: bool,

    /// Force re-download even if cached file exists
    #[arg(long = "force-download")]
    force_download: bool,

    /// Minimum word length (default: 1)
    #[arg(long = "min-length", default_value_t = 1)]
    min_length: usize,

    /// Maximum word length (default: 6)
    #[arg(long = "max-length", default_value_t = 6)]
    max_length: usize,

    /// Output words only (no POS tags), useful for piping to tag_words
    #[arg(long = "words-only")]
    words_only: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    let mut word_data: HashMap<String, WordData> = HashMap::new();
    
    if let Some(ngram_files) = &args.ngram {
        // Process Google Books Ngram file(s)
        for ngram_file in ngram_files {
            if !ngram_file.as_path().exists() {
                eprintln!("Error: File not found: {:?}", ngram_file);
                continue;
            }
            
            let file_data = process_ngram_file(ngram_file)?;
            // Merge data (sum frequencies, merge POS tags)
            for (word, data) in file_data {
                let entry = word_data.entry(word).or_insert_with(|| WordData {
                    freq: 0.0,
                    pos: HashSet::new(),
                });
                entry.freq += data.freq;
                entry.pos.extend(data.pos);
            }
        }
        
        if word_data.is_empty() {
            eprintln!("No words found in Ngram files.");
            std::process::exit(1);
        }
    } else if args.download_coca {
        // Download COCA word frequency data (cached locally)
        let cache_file = download_wordfrequency_data(args.force_download)?;
        word_data = get_top_words_from_wordfrequency(&cache_file)?;
        // Keep the cache file for future use
    } else if let Some(wordfreq_file) = args.wordfreq {
        // Use wordfrequency.info format file
        eprintln!("Reading wordfrequency.info file: {:?}", wordfreq_file);
        word_data = get_top_words_from_wordfrequency(&wordfreq_file)?;
    } else if let Some(csv_file) = args.csv {
        // Use CSV frequency file
        eprintln!("Reading CSV file: {:?}", csv_file);
        word_data = get_top_words_from_csv(&csv_file)?;
    } else {
        eprintln!("Error: Must specify one of: --download-coca, --wordfreq, --csv, or --ngram");
        eprintln!("\nExamples:");
        eprintln!("  # Download and use COCA word frequency data (recommended)");
        eprintln!("  cargo run --bin get_top_words -- -n 1000 --download-coca -o output.txt");
        eprintln!("\n  # Use a wordfrequency.info format file");
        eprintln!("  cargo run --bin get_top_words -- -n 1000 --wordfreq lemmas_60k.txt -o output.txt");
        eprintln!("\n  # Use a CSV frequency file");
        eprintln!("  cargo run --bin get_top_words -- -n 1000 --csv word-freq.csv -o output.txt");
        std::process::exit(1);
    }
    
    if word_data.is_empty() {
        eprintln!("No words found. Check your input files.");
        std::process::exit(1);
    }
    
    // Filter for words by length, no punctuation
    let filtered: HashMap<String, WordData> = word_data
        .into_iter()
        .filter(|(w, _)| {
            let len = w.len();
            len >= args.min_length 
                && len <= args.max_length 
                && w.chars().all(|c| c.is_alphabetic())
        })
        .collect();
    
    // Sort by frequency (descending) and get top N
    let mut sorted_words: Vec<(String, WordData)> = filtered.into_iter().collect();
    sorted_words.sort_by(|a, b| b.1.freq.partial_cmp(&a.1.freq).unwrap());
    sorted_words.truncate(args.top_n);
    
    // Output results in cover_POS.txt format
    let output_path = args.output.clone();
    let mut output: Box<dyn Write> = if let Some(ref path) = output_path {
        Box::new(File::create(path)?)
    } else {
        Box::new(io::stdout())
    };
    
    for (word, data) in &sorted_words {
        if args.words_only {
            // Output just the word, one per line (for piping to tag_words)
            writeln!(output, "{}", word)?;
        } else {
            let mut pos_tags: Vec<String> = data.pos.iter().cloned().collect();
            pos_tags.sort();
            
            if !pos_tags.is_empty() {
                let pos_str = pos_tags.join(",");
                writeln!(output, "{}|{}", word, pos_str)?;
            } else {
                writeln!(output, "{}", word)?;
            }
        }
    }
    
    if let Some(ref path) = output_path {
        let words_with_pos = sorted_words.iter().filter(|(_, d)| !d.pos.is_empty()).count();
        eprintln!("\nTop {} words saved to {:?}", sorted_words.len(), path);
        eprintln!("Words with POS tags: {}/{}", words_with_pos, sorted_words.len());
        if let Some((_, last_data)) = sorted_words.last() {
            if let Some((_, first_data)) = sorted_words.first() {
                eprintln!(
                    "Frequency range: {:.0} to {:.0}",
                    last_data.freq,
                    first_data.freq
                );
            }
        }
    } else {
        eprintln!("\nTotal words: {}", sorted_words.len());
    }
    
    Ok(())
}

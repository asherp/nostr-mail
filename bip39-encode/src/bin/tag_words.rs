//! Tool to assign parts of speech to words using nlprule's tokenizer.
//!
//! This tool reads a word list and uses nlprule to tag each word with its
//! part of speech by testing the word in various sentence contexts.
//!
//! Output format: word|POS1,POS2 (matching cover_POS.txt format)

use clap::Parser;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use bip39_encode::GrammarChecker;

/// Convert nlprule POS tags to our simplified format
fn normalize_nlprule_pos(nlprule_tag: &str) -> Option<&'static str> {
    match nlprule_tag {
        // Nouns
        "NN" | "NNS" | "NNP" | "NNPS" => Some("N"),
        // Verbs
        "VB" | "VBD" | "VBG" | "VBN" | "VBP" | "VBZ" => Some("V"),
        // Adjectives
        "JJ" | "JJR" | "JJS" => Some("Adj"),
        // Adverbs
        "RB" | "RBR" | "RBS" => Some("Adv"),
        // Prepositions
        "IN" => Some("Prep"),
        // Determiners
        "DT" => Some("Det"),
        // Conjunctions
        "CC" => Some("Conj"),
        // Pronouns
        "PRP" | "PRP$" | "WP" | "WP$" => Some("Pron"),
        _ => None,
    }
}

/// Test a word in various sentence contexts to determine its POS tags
fn tag_word_with_nlprule(checker: &GrammarChecker, word: &str) -> HashSet<String> {
    let mut pos_tags = HashSet::new();
    
    // Test contexts for different POS categories
    // We test multiple contexts because words can have multiple POS tags
    let test_sentences = vec![
        // Noun contexts
        format!("The {} works.", word),
        format!("A {} helps.", word),
        format!("This {} is good.", word),
        format!("Many {} help.", word),
        // Verb contexts
        format!("They {} it.", word),
        format!("I {} now.", word),
        format!("We {} here.", word),
        format!("He {} well.", word),
        // Adjective contexts (including slang/informal)
        format!("The {} thing works.", word),
        format!("It is {}.", word),
        format!("A {} item helps.", word),
        format!("Very {} stuff.", word),
        format!("That seems {}.", word),  // For slang adjectives like "suss"
        format!("It looks {}.", word),    // Another adjective context
        // Adverb contexts
        format!("They work {}.", word),
        format!("It runs {}.", word),
        format!("Very {} done.", word),
        // Preposition contexts
        format!("They go {} it.", word),
        format!("We work {} it.", word),
    ];
    
    for sentence in test_sentences {
        for sent in checker.tokenize(&sentence) {
            for token in sent.tokens() {
                // Access text via token.word().text() according to nlprule API
                let token_text = token.word().text().as_str().to_lowercase();
                let word_lower = word.to_lowercase();
                
                // Check if this token matches our word
                if token_text == word_lower {
                    // Extract POS tags from token using nlprule's API
                    // According to docs: token.word().tags()[0].pos()
                    for tag in token.word().tags() {
                        let pos_tag = tag.pos().as_str();
                        if let Some(normalized_pos) = normalize_nlprule_pos(pos_tag) {
                            pos_tags.insert(normalized_pos.to_string());
                        }
                    }
                }
            }
        }
    }
    
    pos_tags
}

/// Alternative approach: Use a single comprehensive sentence to get all POS tags
/// This is more efficient but may miss some contexts
fn tag_word_alternative(checker: &GrammarChecker, word: &str) -> HashSet<String> {
    let mut pos_tags = HashSet::new();
    
    // Create a comprehensive sentence that tests multiple POS roles
    // This sentence structure allows the word to appear in multiple positions
    let comprehensive_sentence = format!("The {} {} {} {} it {} well.", word, word, word, word, word);
    
    for sent in checker.tokenize(&comprehensive_sentence) {
        for token in sent.tokens() {
            // Access text via token.word().text() according to nlprule API
            let token_text = token.word().text().as_str().to_lowercase();
            let word_lower = word.to_lowercase();
            
            // Check if this token matches our word
            if token_text == word_lower {
                // Extract all POS tags for this word occurrence
                for tag in token.word().tags() {
                    let pos_tag = tag.pos().as_str();
                    if let Some(normalized_pos) = normalize_nlprule_pos(pos_tag) {
                        pos_tags.insert(normalized_pos.to_string());
                    }
                }
            }
        }
    }
    
    // Fallback: if no tags found, try individual contexts
    if pos_tags.is_empty() {
        return tag_word_with_nlprule(checker, word);
    }
    
    pos_tags
}

#[derive(Parser)]
#[command(
    name = "tag_words",
    about = "Assign parts of speech to words using nlprule",
    long_about = "Reads a word list and uses nlprule's tokenizer to assign POS tags.\n\
                   Each word is tested in various sentence contexts to determine its part of speech."
)]
struct Args {
    /// Input word list file (one word per line)
    #[arg(short = 'i', long = "input", required = true)]
    input: PathBuf,

    /// Output file path (default: stdout)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Use alternative tagging method (may be more accurate)
    #[arg(long = "alternative")]
    alternative: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    // Initialize grammar checker (required for nlprule)
    eprintln!("Loading nlprule tokenizer and rules...");
    let checker = match GrammarChecker::from_language(bip39_encode::Language::English) {
        Ok(checker) => checker,
        Err(e) => {
            eprintln!("Error: Could not load nlprule data files.");
            eprintln!("Please ensure en_tokenizer.bin and en_rules.bin are available.");
            eprintln!("They should be in: current directory, data/, /app/data/, or /opt/nlprule-data/");
            eprintln!("\nError details: {}", e);
            eprintln!("\nNote: nlprule binary files can be downloaded from:");
            eprintln!("https://github.com/bminixhofer/nlprule");
            return Err(e);
        }
    };
    
    // Read input word list
    eprintln!("Reading word list from {:?}...", args.input);
    let file = File::open(&args.input)?;
    let reader = BufReader::new(file);
    
    let mut words: Vec<String> = Vec::new();
    for line_result in reader.lines() {
        let line = line_result?;
        let word = line.trim().to_lowercase();
        if !word.is_empty() && word.chars().all(|c| c.is_alphabetic()) {
            words.push(word);
        }
    }
    
    eprintln!("Found {} words to tag", words.len());
    
    // Tag each word
    let mut tagged_words: Vec<(String, HashSet<String>)> = Vec::new();
    let mut processed = 0;
    
    for word in words {
        if processed % 100 == 0 && processed > 0 {
            eprintln!("Processed {} words...", processed);
        }
        
        let pos_tags = if args.alternative {
            tag_word_alternative(&checker, &word)
        } else {
            tag_word_with_nlprule(&checker, &word)
        };
        
        tagged_words.push((word, pos_tags));
        processed += 1;
    }
    
    eprintln!("Tagging complete!");
    
    // Output results
    let output_path = args.output.clone();
    let mut output: Box<dyn Write> = if let Some(ref path) = output_path {
        Box::new(File::create(path)?)
    } else {
        Box::new(io::stdout())
    };
    
    let mut words_with_tags = 0;
    for (word, pos_tags) in &tagged_words {
        let mut pos_vec: Vec<String> = pos_tags.iter().cloned().collect();
        pos_vec.sort();
        
        if !pos_vec.is_empty() {
            let pos_str = pos_vec.join(",");
            writeln!(output, "{}|{}", word, pos_str)?;
            words_with_tags += 1;
        } else {
            writeln!(output, "{}", word)?;
        }
    }
    
    if let Some(ref path) = output_path {
        eprintln!("\nResults saved to {:?}", path);
        eprintln!("Words with POS tags: {}/{}", words_with_tags, tagged_words.len());
    } else {
        eprintln!("\nWords with POS tags: {}/{}", words_with_tags, tagged_words.len());
    }
    
    Ok(())
}

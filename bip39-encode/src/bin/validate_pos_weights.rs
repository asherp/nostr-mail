//! Tool to generate POS tag weights from nlprule analysis.
//!
//! This tool reads YAML files containing words with POS tag weights,
//! uses nlprule to tag each word in various contexts, calculates observed
//! POS tag frequencies, and outputs a new YAML file with nlprule's weights.

use clap::Parser;
use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Context;
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
        // Interjections
        "UH" => Some("Intj"),
        // Modal verbs (MD in Penn Treebank)
        "MD" => Some("Modal"),
        // Note: Aux, Cop, To, Prefix are not standard POS tags in nlprule
        // They may need special handling or be inferred from context
        _ => None,
    }
}

/// Test contexts for different POS categories
fn get_test_contexts(word: &str) -> Vec<String> {
    vec![
        // Noun contexts
        format!("The {} works.", word),
        format!("A {} helps.", word),
        format!("This {} is good.", word),
        format!("Many {} help.", word),
        format!("Some {} work.", word),
        format!("Each {} helps.", word),
        // Verb contexts
        format!("They {} it.", word),
        format!("I {} now.", word),
        format!("We {} here.", word),
        format!("He {} well.", word),
        format!("She {} quickly.", word),
        format!("It {} fast.", word),
        // Adjective contexts
        format!("The {} thing works.", word),
        format!("It is {}.", word),
        format!("A {} item helps.", word),
        format!("Very {} stuff.", word),
        format!("That seems {}.", word),
        format!("It looks {}.", word),
        format!("They are {}.", word),
        // Adverb contexts
        format!("They work {}.", word),
        format!("It runs {}.", word),
        format!("Very {} done.", word),
        format!("It moves {}.", word),
        // Preposition contexts
        format!("They go {} it.", word),
        format!("We work {} it.", word),
        format!("It sits {} there.", word),
        // Determiner contexts
        format!("{} thing works.", word),
        format!("{} items help.", word),
        // Conjunction contexts
        format!("This {} that.", word),
        format!("Here {} there.", word),
    ]
}

/// Calculate observed POS tag frequencies for a word using nlprule
fn calculate_observed_weights(
    checker: &GrammarChecker,
    word: &str,
) -> HashMap<String, f64> {
    let mut pos_counts: HashMap<String, usize> = HashMap::new();
    let mut total_count = 0usize;
    
    let contexts = get_test_contexts(word);
    
    for sentence in contexts {
        for sent in checker.tokenize(&sentence) {
            for token in sent.tokens() {
                let token_text = token.word().text().as_str().to_lowercase();
                let word_lower = word.to_lowercase();
                
                if token_text == word_lower {
                    for tag in token.word().tags() {
                        let pos_tag = tag.pos().as_str();
                        if let Some(normalized_pos) = normalize_nlprule_pos(pos_tag) {
                            *pos_counts.entry(normalized_pos.to_string()).or_insert(0) += 1;
                            total_count += 1;
                        }
                    }
                }
            }
        }
    }
    
    // Convert counts to frequencies
    if total_count == 0 {
        return HashMap::new();
    }
    
    pos_counts
        .into_iter()
        .map(|(pos, count)| (pos, count as f64 / total_count as f64))
        .collect()
}

/// Load YAML file and parse word -> POS weights mapping
fn load_yaml_weights(path: &PathBuf) -> anyhow::Result<HashMap<String, HashMap<String, f64>>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read YAML file: {:?}", path))?;
    
    let yaml_data: HashMap<String, HashMap<String, f64>> = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML file: {:?}", path))?;
    
    Ok(yaml_data)
}

/// Round a float to specified decimal places
fn round_to_decimal_places(value: f64, places: usize) -> f64 {
    let multiplier = 10_f64.powi(places as i32);
    (value * multiplier).round() / multiplier
}

#[derive(Parser)]
#[command(
    name = "validate_pos_weights",
    about = "Generate POS tag weights from nlprule analysis",
    long_about = "Reads a YAML file containing words with POS tag weights,\n\
                  uses nlprule to tag each word in various contexts, calculates observed\n\
                  POS tag frequencies, and outputs a new YAML file with nlprule's weights.\n\
                  The output format matches the input YAML structure."
)]
struct Args {
    /// Input YAML file (cover.yaml or payload.yaml)
    #[arg(short = 'f', long = "file", required = true)]
    file: PathBuf,
    
    /// Output YAML file (default: stdout)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    
    /// Minimum weight threshold (weights below this will be omitted, default: 0.01)
    #[arg(short = 't', long = "threshold", default_value = "0.01")]
    min_weight_threshold: f64,
    
    /// Maximum number of words to process (for testing)
    #[arg(short = 'n', long = "max-words")]
    max_words: Option<usize>,
    
    /// Round weights to this many decimal places (default: 3)
    #[arg(short = 'r', long = "round", default_value = "3")]
    decimal_places: usize,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    // Initialize grammar checker
    eprintln!("Loading nlprule tokenizer and rules...");
    let checker = match GrammarChecker::from_language(bip39_encode::Language::English) {
        Ok(checker) => checker,
        Err(e) => {
            eprintln!("Error: Could not load nlprule data files.");
            eprintln!("Please ensure en_tokenizer.bin and en_rules.bin are available.");
            eprintln!("They should be in: current directory, data/, /app/data/, or /opt/nlprule-data/");
            eprintln!("\nError details: {}", e);
            return Err(e);
        }
    };
    
    // Load YAML file to get word list (preserve order)
    eprintln!("Loading words from {:?}...", args.file);
    let all_words = load_yaml_weights(&args.file)?;
    eprintln!("Loaded {} words", all_words.len());
    
    // Limit words if requested
    let words_to_process: Vec<(String, HashMap<String, f64>)> = if let Some(max) = args.max_words {
        all_words.into_iter().take(max).collect()
    } else {
        all_words.into_iter().collect()
    };
    
    eprintln!("Processing {} words...", words_to_process.len());
    
    // Calculate observed weights for each word
    let mut output_weights: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut processed = 0;
    let mut words_without_tags = 0;
    
    for (word, _expected_weights) in words_to_process {
        if processed % 50 == 0 && processed > 0 {
            eprintln!("Processed {} words...", processed);
        }
        
        let observed_weights = calculate_observed_weights(&checker, &word);
        
        // Filter out weights below threshold and round
        let mut filtered_weights: HashMap<String, f64> = HashMap::new();
        for (pos, weight) in observed_weights {
            if weight >= args.min_weight_threshold {
                let rounded = round_to_decimal_places(weight, args.decimal_places);
                filtered_weights.insert(pos, rounded);
            }
        }
        
        // Normalize weights to sum to 1.0
        let total: f64 = filtered_weights.values().sum();
        if total > 0.0 {
            let normalized: HashMap<String, f64> = filtered_weights
                .into_iter()
                .map(|(pos, weight)| (pos, round_to_decimal_places(weight / total, args.decimal_places)))
                .collect();
            output_weights.insert(word, normalized);
        } else {
            // If no weights found, preserve the word with empty weights
            // This ensures all words from input are included in output
            output_weights.insert(word, HashMap::new());
            words_without_tags += 1;
        }
        
        processed += 1;
    }
    
    eprintln!("Processing complete!");
    if words_without_tags > 0 {
        eprintln!("Note: {} words had no POS tags found by nlprule (included with empty weights)", words_without_tags);
    }
    
    // Output YAML
    let yaml_output = serde_yaml::to_string(&output_weights)
        .context("Failed to serialize weights to YAML")?;
    
    if let Some(ref path) = args.output {
        std::fs::write(path, yaml_output)
            .with_context(|| format!("Failed to write output to {:?}", path))?;
        eprintln!("\nYAML output saved to {:?}", path);
        eprintln!("Generated weights for {} words", output_weights.len());
    } else {
        print!("{}", yaml_output);
    }
    
    Ok(())
}

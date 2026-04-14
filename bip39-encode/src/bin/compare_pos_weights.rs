//! Tool to compare POS tag weights between two YAML files.
//!
//! This tool reads two YAML files containing words with POS tag weights,
//! compares them word by word, and outputs a third YAML file with the
//! differences (file1_weight - file2_weight) for each POS tag.

use clap::Parser;
use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Context;
use serde_yaml;
use std::collections::BTreeMap;

/// Load YAML file and parse word -> POS weights mapping
fn load_yaml_weights(path: &PathBuf) -> anyhow::Result<HashMap<String, HashMap<String, f64>>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read YAML file: {:?}", path))?;
    
    let yaml_data: HashMap<String, HashMap<String, f64>> = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML file: {:?}", path))?;
    
    Ok(yaml_data)
}

/// Compare two weight maps and calculate differences
fn calculate_differences(
    weights1: &HashMap<String, f64>,
    weights2: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    let mut differences: HashMap<String, f64> = HashMap::new();
    
    // Get all POS tags from both files
    let mut all_tags: Vec<String> = weights1.keys().cloned().collect();
    for tag in weights2.keys() {
        if !all_tags.contains(tag) {
            all_tags.push(tag.clone());
        }
    }
    
    // Calculate difference for each POS tag
    for tag in all_tags {
        let weight1 = weights1.get(&tag).copied().unwrap_or(0.0);
        let weight2 = weights2.get(&tag).copied().unwrap_or(0.0);
        let diff = weight1 - weight2;
        
        // Only include non-zero differences
        if diff.abs() > 1e-10 {
            differences.insert(tag, diff);
        }
    }
    
    differences
}

#[derive(Parser)]
#[command(
    name = "compare_pos_weights",
    about = "Compare POS tag weights between two YAML files",
    long_about = "Reads two YAML files containing words with POS tag weights,\n\
                  compares them word by word, and outputs a third YAML file with\n\
                  the differences (file1_weight - file2_weight) for each POS tag.\n\
                  Output is sorted alphabetically by word."
)]
struct Args {
    /// First YAML file (file1)
    #[arg(short = '1', long = "file1", required = true)]
    file1: PathBuf,
    
    /// Second YAML file (file2)
    #[arg(short = '2', long = "file2", required = true)]
    file2: PathBuf,
    
    /// Output YAML file with differences
    #[arg(short = 'o', long = "output", required = true)]
    output: PathBuf,
    
    /// Round differences to this many decimal places (default: 3)
    #[arg(short = 'r', long = "round", default_value = "3")]
    decimal_places: usize,
    
    /// Only include words that exist in both files (default: false, includes all words)
    #[arg(short = 'b', long = "both-only")]
    both_only: bool,
}

/// Round a float to specified decimal places
fn round_to_decimal_places(value: f64, places: usize) -> f64 {
    let multiplier = 10_f64.powi(places as i32);
    (value * multiplier).round() / multiplier
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    // Load both YAML files
    eprintln!("Loading file1 from {:?}...", args.file1);
    let weights1 = load_yaml_weights(&args.file1)?;
    eprintln!("Loaded {} words from file1", weights1.len());
    
    eprintln!("Loading file2 from {:?}...", args.file2);
    let weights2 = load_yaml_weights(&args.file2)?;
    eprintln!("Loaded {} words from file2", weights2.len());
    
    // Get all words from both files
    let mut all_words: Vec<String> = weights1.keys().cloned().collect();
    for word in weights2.keys() {
        if !all_words.contains(word) {
            all_words.push(word.clone());
        }
    }
    
    eprintln!("Found {} unique words total", all_words.len());
    
    // Calculate differences for each word
    let mut differences: BTreeMap<String, HashMap<String, f64>> = BTreeMap::new();
    let mut words_in_both = 0;
    let mut words_only_in_file1 = 0;
    let mut words_only_in_file2 = 0;
    
    // Nuance statistics (POS tag diversity)
    let mut file1_more_nuanced = 0;
    let mut file2_more_nuanced = 0;
    let mut same_nuance = 0;
    let mut file1_total_tags = 0;
    let mut file2_total_tags = 0;
    let mut words_compared = 0;
    
    for word in all_words {
        let w1 = weights1.get(&word);
        let w2 = weights2.get(&word);
        
        let (has_w1, has_w2) = (w1.is_some(), w2.is_some());
        
        // Skip if both-only flag is set and word is not in both files
        if args.both_only && (!has_w1 || !has_w2) {
            continue;
        }
        
        if has_w1 && has_w2 {
            words_in_both += 1;
            
            // Calculate nuance statistics for words in both files
            let tags1 = w1.unwrap().len();
            let tags2 = w2.unwrap().len();
            
            file1_total_tags += tags1;
            file2_total_tags += tags2;
            words_compared += 1;
            
            if tags1 > tags2 {
                file1_more_nuanced += 1;
            } else if tags2 > tags1 {
                file2_more_nuanced += 1;
            } else {
                same_nuance += 1;
            }
        } else if has_w1 {
            words_only_in_file1 += 1;
        } else {
            words_only_in_file2 += 1;
        }
        
        let weights1_map = w1.cloned().unwrap_or_default();
        let weights2_map = w2.cloned().unwrap_or_default();
        
        let word_differences = calculate_differences(&weights1_map, &weights2_map);
        
        // Round differences
        let rounded_differences: HashMap<String, f64> = word_differences
            .into_iter()
            .map(|(pos, diff)| (pos, round_to_decimal_places(diff, args.decimal_places)))
            .filter(|(_, diff)| diff.abs() > 1e-10) // Filter out effectively zero differences
            .collect();
        
        // Only include words with non-zero differences
        if !rounded_differences.is_empty() {
            differences.insert(word, rounded_differences);
        }
    }
    
    eprintln!("Words in both files: {}", words_in_both);
    eprintln!("Words only in file1: {}", words_only_in_file1);
    eprintln!("Words only in file2: {}", words_only_in_file2);
    eprintln!("Words with non-zero differences: {}", differences.len());
    
    let avg_tags_file1 = if words_compared > 0 {
        file1_total_tags as f64 / words_compared as f64
    } else {
        0.0
    };
    let avg_tags_file2 = if words_compared > 0 {
        file2_total_tags as f64 / words_compared as f64
    } else {
        0.0
    };
    
    eprintln!("\nNuance Analysis (POS tag diversity):");
    eprintln!("  Average POS tags per word:");
    eprintln!("    File1: {:.2}", avg_tags_file1);
    eprintln!("    File2: {:.2}", avg_tags_file2);
    eprintln!("  Words with more POS tags:");
    eprintln!("    File1 more nuanced: {} words", file1_more_nuanced);
    eprintln!("    File2 more nuanced: {} words", file2_more_nuanced);
    eprintln!("    Same nuance: {} words", same_nuance);
    
    if avg_tags_file1 > avg_tags_file2 {
        eprintln!("  → File1 has more nuance overall (more POS tag diversity)");
    } else if avg_tags_file2 > avg_tags_file1 {
        eprintln!("  → File2 has more nuance overall (more POS tag diversity)");
    } else {
        eprintln!("  → Both files have similar nuance");
    }
    
    // Use BTreeMap directly to ensure alphabetical ordering
    // serde_yaml should preserve the order from BTreeMap
    let yaml_output = serde_yaml::to_string(&differences)
        .context("Failed to serialize differences to YAML")?;
    
    std::fs::write(&args.output, yaml_output)
        .with_context(|| format!("Failed to write output to {:?}", args.output))?;
    
    eprintln!("\nDifferences saved to {:?}", args.output);
    eprintln!("Output contains {} words with non-zero differences", differences.len());
    
    Ok(())
}

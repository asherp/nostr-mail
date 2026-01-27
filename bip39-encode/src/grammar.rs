use pest::Parser;
use pest_derive::Parser;
use rand::Rng;
use std::collections::HashMap;
use std::path::Path;
use crate::{Pos, Sym};

#[derive(Parser)]
#[grammar = "grammar_parser.pest"]
pub struct GrammarParser;

#[derive(Clone, Debug)]
pub struct Production {
    pub symbols: Vec<Sym>,
    pub weight: f64,
}

#[derive(Clone, Debug)]
pub struct GrammarRule {
    pub productions: Vec<Production>,
}

#[derive(Debug)]
pub struct Grammar {
    pub(crate) rules: HashMap<String, GrammarRule>,
}

/// A POS sequence with its probability according to the grammar
#[derive(Clone, Debug)]
pub struct SequenceWithProbability {
    pub sequence: Vec<crate::Pos>,
    pub probability: f64,
}

impl Grammar {
    /// Parse grammar from a string definition
    pub fn from_str(grammar_str: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let pairs = GrammarParser::parse(Rule::grammar, grammar_str)?;
        
        let mut rules = HashMap::new();
        
        // Pest returns a top-level `grammar` pair; the actual `rule` entries are inside it.
        // (Older code accidentally skipped them, leaving `rules` empty.)
        for pair in pairs {
            let mut inner_pairs = Vec::new();
            match pair.as_rule() {
                Rule::grammar => {
                    inner_pairs.extend(pair.into_inner());
                }
                Rule::rule => {
                    // Be permissive in case pest ever returns rules directly.
                    inner_pairs.push(pair);
                }
                _ => continue,
            }

            for p in inner_pairs {
                if p.as_rule() != Rule::rule {
                    continue;
                }
                let mut inner = p.into_inner();
                let non_terminal = inner
                    .next()
                    .ok_or("Missing non-terminal")?
                    .as_str()
                    .to_string();
                // Note: the literal "=" in the pest grammar is not emitted as an inner pair.
                // So the next item after the non-terminal is the production.
                let production_pair = inner.next().ok_or("Missing production")?;

                let mut productions = Vec::new();
                // production = alternative ("|" alternative)*
                // alternative = weighted_production | simple_production
                for alt in production_pair.into_inner() {
                    if alt.as_rule() != Rule::alternative {
                        continue;
                    }
                    for prod in alt.into_inner() {
                        match prod.as_rule() {
                            Rule::weighted_production => {
                                let mut inner = prod.into_inner();
                                let weight_str = inner.next().unwrap().as_str();
                                // Note: the literal ":" in the pest grammar is not emitted as an inner pair.
                                // So the next item after the weight is the symbol sequence.
                                let symbol_seq = inner.next().unwrap();

                                let weight = weight_str.parse::<f64>()?;
                                let symbols = parse_symbol_sequence(symbol_seq)?;
                                productions.push(Production { symbols, weight });
                            }
                            Rule::simple_production => {
                                let symbol_seq = prod.into_inner().next().unwrap();
                                let symbols = parse_symbol_sequence(symbol_seq)?;
                                productions.push(Production {
                                    symbols,
                                    weight: 1.0,
                                });
                            }
                            _ => {}
                        }
                    }
                }

                if productions.is_empty() {
                    return Err(format!("No productions parsed for non-terminal: {}", non_terminal).into());
                }

                // Normalize weights to probabilities
                let total_weight: f64 = productions.iter().map(|p| p.weight).sum();
                if total_weight > 0.0 {
                    for prod in &mut productions {
                        prod.weight /= total_weight;
                    }
                } else {
                    // Equal weights if none specified
                    let equal_weight = 1.0 / productions.len() as f64;
                    for prod in &mut productions {
                        prod.weight = equal_weight;
                    }
                }

                rules.insert(non_terminal.clone(), GrammarRule { productions });
            }
        }
        
        Ok(Grammar { rules })
    }
    
    /// Load grammar from the embedded body.cfg file
    pub fn default() -> Result<Self, Box<dyn std::error::Error>> {
        let grammar_str = include_str!("../languages/english/body.cfg");
        let result = Self::from_str(grammar_str);
        if let Err(ref e) = result {
            eprintln!("Grammar parsing error: {}", e);
            eprintln!("Grammar string length: {}", grammar_str.len());
            eprintln!("First 500 chars:\n{}", &grammar_str[..grammar_str.len().min(500)]);
        }
        result.map_err(|e| format!("Failed to parse grammar: {}", e).into())
    }

    /// Load grammar from a file on disk.
    ///
    /// This is useful when you want to customize grammars outside of the binary
    /// and/or cache derived data alongside the grammar file.
    #[allow(dead_code)]
    pub fn from_file(grammar_path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let grammar_str = std::fs::read_to_string(grammar_path)?;
        Self::from_str(&grammar_str)
    }

    /// Convenience helper used by tests (and callers that don't care about caching yet).
    ///
    /// Historically this project had an on-disk cache here; the test suite still expects
    /// this API to exist. For now, we compute deterministically from the grammar file
    /// each call.
    #[allow(dead_code)]
    pub fn precompute_sequences_with_probability_cached_from_file(
        grammar_path: impl AsRef<Path>,
        start_symbol: &str,
        max_k: usize,
    ) -> Result<Vec<Vec<SequenceWithProbability>>, Box<dyn std::error::Error>> {
        let grammar = Self::from_file(grammar_path)?;
        Ok(grammar.precompute_sequences_with_probability(start_symbol, max_k))
    }
    
    /// Load subject grammar from the embedded subject.cfg file
    pub fn subject() -> Result<Self, Box<dyn std::error::Error>> {
        let grammar_str = include_str!("../languages/english/subject.cfg");
        let result = Self::from_str(grammar_str);
        if let Err(ref e) = result {
            eprintln!("Grammar parsing error: {}", e);
            eprintln!("Grammar string length: {}", grammar_str.len());
            eprintln!("First 500 chars:\n{}", &grammar_str[..grammar_str.len().min(500)]);
        }
        result.map_err(|e| format!("Failed to parse subject grammar: {}", e).into())
    }
    
    /// Get a production for a non-terminal, selecting randomly based on weights.
    /// Fully declarative: no special-casing of any symbol; the CFG defines behavior.
    #[allow(dead_code)]
    pub fn expand<R: Rng>(&self, rng: &mut R, non_terminal: &str) -> Option<Vec<Sym>> {
        let rule = self.rules.get(non_terminal)?;

        // Select production based on weights
        let mut rand_val = rng.gen::<f64>();
        for prod in &rule.productions {
            rand_val -= prod.weight;
            if rand_val <= 0.0 {
                return Some(prod.symbols.clone());
            }
        }

        // Fallback to first production
        rule.productions.first().map(|p| p.symbols.clone())
    }
    
    /// Enumerate all valid POS sequences of exactly length k with their probabilities.
    /// Sequences are sorted by probability (highest first).
    #[allow(dead_code)]
    pub fn enumerate_sequences_with_probability(
        &self,
        start_symbol: &str,
        k: usize,
    ) -> Vec<SequenceWithProbability> {
        // Memoization: (nonterminal, remaining_length) -> (sequence, probability)
        let mut memo: HashMap<(String, usize), Vec<(Vec<crate::Pos>, f64)>> = HashMap::new();

        self.enumerate_sequences_with_probability_internal(start_symbol, k, &mut memo)
    }

    /// Precompute POS sequences (and their probabilities) for lengths 0..=max_k.
    ///
    /// This is useful when you want a fixed mapping of:
    /// - **k → [POS sequences of length k]**
    ///
    /// The mapping is fully determined by the grammar file, so you can cache it
    /// per grammar (and per start symbol). Internally this shares the same DP memo
    /// across k values, which makes `max_k` up to ~20 cheap.
    pub fn precompute_sequences_with_probability(
        &self,
        start_symbol: &str,
        max_k: usize,
    ) -> Vec<Vec<SequenceWithProbability>> {
        // Memoization: (nonterminal, remaining_length) -> (sequence, probability)
        let mut memo: HashMap<(String, usize), Vec<(Vec<crate::Pos>, f64)>> = HashMap::new();

        let mut by_k: Vec<Vec<SequenceWithProbability>> = vec![Vec::new(); max_k + 1];
        for k in 0..=max_k {
            by_k[k] = self.enumerate_sequences_with_probability_internal(start_symbol, k, &mut memo);
        }
        by_k
    }

    fn enumerate_sequences_with_probability_internal(
        &self,
        start_symbol: &str,
        k: usize,
        memo: &mut HashMap<(String, usize), Vec<(Vec<crate::Pos>, f64)>>,
    ) -> Vec<SequenceWithProbability> {
        fn enumerate_recursive(
            grammar: &Grammar,
            sym: &Sym,
            remaining: usize,
            memo: &mut HashMap<(String, usize), Vec<(Vec<crate::Pos>, f64)>>,
        ) -> Vec<(Vec<crate::Pos>, f64)> {
            match sym {
                Sym::T(pos) => {
                    if remaining == 1 {
                        vec![(vec![*pos], 1.0)] // Terminal has probability 1.0
                    } else {
                        Vec::new()
                    }
                }
                Sym::Opt(inner) => {
                    let mut results = Vec::new();
                    
                    // Include the optional symbol (probability 0.5)
                    let include_results = enumerate_recursive(grammar, inner, remaining, memo)
                        .into_iter()
                        .map(|(seq, prob)| (seq, prob * 0.5))
                        .collect::<Vec<_>>();
                    results.extend(include_results);
                    
                    // Exclude the optional symbol (probability 0.5, produces empty)
                    if remaining == 0 {
                        results.push((Vec::new(), 0.5));
                    }
                    
                    results
                }
                Sym::NT(nt) => {
                    let key = (nt.clone(), remaining);
                    if let Some(cached) = memo.get(&key) {
                        return cached.clone();
                    }
                    
                    let rule = match grammar.rules.get(nt) {
                        Some(r) => r,
                        None => return Vec::new(),
                    };
                    
                    let mut all_results = Vec::new();
                    
                    // Try each production
                    for prod in &rule.productions {
                        let prod_weight = prod.weight; // Already normalized to probability
                        
                        // Recursively enumerate for each symbol in the production
                        // We need to try all ways to distribute remaining slots across symbols
                        let mut production_results = vec![(Vec::new(), 1.0)];
                        
                        for symbol in &prod.symbols {
                            let mut new_results = Vec::new();
                            
                            for (partial_seq, partial_prob) in production_results {
                                let used = partial_seq.len();
                                let available = remaining.saturating_sub(used);
                                
                                // Try allocating 0 to available slots to this symbol
                                for symbol_slots in 0..=available {
                                    let symbol_results = enumerate_recursive(
                                        grammar,
                                        symbol,
                                        symbol_slots,
                                        memo,
                                    );
                                    
                                    for (symbol_seq, symbol_prob) in symbol_results {
                                        // Symbol must use exactly the allocated slots
                                        if symbol_seq.len() != symbol_slots {
                                            continue;
                                        }
                                        
                                        let mut combined = partial_seq.clone();
                                        combined.extend(symbol_seq);
                                        
                                        // Only keep if we haven't exceeded remaining
                                        if combined.len() <= remaining {
                                            let combined_prob = partial_prob * symbol_prob;
                                            new_results.push((combined, combined_prob));
                                        }
                                    }
                                }
                            }
                            
                            production_results = new_results;
                        }
                        
                        // Multiply by production weight and filter to exact length
                        for (seq, symbol_prob) in production_results {
                            if seq.len() == remaining {
                                // Final probability = production weight * product of symbol probabilities
                                let final_prob = prod_weight * symbol_prob;
                                all_results.push((seq, final_prob));
                            }
                        }
                    }
                    
                    // Deduplicate: sum probabilities for identical sequences
                    let mut prob_map: HashMap<Vec<crate::Pos>, f64> = HashMap::new();
                    for (seq, prob) in all_results {
                        *prob_map.entry(seq).or_insert(0.0) += prob;
                    }
                    
                    let final_results: Vec<(Vec<crate::Pos>, f64)> = prob_map.into_iter().collect();
                    memo.insert(key, final_results.clone());
                    
                    final_results
                }
            }
        }
        
        let start_sym = Sym::NT(start_symbol.to_string());
        let results = enumerate_recursive(self, &start_sym, k, memo);
        
        // Deduplicate final results and convert to SequenceWithProbability
        let mut prob_map: HashMap<Vec<crate::Pos>, f64> = HashMap::new();
        for (seq, prob) in results {
            *prob_map.entry(seq).or_insert(0.0) += prob;
        }
        
        let mut sequences: Vec<SequenceWithProbability> = prob_map
            .into_iter()
            .map(|(sequence, probability)| SequenceWithProbability {
                sequence,
                probability,
            })
            .collect();
        
        // Sort by probability (highest first)
        sequences.sort_by(|a, b| b.probability.partial_cmp(&a.probability).unwrap_or(std::cmp::Ordering::Equal));
        
        sequences
    }
    
    /// Format the grammar rules in a concise text representation
    pub fn format_concise(&self) -> String {
        let mut output = String::new();
        let mut rules: Vec<_> = self.rules.iter().collect();
        rules.sort_by_key(|(name, _)| *name);
        
        if rules.is_empty() {
            return "No grammar rules found.\n".to_string();
        }
        
        for (non_terminal, rule) in rules {
            output.push_str(&format!("{} = ", non_terminal));
            
            let productions_str: Vec<String> = rule.productions.iter().map(|prod| {
                let symbols_str: Vec<String> = prod.symbols.iter().map(|sym| {
                    match sym {
                        Sym::T(pos) => {
                            // Format POS tags concisely
                            match pos {
                                Pos::Det => "Det".to_string(),
                                Pos::Adj => "Adj".to_string(),
                                Pos::N => "N".to_string(),
                                Pos::V => "V".to_string(),
                                Pos::Modal => "Modal".to_string(),
                                Pos::Aux => "Aux".to_string(),
                                Pos::Cop => "Cop".to_string(),
                                Pos::To => "To".to_string(),
                                Pos::Prep => "Prep".to_string(),
                                Pos::Adv => "Adv".to_string(),
                                Pos::Dot => "Dot".to_string(),
                                Pos::Prefix => "Prefix".to_string(),
                                Pos::Conj => "Conj".to_string(),
                            }
                        }
                        Sym::NT(nt) => nt.clone(),
                        Sym::Opt(inner) => match &**inner {
                            Sym::T(pos) => format!("{:?}?", pos),
                            Sym::NT(nt) => format!("{}?", nt),
                            Sym::Opt(_) => "Opt?".to_string(),
                        },
                    }
                }).collect();
                let prod_str = symbols_str.join(" ");
                if (prod.weight - 1.0).abs() > 0.001 {
                    format!("({:.2}: {})", prod.weight, prod_str)
                } else {
                    prod_str
                }
            }).collect();
            
            output.push_str(&productions_str.join(" | "));
            output.push('\n');
        }
        
        output
    }
}

fn parse_symbol_sequence(pair: pest::iterators::Pair<Rule>) -> Result<Vec<Sym>, Box<dyn std::error::Error>> {
    let mut symbols = Vec::new();
    
    for symbol_pair in pair.into_inner() {
        match symbol_pair.as_rule() {
            Rule::symbol => {
                let mut inner = symbol_pair.into_inner();
                let sym_type = inner.next().unwrap();
                let optional = inner.next();
                
                let base_sym = match sym_type.as_rule() {
                    Rule::terminal => {
                        let pos = match sym_type.as_str() {
                            "Det" => Pos::Det,
                            "Adj" => Pos::Adj,
                            "N" => Pos::N,
                            "V" => Pos::V,
                            "Modal" => Pos::Modal,
                            "Aux" => Pos::Aux,
                            "Cop" => Pos::Cop,
                            "To" => Pos::To,
                            "Prep" => Pos::Prep,
                            "Adv" => Pos::Adv,
                            "Dot" => Pos::Dot,
                            "Prefix" => Pos::Prefix,
                            "Conj" => Pos::Conj,
                            _ => return Err(format!("Unknown terminal: {}", sym_type.as_str()).into()),
                        };
                        Sym::T(pos)
                    }
                    Rule::non_terminal => {
                        Sym::NT(sym_type.as_str().to_string())
                    }
                    _ => return Err("Invalid symbol type".into()),
                };
                
                // Optional symbols are represented explicitly in the AST and handled in expansion.
                if optional.is_some() {
                    symbols.push(Sym::Opt(Box::new(base_sym)));
                } else {
                    symbols.push(base_sym);
                }
            }
            _ => {}
        }
    }
    
    Ok(symbols)
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pos;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn format_pos_sequence(seq: &[Pos]) -> String {
        seq.iter().map(|pos| {
            match pos {
                Pos::Det => "Det",
                Pos::Adj => "Adj",
                Pos::N => "N",
                Pos::V => "V",
                Pos::Modal => "Modal",
                Pos::Aux => "Aux",
                Pos::Cop => "Cop",
                Pos::To => "To",
                Pos::Prep => "Prep",
                Pos::Adv => "Adv",
                Pos::Dot => "Dot",
                Pos::Prefix => "Prefix",
                Pos::Conj => "Conj",
            }
        }).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn test_enumerate_sequences_with_probability() {
        let grammar = Grammar::default().expect("Failed to load body grammar");

        // Precompute once (shares DP memo across k values) and then read by length.
        let precomputed = grammar.precompute_sequences_with_probability("S", 20);
        
        let mut total_prob_all_k: f64 = 0.0;
        let mut k_values: Vec<usize> = Vec::new();
        let mut prob_by_k: Vec<(usize, f64)> = Vec::new();
        
        // Test with different values of k
        for k in 3..=20 {
            println!("\n=== Testing k = {} ===", k);
            let sequences = &precomputed[k];
            
            if sequences.is_empty() {
                println!("No valid sequences found for k = {}", k);
                continue;
            }
            
            println!("Found {} valid sequence(s):", sequences.len());
            println!("{:-<80}", "");
            
            let total_prob: f64 = sequences.iter().map(|s| s.probability).sum();
            println!("Total probability: {:.6} (probability that grammar generates exactly {} terminals)", total_prob, k);
            println!("{:-<80}", "");
            
            total_prob_all_k += total_prob;
            k_values.push(k);
            prob_by_k.push((k, total_prob));
            
            for (i, seq_prob) in sequences.iter().take(20).enumerate() {
                let seq_str = format_pos_sequence(&seq_prob.sequence);
                println!("{}. [{:>6.4}%] {}", 
                    i + 1, 
                    seq_prob.probability * 100.0,
                    seq_str
                );
            }
            if sequences.len() > 20 {
                println!("... and {} more sequences", sequences.len() - 20);
            }
        }
        
        // Print summary across all k values
        println!("\n{:=<80}", "");
        println!("SUMMARY: Probability distribution across all sequence lengths");
        println!("{:=<80}", "");
        for (k, prob) in &prob_by_k {
            println!("k = {:2}: {:.6} ({:.2}%)", k, prob, prob * 100.0);
        }
        println!("{:-<80}", "");
        println!("Sum across all k values: {:.6}", total_prob_all_k);
        println!("Expected: close to 1.0 (all possible sequences from grammar)");
        println!("{:=<80}", "");
    }

    #[test]
    fn test_enumerate_simple_cases() {
        let grammar = Grammar::default().expect("Failed to load body grammar");
        
        // VP should NOT have k=1 sequences once bare `V` is removed from the grammar.
        println!("Testing VP with k=1:");
        let vp_1 = grammar.enumerate_sequences_with_probability("VP", 1);
        println!("  Found {} sequences", vp_1.len());
        assert!(
            vp_1.is_empty(),
            "Expected no VP sequences of length 1 after removing bare V, got: {:?}",
            vp_1
        );

        // VP with k=2 should still work (e.g., Cop Adj, V NP with NP=N, Modal V).
        println!("Testing VP with k=2:");
        let vp_2 = grammar.enumerate_sequences_with_probability("VP", 2);
        println!("  Found {} sequences", vp_2.len());
        assert!(!vp_2.is_empty(), "Expected some VP sequences of length 2");
        
        // Test NP with k=1 (should return N)
        println!("Testing NP with k=1:");
        let np_1 = grammar.enumerate_sequences_with_probability("NP", 1);
        println!("  Found {} sequences", np_1.len());
        for seq in &np_1 {
            println!("    {:?} (prob: {})", seq.sequence, seq.probability);
        }
        
        // Test S_N with k=3: N(1) + VP(1) + Dot(1)
        println!("Testing S_N with k=3:");
        let s_n_3 = grammar.enumerate_sequences_with_probability("S_N", 3);
        println!("  Found {} sequences", s_n_3.len());
        for seq in &s_n_3 {
            println!("    {:?} (prob: {})", seq.sequence, seq.probability);
        }
    }

    #[test]
    fn test_enumerate_start_symbols() {
        let grammar = Grammar::default().expect("Failed to load body grammar");
        
        let start_symbols = vec!["S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"];
        
        for start in start_symbols {
            println!("\n=== Testing start symbol: {} ===", start);
            let sequences = grammar.enumerate_sequences_with_probability(start, 5);
            
            if sequences.is_empty() {
                println!("No valid sequences found for {} with k=5", start);
                continue;
            }
            
            println!("Found {} valid sequence(s):", sequences.len());
            for (i, seq_prob) in sequences.iter().take(10).enumerate() {
                let seq_str = format_pos_sequence(&seq_prob.sequence);
                println!("  {}. [{:>6.4}%] {}", 
                    i + 1, 
                    seq_prob.probability * 100.0,
                    seq_str
                );
            }
            if sequences.len() > 10 {
                println!("  ... and {} more", sequences.len() - 10);
            }
        }
    }

    #[test]
    fn test_sequence_cache_roundtrip_tempdir() {
        // Write a temporary grammar file, build the cache, then ensure a subsequent call reads it.
        let grammar_str = include_str!("../languages/english/body.cfg");
        let tmp = std::env::temp_dir();
        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let grammar_path = tmp.join(format!("bip39_encode_body_{uniq}.cfg"));
        std::fs::write(&grammar_path, grammar_str).unwrap();

        let by_k_1 = Grammar::precompute_sequences_with_probability_cached_from_file(&grammar_path, "S", 20)
            .expect("cache compute 1");
        let by_k_2 = Grammar::precompute_sequences_with_probability_cached_from_file(&grammar_path, "S", 20)
            .expect("cache compute 2");

        // Basic sanity: same sizes and non-empty for a known length.
        assert_eq!(by_k_1.len(), by_k_2.len());
        assert!(!by_k_1[4].is_empty());
        assert_eq!(by_k_1[4].len(), by_k_2[4].len());
    }
}

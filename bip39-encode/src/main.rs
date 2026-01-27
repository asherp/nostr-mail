// Proof-of-concept: tiny CFG sentence generator that embeds a POS-tagged payload
// (e.g., BIP39 words) in-order, inserting only SFW cover words.
//
// Cargo.toml:
// [dependencies]
// rand = "0.8"

mod grammar;

use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand::rngs::StdRng;
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::OnceLock;
use std::path::Path;
use std::time::Instant;
use grammar::{Grammar, SequenceWithProbability};

static PRINTED_SENTENCE_KINDS: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HighlightMode {
    None,
    Bars,
    Color(u8),  // ANSI color code (30-37 for foreground colors)
    Madlib,
}

/// Parse color name or ANSI code to color value
fn parse_color(color_str: &str) -> Result<u8, String> {
    match color_str.to_lowercase().as_str() {
        "black" => Ok(30),
        "red" => Ok(31),
        "green" => Ok(32),
        "yellow" => Ok(33),
        "blue" => Ok(34),
        "magenta" => Ok(35),
        "cyan" => Ok(36),
        "white" => Ok(37),
        _ => {
            // Try parsing as a number
            color_str.parse::<u8>()
                .map_err(|_| format!("Invalid color: {}. Use a color name (red, green, blue, etc.) or ANSI code (30-37)", color_str))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GenerationMode {
    Subject,
    Body,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SentenceLengthMode {
    Compact,
    Natural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pos {
    Det,
    Adj,
    N,
    V,
    Modal,
    Aux,
    Cop,
    To,
    Prep,
    Adv,
    Conj,
    Dot,
    Prefix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Number {
    Singular,
    Plural,
}

#[derive(Clone, Debug)]
struct PayloadTok {
    word: String,
    allowed: HashSet<Pos>,
}

impl PayloadTok {
    fn new(word: impl Into<String>, allowed: &[Pos]) -> Self {
        Self {
            word: word.into(),
            allowed: allowed.iter().copied().collect(),
        }
    }
}

/// Small, controlled, SFW lexicon by POS.
/// IMPORTANT: ensure cover lexicon does NOT contain any payload (BIP39) words,
/// so decoding (filtering BIP39 words) is trivial and unambiguous.
#[derive(Clone, Debug)]
struct Lexicon {
    by_pos: HashMap<Pos, Vec<String>>,
    /// Lowercased payload words (for filtering / repetition logic).
    payload_set: HashSet<String>,
    /// Lowercased full BIP39 set (for collision checks when inflecting cover words).
    bip39_set: HashSet<String>,
}

impl Lexicon {
    fn new(payload_set: HashSet<String>, bip39_set: HashSet<String>) -> Self {
        Self {
            by_pos: HashMap::new(),
            payload_set,
            bip39_set,
        }
    }

    fn with_words(mut self, pos: Pos, words: &[&str]) -> Self {
        self.by_pos
            .entry(pos)
            .or_insert_with(Vec::new)
            .extend(words.iter().map(|w| w.to_string()));
        self
    }

    fn pick_cover<R: Rng>(&self, rng: &mut R, pos: Pos, recent_words: &[&str]) -> String {
        let list = self.by_pos.get(&pos).unwrap_or_else(|| {
            panic!(
                "missing lexicon list for {:?}. Add cover words for this POS.",
                pos
            )
        });

        // Filter out payload words and recent words (to avoid repetition within a window)
        let available: Vec<&String> = list
            .iter()
            .filter(|w| {
                !self.payload_set.contains(&w.to_lowercase()) && 
                !recent_words.iter().any(|&rw| rw == w.as_str())
            })
            .collect();

        if available.is_empty() {
            // If all words would be repeats, fall back to any non-payload word
            let fallback: Vec<&String> = list
                .iter()
                .filter(|w| !self.payload_set.contains(&w.to_lowercase()))
                .collect();
            if fallback.is_empty() {
                panic!("No available cover words for {:?}", pos);
            }
            // Prioritize shorter words in fallback too
            let min_len = fallback.iter().map(|w| w.len()).min().unwrap_or(0);
            let shortest_fallback: Vec<&String> = fallback
                .iter()
                .filter(|w| w.len() == min_len)
                .copied()
                .collect();
            return shortest_fallback.choose(rng).unwrap().to_string();
        }

        // Find the shortest length among available words
        let min_len = available.iter().map(|w| w.len()).min().unwrap_or(0);
        
        // Filter to only words of the shortest length
        let shortest_words: Vec<&String> = available
            .iter()
            .filter(|w| w.len() == min_len)
            .copied()
            .collect();

        shortest_words.choose(rng).unwrap().to_string()
    }

    /// Like `pick_cover`, but allows an additional predicate to enforce lightweight grammar constraints
    /// (e.g., "bare verb after Modal", "transitive verb before NP").
    ///
    /// Returns `None` if no word satisfies the predicate (caller should fall back to `pick_cover`).
    fn pick_cover_filtered<R: Rng, F: Fn(&str) -> bool>(
        &self,
        rng: &mut R,
        pos: Pos,
        recent_words: &[&str],
        predicate: F,
    ) -> Option<String> {
        let list = self.by_pos.get(&pos)?;

        // Filter out payload words, recent words, and words failing the predicate.
        let available: Vec<&String> = list
            .iter()
            .filter(|w| {
                !self.payload_set.contains(&w.to_lowercase())
                    && !recent_words.iter().any(|&rw| rw == w.as_str())
                    && predicate(w.as_str())
            })
            .collect();

        if available.is_empty() {
            return None;
        }

        let min_len = available.iter().map(|w| w.len()).min().unwrap_or(0);
        let shortest: Vec<&String> = available
            .iter()
            .filter(|w| w.len() == min_len)
            .copied()
            .collect();

        Some(shortest.choose(rng).unwrap().to_string())
    }

}

#[derive(Clone, Debug)]
enum Sym {
    NT(String),
    T(Pos),
    Opt(Box<Sym>),
}

fn payload_fits(tok: &PayloadTok, slot: Pos) -> bool {
    // Since BIP39 words now have only one POS tag, strict matching only
    tok.allowed.contains(&slot)
}

/// Get the grammar instance for the given mode (lazy-loaded)
fn get_grammar(mode: GenerationMode) -> &'static Grammar {
    match mode {
        GenerationMode::Subject => {
            static SUBJECT_GRAMMAR: OnceLock<Grammar> = OnceLock::new();
            SUBJECT_GRAMMAR.get_or_init(|| {
                Grammar::subject().expect("Failed to load subject grammar")
            })
        }
        GenerationMode::Body => {
            static BODY_GRAMMAR: OnceLock<Grammar> = OnceLock::new();
            BODY_GRAMMAR.get_or_init(|| {
                Grammar::default().expect("Failed to load body grammar")
            })
        }
    }
}

fn start_nonterminal_for_pos(pos: Pos) -> &'static str {
    // Body grammar uses only "S" with weighted alternatives
    // Subject grammar may still have S_* variants, but we default to "S" for simplicity
    // The planner will find sequences that match naturally
    match pos {
        Pos::N | Pos::V | Pos::Adj | Pos::Adv | Pos::Prep | Pos::Det |
        Pos::Modal | Pos::Aux | Pos::Cop | Pos::To | Pos::Conj | Pos::Dot | Pos::Prefix => "S",
    }
}

/// Precomputed sequences organized by start symbol and k
struct SequenceCache {
    by_start_symbol: HashMap<String, Vec<Vec<SequenceWithProbability>>>,
}

impl SequenceCache {
    /// Load sequences for all start symbols we might use, up to k_max
    fn load(mode: GenerationMode, k_max: usize, verbose: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let grammar_path = match mode {
            GenerationMode::Subject => "languages/english/subject.cfg",
            GenerationMode::Body => "languages/english/body.cfg",
        };
        let grammar_label = match mode {
            GenerationMode::Subject => "subject",
            GenerationMode::Body => "body",
        };
        
        let grammar_path = Path::new(grammar_path);
        if !grammar_path.exists() {
            return Err(format!("Grammar file not found: {:?}", grammar_path).into());
        }
        
        let mut by_start_symbol = HashMap::new();
        let grammar_str = std::fs::read_to_string(grammar_path)?;
        let grammar = Grammar::from_str(&grammar_str)?;
        
        // Load sequences for all possible start symbols
        // Body grammar only uses "S" (simplified), subject grammar may have S_* variants
        let start_symbols = if mode == GenerationMode::Body {
            vec!["S"]
        } else {
            vec!["S", "S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"]
        };
        
        for start_symbol in start_symbols {
            let sequences_by_k = grammar.precompute_sequences_with_probability(start_symbol, k_max);
            if !sequences_by_k.is_empty() {
                by_start_symbol.insert(start_symbol.to_string(), sequences_by_k);
            }
        }
        
        print_sentence_kinds_once(grammar_label, k_max, &by_start_symbol, verbose);

        Ok(SequenceCache { by_start_symbol })
    }
    
    /// Get sequences for a given start symbol and k
    fn get(&self, start_symbol: &str, k: usize) -> Option<&[SequenceWithProbability]> {
        self.by_start_symbol
            .get(start_symbol)?
            .get(k)
            .map(|v| v.as_slice())
    }
}

fn print_sentence_kinds_once(
    grammar_label: &str,
    k_max: usize,
    by_start_symbol: &HashMap<String, Vec<Vec<SequenceWithProbability>>>,
    verbose: bool,
) {
    if by_start_symbol.is_empty() || !verbose {
        return;
    }

    PRINTED_SENTENCE_KINDS.get_or_init(|| {
        let mut total_sequences = 0usize;
        let mut per_start: Vec<(String, usize)> = by_start_symbol
            .iter()
            .map(|(start_symbol, sequences_by_k)| {
                let count = sequences_by_k.iter().map(|seqs| seqs.len()).sum::<usize>();
                total_sequences += count;
                (start_symbol.clone(), count)
            })
            .collect();
        per_start.sort_by(|a, b| a.0.cmp(&b.0));
        let per_start_str = per_start
            .iter()
            .map(|(s, c)| format!("{}={}", s, c))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "Grammar {} defines {} sentence kinds (k <= {}): {}",
            grammar_label,
            total_sequences,
            k_max,
            per_start_str
        );
    });
}

/// Find the maximum subsequence embedding of payload words into slots.
/// Returns Some(placement_map) where placement_map[slot_index] = payload_index if that slot should contain a payload word.
/// Returns None if j payload words cannot be embedded.
fn max_subsequence_embedding(
    slots: &[Pos],
    payload: &[PayloadTok],
    payload_start: usize,
    j: usize,
) -> Option<HashMap<usize, usize>> {
    if j == 0 {
        return Some(HashMap::new());
    }
    
    if payload_start + j > payload.len() {
        return None;
    }
    
    // Filter out Dot and function word slots that can't hold payload words
    // Dot is punctuation, Prefix/Aux/Cop/To are function words that must be cover words
    let word_slots: Vec<(usize, Pos)> = slots
        .iter()
        .enumerate()
        .filter(|(_, pos)| {
            **pos != Pos::Dot 
            && **pos != Pos::Prefix 
            && **pos != Pos::Aux 
            && **pos != Pos::Cop 
            && **pos != Pos::To
        })
        .map(|(idx, pos)| (idx, *pos))
        .collect();
    
    if word_slots.len() < j {
        return None;
    }
    
    // Greedy matching: try to place each payload word in order
    let mut placement = HashMap::new();
    let mut payload_idx = payload_start;
    let mut slot_idx_in_word_slots = 0;
    
    while payload_idx < payload_start + j && slot_idx_in_word_slots < word_slots.len() {
        let (original_slot_idx, slot_pos) = word_slots[slot_idx_in_word_slots];
        let payload_word = &payload[payload_idx];
        
        // Check if this payload word can go in this slot
        if payload_fits(payload_word, slot_pos) {
            placement.insert(original_slot_idx, payload_idx);
            payload_idx += 1;
        }
        
        slot_idx_in_word_slots += 1;
    }
    
    // Did we place all j words?
    if payload_idx == payload_start + j {
        Some(placement)
    } else {
        None
    }
}

/// Plan a sentence: find the best POS sequence and payload embedding for given k.
/// Returns (slots, forced_placement_map, j) where j is the number of payload words embedded.
/// If require_prefix is true, only consider sequences that start with Pos::Prefix.
fn plan_sentence<R: Rng>(
    rng: &mut R,
    cache: &SequenceCache,
    start_symbol: &str,
    k: usize,
    payload: &[PayloadTok],
    payload_start: usize,
    require_prefix: bool,
) -> Option<(Vec<Pos>, HashMap<usize, usize>, usize)> {
    let sequences = cache.get(start_symbol, k)?;
    
    if sequences.is_empty() {
        return None;
    }
    
    let remaining_payload = payload.len().saturating_sub(payload_start);
    if remaining_payload == 0 {
        return None;
    }
    
    // Filter sequences if require_prefix is true
    // Keep track of original indices when filtering
    let filtered_with_indices: Vec<(usize, &SequenceWithProbability)> = if require_prefix {
        sequences.iter()
            .enumerate()
            .filter(|(_, seq_prob)| !seq_prob.sequence.is_empty() && seq_prob.sequence[0] == Pos::Prefix)
            .collect()
    } else {
        sequences.iter().enumerate().collect()
    };
    
    if filtered_with_indices.is_empty() {
        return None;
    }
    
    // m = number of word slots that can hold payload words (excluding Dot and function words)
    // Try j from min(remaining_payload, m) down to 1
    // For each j, try sequences in probability order
    
    // First, figure out m by looking at the first sequence
    // Exclude Dot (punctuation) and function words that must be cover words (Prefix, Aux, Cop, To)
    let first_seq = &filtered_with_indices[0].1.sequence;
    let m = first_seq.iter().filter(|&&pos| {
        pos != Pos::Dot 
        && pos != Pos::Prefix 
        && pos != Pos::Aux 
        && pos != Pos::Cop 
        && pos != Pos::To
    }).count();
    
    let max_j = remaining_payload.min(m);
    
    // Try j from max_j down to 1
    for j in (1..=max_j).rev() {
        // Collect all sequences that can embed j payload words.
        // Then choose among them probabilistically by grammar probability.
        let mut candidates: Vec<(usize, HashMap<usize, usize>)> = Vec::new();
        let mut total_prob: f64 = 0.0;

        for (original_idx, seq_prob) in filtered_with_indices.iter() {
            if let Some(placement) = max_subsequence_embedding(
                &seq_prob.sequence,
                payload,
                payload_start,
                j,
            ) {
                total_prob += seq_prob.probability;
                candidates.push((*original_idx, placement));
            }
        }

        if candidates.is_empty() {
            continue;
        }

        // Weighted random selection by probability.
        // (If probabilities are all zeros, fall back to uniform.)
        if total_prob > 0.0 {
            let mut r = rng.gen::<f64>() * total_prob;
            let mut last: Option<(usize, HashMap<usize, usize>)> = None;

            for (idx, placement) in candidates.iter() {
                last = Some((*idx, placement.clone()));
                let w = sequences[*idx].probability;
                if r <= w {
                    return Some((sequences[*idx].sequence.clone(), placement.clone(), j));
                }
                r -= w;
            }

            // Numerical edge-case: fall back to last feasible candidate.
            let (idx, placement) = last.expect("candidates non-empty");
            return Some((sequences[idx].sequence.clone(), placement, j));
        } else {
            let (idx, placement) = candidates
                .choose(rng)
                .expect("candidates non-empty")
                .clone();
            return Some((sequences[idx].sequence.clone(), placement, j));
        }
    }
    
    None
}

/// Generate a minimal fallback sentence structure that can always embed a payload word.
/// This ensures payload preservation even if it results in grammar errors.
/// Returns (slots, forced_placement_map) where the word is forced into the first compatible slot.
fn generate_fallback_sentence(
    payload: &[PayloadTok],
    payload_start: usize,
    mode: GenerationMode,
) -> Option<(Vec<Pos>, HashMap<usize, usize>)> {
    if payload_start >= payload.len() {
        return None;
    }
    
    let word = &payload[payload_start];
    
    // In subject mode, don't include Dot (email subjects don't have periods)
    let include_dot = mode != GenerationMode::Subject;
    
    // Create minimal sentence structures that can accommodate any POS
    // We prioritize the first allowed POS tag, but will force-place if needed
    let (slots, slot_idx) = if word.allowed.contains(&Pos::N) {
        // Simple: "The [word]." or "The [word]" for subject
        if include_dot {
            (vec![Pos::Det, Pos::N, Pos::Dot], 1)
        } else {
            (vec![Pos::Det, Pos::N], 1)
        }
    } else if word.allowed.contains(&Pos::V) {
        // "The note [word]." or "The note [word]" for subject
        if include_dot {
            (vec![Pos::Det, Pos::N, Pos::V, Pos::Dot], 2)
        } else {
            (vec![Pos::Det, Pos::N, Pos::V], 2)
        }
    } else if word.allowed.contains(&Pos::Adj) {
        // "The [word] note." or "The [word] note" for subject
        if include_dot {
            (vec![Pos::Det, Pos::Adj, Pos::N, Pos::Dot], 1)
        } else {
            (vec![Pos::Det, Pos::Adj, Pos::N], 1)
        }
    } else if word.allowed.contains(&Pos::Adv) {
        // "The note works [word]." or "The note works [word]" for subject
        if include_dot {
            (vec![Pos::Det, Pos::N, Pos::V, Pos::Adv, Pos::Dot], 3)
        } else {
            (vec![Pos::Det, Pos::N, Pos::V, Pos::Adv], 3)
        }
    } else if word.allowed.contains(&Pos::Prep) {
        // "The note [word] the user." or "The note [word] the user" for subject
        if include_dot {
            (vec![Pos::Det, Pos::N, Pos::Prep, Pos::Det, Pos::N, Pos::Dot], 2)
        } else {
            (vec![Pos::Det, Pos::N, Pos::Prep, Pos::Det, Pos::N], 2)
        }
    } else if word.allowed.contains(&Pos::Det) {
        // "[word] note works." or "[word] note works" for subject
        if include_dot {
            (vec![Pos::Det, Pos::N, Pos::V, Pos::Dot], 0)
        } else {
            (vec![Pos::Det, Pos::N, Pos::V], 0)
        }
    } else {
        // Last resort: force into any slot (will cause grammar error but preserves payload)
        // Use noun slot as most common
        if include_dot {
            (vec![Pos::Det, Pos::N, Pos::Dot], 1)
        } else {
            (vec![Pos::Det, Pos::N], 1)
        }
    };
    
    let mut forced = HashMap::new();
    forced.insert(slot_idx, payload_start);
    
    Some((slots, forced))
}

/// Check if a word starts with a vowel sound (needed for a/an selection).
fn starts_with_vowel_sound(word: &str) -> bool {
    // Normalize first so highlighting, bars, punctuation, and case don't break a/an selection.
    let normalized = normalize_token_for_bip39(word);
    if normalized.is_empty() {
        return false;
    }
    let first_char = normalized.chars().next().unwrap();
    matches!(first_char, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// Heuristic: "bare" verb form appropriate after a modal (e.g., "can go", not "can going/goed/goes").
fn is_bare_verb_form(word: &str) -> bool {
    let w = normalize_token_for_bip39(word).to_lowercase();
    if w.is_empty() {
        return false;
    }
    // Very cheap morphology filter: exclude common inflections.
    if w.ends_with("ing") || w.ends_with("ed") || w.ends_with("s") {
        // Allow a couple of irregular bare forms that end with 's' in writing? (none for modals)
        return false;
    }
    true
}

/// Tiny transitivity heuristic for common cover verbs.
/// We only use this to bias *cover word selection* in `V NP` contexts.
fn is_likely_transitive_verb(word: &str) -> bool {
    let w = normalize_token_for_bip39(word).to_lowercase();
    // Small whitelist: keep it short and safe; we can expand later.
    matches!(
        w.as_str(),
        "add"
            | "bring"
            | "build"
            | "call"
            | "change"
            | "check"
            | "close"
            | "create"
            | "deliver"
            | "find"
            | "fix"
            | "get"
            | "give"
            | "help"
            | "hold"
            | "keep"
            | "leave"
            | "make"
            | "need"
            | "open"
            | "put"
            | "read"
            | "save"
            | "see"
            | "send"
            | "set"
            | "show"
            | "take"
            | "tell"
            | "use"
            | "verify"
            | "write"
    )
}

fn pluralize_cover_noun(s: &str) -> String {
    // Conservative, ASCII-only rules are fine here because cover lexicon is lowercase ASCII.
    if s.is_empty() {
        return String::new();
    }
    // If it's already plural-ish, leave it (avoid "classs").
    if s.ends_with('s') {
        return s.to_string();
    }
    if s.ends_with("ch") || s.ends_with("sh") || s.ends_with('x') || s.ends_with('z') {
        return format!("{s}es");
    }
    if s.ends_with('y') && s.len() >= 2 {
        let prev = s.as_bytes()[s.len() - 2] as char;
        if !matches!(prev, 'a' | 'e' | 'i' | 'o' | 'u') {
            return format!("{}ies", &s[..s.len() - 1]);
        }
    }
    format!("{s}s")
}

/// Fill a slot stream with cover words + payload words (in-order).
/// Returns (words, payload_embedded_count).
/// `prev_words` are the last few words from the previous sentence (if any), to prevent repetition across sentences.
/// `expected_first_pos` is the POS that should appear first (if set), used to ensure payload word placement.
/// `forced_placements` maps slot_index -> payload_index for slots that must contain specific payload words.
fn fill_slots<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    slots: &[Pos],
    payload: &[PayloadTok],
    payload_i: &mut usize,
    prev_words: &[&str],
    _expected_first_pos: Option<Pos>,
    forced_placements: Option<&HashMap<usize, usize>>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    const REPETITION_WINDOW: usize = 3; // Check last 3 words to avoid repetition
    // Cache for words picked early (for a/an selection) to reuse later
    let mut word_cache: HashMap<usize, String> = HashMap::new();
    // Noun number chosen per NP, keyed by the noun slot index within this sentence.
    let mut noun_number: HashMap<usize, Number> = HashMap::new();
    // Only used for agreement when we introduce Aux/Cop later; keep it stable now.
    let mut subject_number: Option<Number> = None;
    // Track which payload words have been used (by index) to avoid skipping words unnecessarily
    let mut used_payload_indices: HashSet<usize> = HashSet::new();

    for (i, &slot) in slots.iter().enumerate() {
        match slot {
            Pos::Dot => {
                if let Some(last) = out.last_mut() {
                    last.push('.');
                } else {
                    out.push(".".to_string());
                }
            }
            Pos::Det => {
                // Check if this slot has a forced placement
                let payload_word_idx = if let Some(forced) = forced_placements {
                    forced.get(&i).copied()
                } else {
                    // Allow embedding payload determiners (e.g., "this", "that") without mutation.
                    // Use current payload word if it fits, otherwise use cover word (strict order preservation)
                    if *payload_i < payload.len() 
                        && !used_payload_indices.contains(payload_i)
                        && payload_fits(&payload[*payload_i], Pos::Det) {
                        Some(*payload_i)
                    } else {
                        None
                    }
                };
                
                if let Some(idx) = payload_word_idx {
                    // Use payload word and advance to next
                    out.push(payload[idx].word.clone());
                    used_payload_indices.insert(idx);
                    // Only advance payload_i if we're not using forced placements
                    if forced_placements.is_none() {
                        *payload_i += 1;
                    }
                    continue;
                }

                // Build recent words list: prev_words + last few words from current output
                let mut recent_words: Vec<&str> = prev_words.to_vec();
                let start_idx = out.len().saturating_sub(REPETITION_WINDOW);
                recent_words.extend(out[start_idx..].iter().map(|s| s.as_str()));

                // Determine NP number (singular/plural) for this NP.
                // We force the first NP (likely the subject) to singular to reduce agreement issues
                // when payload nouns are embedded (payload spelling cannot be pluralized).
                let mut np_number = if subject_number.is_none() {
                    Number::Singular
                } else if rng.gen_bool(0.25) {
                    Number::Plural
                } else {
                    Number::Singular
                };

                // Locate the noun slot for this NP (Det Adj? N).
                let mut noun_idx = None;
                let mut j = i + 1;
                while j < slots.len() && slots[j] == Pos::Adj {
                    j += 1;
                }
                if j < slots.len() && slots[j] == Pos::N {
                    noun_idx = Some(j);
                }

                // Record subject number on first NP.
                if subject_number.is_none() {
                    subject_number = Some(np_number);
                }

                if let Some(ni) = noun_idx {
                    noun_number.insert(ni, np_number);
                } else {
                    // No noun slot ahead; fall back to singular-safe determiners.
                    np_number = Number::Singular;
                }
                
                // Determine what the next word will be (for a/an selection)
                // Prefer forced placements when present (body/subject mode planning). In that mode,
                // payload_i is intentionally not advanced inside fill_slots, so "peek by payload_i"
                // can be wrong.
                let next_word_str = if let Some(forced) = forced_placements {
                    if i + 1 < slots.len() {
                        forced
                            .get(&(i + 1))
                            .and_then(|&payload_idx| payload.get(payload_idx))
                            .map(|t| t.word.as_str())
                    } else {
                        None
                    }
                } else if *payload_i < payload.len() {
                    // Check if next slot would fit the current payload word (strict order preservation)
                    if let Some(next_slot) = slots.get(i + 1) {
                        // Check if current payload word fits and hasn't been used
                        if !used_payload_indices.contains(payload_i)
                            && payload_fits(&payload[*payload_i], *next_slot)
                        {
                            // Next word is a payload word - use it directly
                            Some(payload[*payload_i].word.as_str())
                        } else {
                            // Next word will be a cover word - actually pick it now and cache it
                            let picked_word = lex.pick_cover(rng, *next_slot, &recent_words);
                            word_cache.insert(i + 1, picked_word.clone());
                            // Get reference from cache (it was just inserted, so unwrap is safe)
                            word_cache.get(&(i + 1)).map(|s| s.as_str())
                        }
                    } else {
                        None
                    }
                } else if i + 1 < slots.len() {
                    // No more payload words, next will be a cover word - pick it now and cache it
                    if let Some(next_slot) = slots.get(i + 1) {
                        let picked_word = lex.pick_cover(rng, *next_slot, &recent_words);
                        word_cache.insert(i + 1, picked_word);
                        // Get reference from cache (it was just inserted, so unwrap is safe)
                        word_cache.get(&(i + 1)).map(|s| s.as_str())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let det_word = match np_number {
                    Number::Singular => {
                        // Use a/an sometimes when we know the next word, otherwise choose a safer determiner.
                        // Prioritize shorter words: "the" (3) < "a"/"an" (1-2) < "each"/"some" (4)
                        if let Some(next) = next_word_str {
                            if rng.gen_bool(0.35) {
                                // Normalize the word before checking vowel sound (strip any potential formatting)
                                let normalized_next = normalize_token_for_bip39(next);
                                if starts_with_vowel_sound(&normalized_next) {
                                    "an".to_string()
                                } else {
                                    "a".to_string()
                                }
                            } else {
                                // Prefer "the" (shortest) but allow some variety
                                let det_options = ["the", "each", "some"];
                                if rng.gen_bool(0.7) {
                                    "the".to_string() // Prefer shortest
                                } else {
                                    det_options.choose(rng).unwrap().to_string()
                                }
                            }
                        } else {
                            // Prefer "the" (shortest) but allow some variety
                            let det_options = ["the", "each", "some"];
                            if rng.gen_bool(0.7) {
                                "the".to_string() // Prefer shortest
                            } else {
                                det_options.choose(rng).unwrap().to_string()
                            }
                        }
                    }
                    Number::Plural => {
                        // Avoid these/those because payload nouns cannot be pluralized.
                        // Prefer "the" (3) over "some" (4)
                        let det_options = ["the", "some"];
                        if rng.gen_bool(0.7) {
                            "the".to_string() // Prefer shortest
                        } else {
                            det_options.choose(rng).unwrap().to_string()
                        }
                    }
                };

                // Check if this determiner would repeat a recent word
                // If so and it's not a/an, pick a different one
                let final_det = if recent_words.contains(&det_word.as_str()) && det_word != "a" && det_word != "an" {
                    // Avoid repetition for non-a/an determiners
                    // Prioritize shorter words when avoiding repetition
                    let det_options = ["the", "each", "some"];
                    // Try shortest first, then others
                    if !recent_words.contains(&"the") {
                        "the".to_string()
                    } else {
                        det_options.iter()
                            .find(|&&d| !recent_words.contains(&d))
                            .copied()
                            .unwrap_or_else(|| det_options.choose(rng).unwrap())
                            .to_string()
                    }
                } else {
                    det_word
                };
                
                out.push(final_det);
            }
            _ => {
                // Certain slots should never use payload words (grammatical function words)
                let must_use_cover = matches!(
                    slot,
                    Pos::Aux | Pos::Cop | Pos::To | Pos::Prefix | Pos::Modal | Pos::Conj
                );
                
                // Check if this slot has a forced placement
                let payload_word_idx = if let Some(forced) = forced_placements {
                    forced.get(&i).copied()
                } else if must_use_cover {
                    None
                } else if *payload_i < payload.len() 
                    && !used_payload_indices.contains(payload_i)
                    && payload_fits(&payload[*payload_i], slot) {
                    Some(*payload_i)
                } else {
                    None
                };
                
                if let Some(idx) = payload_word_idx {
                    // Use payload word and advance to next
                    out.push(payload[idx].word.clone());
                    used_payload_indices.insert(idx);
                    // Only advance payload_i if we're not using forced placements
                    if forced_placements.is_none() {
                        *payload_i += 1;
                    }
                } else {
                    // Current word doesn't fit - advance to next unused word if current is already used
                    if *payload_i < payload.len() && used_payload_indices.contains(payload_i) {
                        // Current word is already used, advance to next unused
                        while *payload_i < payload.len() && used_payload_indices.contains(payload_i) {
                            *payload_i += 1;
                        }
                    }
                    // Use cover word
                    // Check if we already picked this word (for a/an selection)
                    if let Some(cached_word) = word_cache.remove(&i) {
                        out.push(cached_word);
                    } else {
                        // Build recent words list: prev_words + last few words from current output
                        let mut recent_words: Vec<&str> = prev_words.to_vec();
                        let start_idx = out.len().saturating_sub(REPETITION_WINDOW);
                        recent_words.extend(out[start_idx..].iter().map(|s| s.as_str()));
                        let cover_word = if slot == Pos::Aux {
                            match subject_number.unwrap_or(Number::Singular) {
                                Number::Singular => "does".to_string(),
                                Number::Plural => "do".to_string(),
                            }
                        } else if slot == Pos::Cop {
                            match subject_number.unwrap_or(Number::Singular) {
                                Number::Singular => "is".to_string(),
                                Number::Plural => "are".to_string(),
                            }
                        } else if slot == Pos::V {
                            // Lightweight agreement constraints:
                            // - Modal → bare V only
                            // - V → NP only if (likely) transitive (cover words only)
                            let prev_slot = if i > 0 { Some(slots[i - 1]) } else { None };
                            let next_slot = slots.get(i + 1).copied();
                            let after_modal = matches!(prev_slot, Some(Pos::Modal));
                            let next_starts_np = matches!(next_slot, Some(Pos::Det) | Some(Pos::N));
                            let want_transitive = next_starts_np;

                            let constrained = if after_modal && want_transitive {
                                lex.pick_cover_filtered(rng, slot, &recent_words, |w| {
                                    is_bare_verb_form(w) && is_likely_transitive_verb(w)
                                })
                            } else if after_modal {
                                lex.pick_cover_filtered(rng, slot, &recent_words, |w| is_bare_verb_form(w))
                            } else if want_transitive {
                                lex.pick_cover_filtered(rng, slot, &recent_words, |w| {
                                    is_likely_transitive_verb(w)
                                })
                            } else {
                                None
                            };

                            constrained.unwrap_or_else(|| lex.pick_cover(rng, slot, &recent_words))
                        } else if slot == Pos::To {
                            "to".to_string()
                        } else if slot == Pos::Prefix {
                            // Prefix words are always cover words (not payload)
                            lex.pick_cover(rng, slot, &recent_words)
                        } else if slot == Pos::N {
                            let num = noun_number.get(&i).copied().unwrap_or(Number::Singular);
                            match num {
                                Number::Singular => lex.pick_cover(rng, slot, &recent_words),
                                Number::Plural => {
                                    // Attempt a few times to find a plural that won't collide with BIP39.
                                    const MAX_TRIES: usize = 8;
                                    let mut chosen = None;
                                    for _ in 0..MAX_TRIES {
                                        let base = lex.pick_cover(rng, slot, &recent_words);
                                        let plural = pluralize_cover_noun(&base);
                                        let plural_lc = plural.to_lowercase();
                                        if !lex.bip39_set.contains(&plural_lc)
                                            && !recent_words.iter().any(|&rw| rw == plural_lc.as_str())
                                        {
                                            chosen = Some(plural);
                                            break;
                                        }
                                    }
                                    chosen.unwrap_or_else(|| lex.pick_cover(rng, slot, &recent_words))
                                }
                            }
                        } else {
                            lex.pick_cover(rng, slot, &recent_words)
                        };
                        out.push(cover_word);
                    }
                }
            }
        }
    }
    
    // At the end, advance payload_i to point to the first unused word
    // This ensures we don't re-check words we've already used
    while *payload_i < payload.len() && used_payload_indices.contains(payload_i) {
        *payload_i += 1;
    }

    out
}

/// Compute k candidates based on the length mode.
/// Returns a vector of k values to try in order.
fn compute_k_candidates<R: Rng>(
    rng: &mut R,
    cache: &SequenceCache,
    start_symbol: &str,
    k_min: usize,
    k_max: usize,
    length_mode: SentenceLengthMode,
    require_prefix: bool,
) -> Vec<usize> {
    match length_mode {
        SentenceLengthMode::Compact => {
            // Compact mode: try k from k_min to k_max, shortest first
            let k_start = if require_prefix { k_min + 1 } else { k_min };
            (k_start..=k_max).collect()
        }
        SentenceLengthMode::Natural => {
            // Natural mode: sample k from grammar's length distribution
            // Ignore k_min and sample from all available k values up to k_max
            // Start from k=1 (k=0 would be empty sequence, not useful)
            let natural_k_start = if require_prefix { 2 } else { 1 }; // k=1 can't have Prefix, need at least k=2
            
            // Compute weights for each k (ignoring k_min)
            let mut k_weights: Vec<(usize, f64)> = Vec::new();
            for k in natural_k_start..=k_max {
                if let Some(sequences) = cache.get(start_symbol, k) {
                    let weight: f64 = if require_prefix {
                        sequences.iter()
                            .filter(|seq_prob| !seq_prob.sequence.is_empty() && seq_prob.sequence[0] == Pos::Prefix)
                            .map(|seq_prob| seq_prob.probability)
                            .sum()
                    } else {
                        sequences.iter()
                            .map(|seq_prob| seq_prob.probability)
                            .sum()
                    };
                    if weight > 0.0 {
                        k_weights.push((k, weight));
                    }
                }
            }
            
            if k_weights.is_empty() {
                // Fallback to compact mode if no weights found
                let k_start = if require_prefix { k_min + 1 } else { k_min };
                return (k_start..=k_max).collect();
            }
            
            // Sample one k from the distribution
            let total_weight: f64 = k_weights.iter().map(|(_, w)| w).sum();
            if total_weight <= 0.0 {
                // Fallback to compact mode if total weight is zero
                let k_start = if require_prefix { k_min + 1 } else { k_min };
                return (k_start..=k_max).collect();
            }
            
            let mut r = rng.gen::<f64>() * total_weight;
            let mut sampled_k = None;
            for (k, weight) in &k_weights {
                if r <= *weight {
                    sampled_k = Some(*k);
                    break;
                }
                r -= weight;
            }
            let sampled_k = sampled_k.unwrap_or_else(|| k_weights[0].0);
            
            // Build candidate list:
            // 1. Sampled k first
            // 2. Remaining k's in descending weight order
            // 3. Compact fallback (k_min..=k_max) for robustness
            
            let mut candidates = vec![sampled_k];
            
            // Add remaining k's in descending weight order (excluding sampled_k)
            let mut remaining: Vec<(usize, f64)> = k_weights.iter()
                .filter(|(k, _)| *k != sampled_k)
                .cloned()
                .collect();
            remaining.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            candidates.extend(remaining.into_iter().map(|(k, _)| k));
            
            // Add compact fallback for robustness (only k's not already in candidates)
            // Still respect k_min for the fallback to ensure we don't miss any valid k values
            let candidates_set: std::collections::HashSet<usize> = candidates.iter().cloned().collect();
            let k_start = if require_prefix { k_min + 1 } else { k_min };
            for k in k_start..=k_max {
                if !candidates_set.contains(&k) {
                    candidates.push(k);
                }
            }
            
            candidates
        }
    }
}

/// Generate sentences until all payload tokens are embedded.
/// Returns (formatted_text, payload_set) where formatted_text has BIP39 words highlighted according to highlight_mode.
fn generate_text<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    payload: &[PayloadTok],
    highlight_mode: HighlightMode,
    verbose: bool,
    mode: GenerationMode,
    k_min: usize,
    k_max: usize,
    length_mode: SentenceLengthMode,
) -> (String, HashSet<String>) {
    let mut words: Vec<String> = Vec::new();
    let mut payload_i: usize = 0;

    // Build payload set for highlighting
    let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();

    // Load precomputed sequences
    let cache = match SequenceCache::load(mode, k_max, verbose) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading sequence cache: {}", e);
            eprintln!("Falling back to random generation");
            // Fall back to old random generation - but we still need to update fill_slots calls
            // For now, just panic - we'll handle this better later
            panic!("Sequence cache required for new algorithm");
        }
    };

    // For subject mode, generate a single sentence with all payload words
    // For body mode, generate multiple sentences as before
    if mode == GenerationMode::Subject {
        // Generate sentences until all payload words are embedded
        // Keep generating sentences and concatenating them until all words are used
        let mut all_sentence_words: Vec<String> = Vec::new();
        let mut current_payload_i = 0;
        let mut prev_words_strings: Vec<String> = Vec::new(); // Store owned strings
        let mut sentence_count = 0;
        const MAX_SENTENCES: usize = 100; // Safety limit to prevent infinite loops
        
        while current_payload_i < payload.len() && sentence_count < MAX_SENTENCES {
            sentence_count += 1;
            
            // Get the next payload word's POS for start_symbol selection
            // For the first sentence in subject mode, decide if we want a prefix (30% chance)
            let (start_symbol, want_prefix) = if sentence_count == 1 && mode == GenerationMode::Subject {
                let want_prefix = rng.gen_bool(0.3);  // Match grammar: S = (0.3: Prefix SContent) | (0.7: SContent)
                ("S", want_prefix)  // First sentence: use "S" which includes (0.3: Prefix SContent) option
            } else if current_payload_i < payload.len() {
                let next_word = &payload[current_payload_i];
                if next_word.allowed.is_empty() {
                    panic!(
                        "BUG: Payload word '{}' has no allowed POS tags!\n\
                         This indicates a POS tagging failure. Check:\n\
                         1. Is '{}' in bip39_POS.txt?\n\
                         2. Does it have POS tags assigned?\n\
                         3. Is the POS tag parsing working correctly?",
                        next_word.word,
                        next_word.word
                    );
                }
                
                let pos = if next_word.allowed.contains(&Pos::N) {
                    Pos::N
                } else if next_word.allowed.contains(&Pos::V) {
                    Pos::V
                } else if next_word.allowed.contains(&Pos::Adj) {
                    Pos::Adj
                } else if next_word.allowed.contains(&Pos::Adv) {
                    Pos::Adv
                } else if next_word.allowed.contains(&Pos::Prep) {
                    Pos::Prep
                } else {
                    next_word.allowed.iter().next().copied().expect("Payload word should have at least one POS tag")
                };
                
                let nt = start_nonterminal_for_pos(pos);
                let g = get_grammar(mode);
                let symbol = if g.rules.contains_key(nt) {
                    nt
                } else {
                    // Fallback: for subsequent sentences in subject mode, use POS-specific start
                    // to avoid Prefix. For first sentence or body mode, "S" is fine.
                    if sentence_count > 1 && mode == GenerationMode::Subject {
                        // Try to find any POS-specific start symbol that exists (subject grammar may have S_* variants)
                        // This ensures we don't get Prefix in subsequent sentences
                        let alternatives = ["S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"];
                        alternatives.iter()
                            .find(|&&alt| g.rules.contains_key(alt))
                            .copied()
                            .unwrap_or("S")  // Fallback to S (body grammar only uses S)
                    } else {
                        "S"
                    }
                };
                (symbol, false)  // Subsequent sentences never want prefix
            } else {
                // No more payload words - use "S" for body mode, but for subject mode
                // subsequent sentences, prefer non-Prefix start symbols
                let symbol = if sentence_count > 1 && mode == GenerationMode::Subject {
                    let g = get_grammar(mode);
                    // Try POS-specific start symbols (subject grammar may have S_* variants)
                    let alternatives = ["S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"];
                    alternatives.iter()
                        .find(|&&alt| g.rules.contains_key(alt))
                        .copied()
                        .unwrap_or("S")  // Fallback to S (body grammar only uses S)
                } else {
                    "S"
                };
                (symbol, false)  // No payload words remaining, no prefix
            };
            
            // Compute k candidates based on length mode
            let k_candidates = compute_k_candidates(
                rng,
                &cache,
                start_symbol,
                k_min,
                k_max,
                length_mode,
                want_prefix,
            );
            let mut planned = None;
            for k in k_candidates {
                if let Some((slots, forced_placements, j)) = plan_sentence(
                    rng,
                    &cache,
                    start_symbol,
                    k,
                    payload,
                    current_payload_i,
                    want_prefix,
                ) {
                    planned = Some((slots, forced_placements, j));
                    break; // Found a plan, use it
                }
            }
            
            // If we wanted a prefix but didn't find one, fall back to non-prefix
            if planned.is_none() && want_prefix {
                let k_candidates_fallback = compute_k_candidates(
                    rng,
                    &cache,
                    start_symbol,
                    k_min,
                    k_max,
                    length_mode,
                    false,  // Don't require prefix in fallback
                );
                for k in k_candidates_fallback {
                    if let Some((slots, forced_placements, j)) = plan_sentence(
                        rng,
                        &cache,
                        start_symbol,
                        k,
                        payload,
                        current_payload_i,
                        false,  // Don't require prefix in fallback
                    ) {
                        planned = Some((slots, forced_placements, j));
                        break;
                    }
                }
            }
            
            let (slots, forced_placements, _j) = match planned {
                Some(p) => p,
                None => {
                    // Fallback: generate minimal sentence structure to always embed the word
                    // This preserves payload order even if it results in grammar errors
                    let word_name = if current_payload_i < payload.len() {
                        payload[current_payload_i].word.as_str()
                    } else {
                        "unknown"
                    };
                    if verbose {
                        eprintln!("Warning: Could not plan sentence for word '{}' (index {}). Using fallback structure (may have grammar errors).", 
                                 word_name, current_payload_i);
                    }
                    match generate_fallback_sentence(payload, current_payload_i, mode) {
                        Some((fallback_slots, fallback_placements)) => {
                            (fallback_slots, fallback_placements, 1)
                        }
                        None => {
                            // This should never happen, but if it does, panic rather than skip
                            panic!("BUG: Cannot generate fallback sentence for word '{}' at index {}. This should never happen.", 
                                   word_name, current_payload_i);
                        }
                    }
                }
            };
            
            let payload_i_before = current_payload_i;
            // Advance payload_i to account for forced placements (they're in order)
            let max_forced_idx = forced_placements.values().max().copied().unwrap_or(current_payload_i.saturating_sub(1));
            let mut temp_payload_i = (max_forced_idx + 1).max(current_payload_i);
            
            // Convert prev_words_strings to slice for fill_slots
            let prev_words_refs: Vec<&str> = prev_words_strings.iter().map(|s| s.as_str()).collect();
            let mut sentence_words = fill_slots(
                rng, 
                lex, 
                &slots, 
                payload, 
                &mut temp_payload_i,
                &prev_words_refs,
                None, // start_pos not needed with forced placements
                Some(&forced_placements)
            );
            
            // Update current_payload_i to reflect what was actually used
            current_payload_i = temp_payload_i.max(max_forced_idx + 1);
            
            // Capitalize the first word of the first sentence only
            if all_sentence_words.is_empty() {
                if let Some(first) = sentence_words.first_mut() {
                    *first = capitalize(first);
                }
            }
            
            // Update prev_words_strings with last few words from this sentence for next iteration
            // Extract strings before appending to avoid lifetime issues
            let start_idx = sentence_words.len().saturating_sub(3);
            prev_words_strings = sentence_words[start_idx..].iter().cloned().collect();
            
            // Append this sentence to all sentences
            all_sentence_words.append(&mut sentence_words);
            
            // If no progress was made, break to avoid infinite loop
            if current_payload_i == payload_i_before {
                if verbose {
                    eprintln!("Warning: No progress embedding words. Stopping at {}/{} words embedded.", current_payload_i, payload.len());
                }
                break;
            }
        }
        
        // Verify all payload words were embedded
        if current_payload_i < payload.len() {
            if verbose {
                eprintln!("Warning: Not all payload words embedded in subject mode. Embedded {}/{} after {} sentences", current_payload_i, payload.len(), sentence_count);
            }
        }
        
        let mut sentence_words = all_sentence_words;
        
        // Note: Verbose grammar/word mapping printing skipped for subject mode with multiple sentences
        // (would be complex to map multiple sentence structures)
        
        // First word should already be capitalized (done in loop), but ensure it's capitalized
        if let Some(first) = sentence_words.first_mut() {
            *first = capitalize(first);
        }

            // Print sentence as it's generated if verbose
            if verbose {
                let sentence_text: String = sentence_words.iter()
                    .map(|w| {
                        let word_clean = normalize_token_for_bip39(w);
                        if !word_clean.is_empty() && payload_set.contains(&word_clean) {
                        match highlight_mode {
                            HighlightMode::None => w.clone(),
                            HighlightMode::Bars => wrap_payload_with_bars(w),
                            HighlightMode::Color(color) => wrap_payload_with_color(w, color),
                            HighlightMode::Madlib => {
                                    // Find the payload token to get its POS tag
                                    if let Some(payload_tok) = payload.iter().find(|t| t.word.to_lowercase() == word_clean) {
                                        // Use the first allowed POS tag for madlib
                                        if let Some(&first_pos) = payload_tok.allowed.iter().next() {
                                            let pos_str = match first_pos {
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
                                                Pos::Conj => "Conj",
                                                Pos::Dot => "Dot",
                                                Pos::Prefix => "Prefix",
                                            };
                                            let punct: String = w.chars().filter(|c| !c.is_alphabetic()).collect();
                                            format!("[{}]{}", pos_str, punct)
                                        } else {
                                            w.clone()
                                        }
                                    } else {
                                        w.clone()
                                    }
                                }
                            }
                        } else {
                            w.clone()
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(" ");
                eprintln!("{}", sentence_text);
            }
        
        words = sentence_words;
    } else {
        // Body mode: Keep generating sentences until all payload tokens are embedded
        let mut sentence_count = 0;
        const MAX_SENTENCES: usize = 200; // Safety limit to prevent infinite loops
        while payload_i < payload.len() && sentence_count < MAX_SENTENCES {
            sentence_count += 1;
        // Make each sentence size adapt to remaining needs.
        let remaining_payload = payload.len().saturating_sub(payload_i);
        // Adapt sentence length based on remaining payload
        let _sentence_min = if remaining_payload > 10 {
            18
        } else if remaining_payload > 5 {
            14
        } else {
            5
        };

        // Get the next payload word's POS for start_symbol selection
        let next_word = if payload_i < payload.len() {
            Some(&payload[payload_i])
        } else {
            None
        };
        
        let start_symbol = if let Some(next_word) = next_word {
            // Panic if the payload word has no allowed POS tags - this indicates a POS tagging failure
            if next_word.allowed.is_empty() {
                panic!(
                    "BUG: Payload word '{}' has no allowed POS tags!\n\
                     This indicates a POS tagging failure. Check:\n\
                     1. Is '{}' in bip39_POS.txt?\n\
                     2. Does it have POS tags assigned?\n\
                     3. Is the POS tag parsing working correctly?",
                    next_word.word,
                    next_word.word
                );
            }
            
            let pos = if next_word.allowed.contains(&Pos::N) {
                Pos::N
            } else if next_word.allowed.contains(&Pos::V) {
                Pos::V
            } else if next_word.allowed.contains(&Pos::Adj) {
                Pos::Adj
            } else if next_word.allowed.contains(&Pos::Adv) {
                Pos::Adv
            } else if next_word.allowed.contains(&Pos::Prep) {
                Pos::Prep
            } else {
                next_word.allowed.iter().next().copied().expect("Payload word should have at least one POS tag")
            };
            
            let nt = start_nonterminal_for_pos(pos);
            let g = get_grammar(mode);
            if g.rules.contains_key(nt) {
                nt
            } else {
                "S"
            }
        } else {
            "S"
        };

        // Pass the last few words from previous sentence to prevent repetition across sentences
        const REPETITION_WINDOW: usize = 3;
        let prev_words: Vec<String> = words
            .iter()
            .rev()
            .take(REPETITION_WINDOW)
            .map(|s| {
                s.trim_end_matches('.').trim_end_matches(' ').to_lowercase()
            })
            .rev()
            .collect();
        let prev_words_refs: Vec<&str> = prev_words.iter().map(|s| s.as_str()).collect();
        let payload_i_before = payload_i;
        
        // Compute k candidates based on length mode (body mode never requires prefix)
        let k_candidates = compute_k_candidates(
            rng,
            &cache,
            start_symbol,
            k_min,
            k_max,
            length_mode,
            false,  // Body mode never requires prefix
        );
        let mut planned = None;
        for k in k_candidates {
            if let Some((slots, forced_placements, j)) = plan_sentence(
                rng,
                &cache,
                start_symbol,
                k,
                payload,
                payload_i,
                false,  // Body mode never requires prefix
            ) {
                planned = Some((slots, forced_placements, j));
                break; // Found a plan, use it
            }
        }
        
        // If planning failed with preferred start_symbol, try fallback with "S"
        if planned.is_none() && start_symbol != "S" {
            let k_candidates_fallback = compute_k_candidates(
                rng,
                &cache,
                "S",
                k_min,
                k_max,
                length_mode,
                false,  // Body mode never requires prefix
            );
            for k in k_candidates_fallback {
                if let Some((slots, forced_placements, j)) = plan_sentence(
                    rng,
                    &cache,
                    "S",
                    k,
                    payload,
                    payload_i,
                    false,  // Body mode never requires prefix
                ) {
                    planned = Some((slots, forced_placements, j));
                    break;
                }
            }
        }
        
        // If still no plan, try other POS tags from the word's allowed tags
        if planned.is_none() {
            if let Some(next_word) = next_word {
            for &alt_pos in &next_word.allowed {
                if alt_pos == Pos::N || alt_pos == Pos::V || alt_pos == Pos::Adj || alt_pos == Pos::Adv || alt_pos == Pos::Prep {
                    let alt_nt = start_nonterminal_for_pos(alt_pos);
                    let g = get_grammar(mode);
                    let alt_start = if g.rules.contains_key(alt_nt) {
                        alt_nt
                    } else {
                        "S"
                    };
                    
                    let k_candidates_alt = compute_k_candidates(
                        rng,
                        &cache,
                        alt_start,
                        k_min,
                        k_max,
                        length_mode,
                        false,  // Body mode never requires prefix
                    );
                    for k in k_candidates_alt {
                        if let Some((slots, forced_placements, j)) = plan_sentence(
                            rng,
                            &cache,
                            alt_start,
                            k,
                            payload,
                            payload_i,
                            false,  // Body mode never requires prefix
                        ) {
                            planned = Some((slots, forced_placements, j));
                            break;
                        }
                    }
                    if planned.is_some() {
                        break;
                    }
                }
            }
            }
        }
        
        let (slots, forced_placements, _j) = match planned {
            Some(p) => p,
            None => {
                // Fallback: generate minimal sentence structure to always embed the word
                // This preserves payload order even if it results in grammar errors
                let word_name = if payload_i < payload.len() {
                    payload[payload_i].word.as_str()
                } else {
                    "unknown"
                };
                if verbose {
                    eprintln!("Warning: Could not plan sentence for word '{}' (index {}). Using fallback structure (may have grammar errors).", 
                             word_name, payload_i);
                }
                match generate_fallback_sentence(payload, payload_i, mode) {
                    Some((fallback_slots, fallback_placements)) => {
                        (fallback_slots, fallback_placements, 1)
                    }
                    None => {
                        // This should never happen, but if it does, panic rather than skip
                        panic!("BUG: Cannot generate fallback sentence for word '{}' at index {}. This should never happen.", 
                               word_name, payload_i);
                    }
                }
            }
        };
        
        // Advance payload_i to account for forced placements
        let max_forced_idx = forced_placements.values().max().copied().unwrap_or(payload_i_before.saturating_sub(1));
        let mut temp_payload_i = (max_forced_idx + 1).max(payload_i_before);
        
        let mut sentence_words = fill_slots(
            rng, 
            lex, 
            &slots, 
            payload, 
            &mut temp_payload_i,
            &prev_words_refs,
            None, // start_pos not needed with forced placements
            Some(&forced_placements)
        );
        
        // Update payload_i to reflect what was actually used
        payload_i = temp_payload_i.max(max_forced_idx + 1);
        
        // Check if payload word was placed - this should always be true with forced placements
        if payload_i <= payload_i_before && forced_placements.is_empty() {
            let slots_str: Vec<String> = slots.iter().map(|pos| {
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
                        Pos::Conj => "Conj".to_string(),
                    Pos::Dot => "Dot".to_string(),
                    Pos::Prefix => "Prefix".to_string(),
                }
            }).collect();
            
            let next_payload_word = if payload_i_before < payload.len() {
                format!("{} (allowed POS: {:?})", payload[payload_i_before].word, payload[payload_i_before].allowed)
            } else {
                "none".to_string()
            };
            
            panic!(
                "BUG: Generated sentence with no payload words!\n\
                 Start symbol: {}\n\
                 Next payload word: {}\n\
                 Generated slots: {}\n\
                 Sentence: {}\n\
                 This should never happen - the planner should guarantee payload word placement.",
                start_symbol,
                next_payload_word,
                slots_str.join(" "),
                sentence_words.join(" ")
            );
        }
        
        payload_i = temp_payload_i.max(payload_i);

        // Print grammar structure in verbose mode
        if verbose {
            let grammar_str: Vec<String> = slots.iter().map(|pos| {
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
                    Pos::Conj => "Conj".to_string(),
                    Pos::Dot => "Dot".to_string(),
                    Pos::Prefix => "Prefix".to_string(),
                }
            }).collect();
            eprintln!("Grammar: {}", grammar_str.join(" "));
        }

        // Print actual word-to-POS mapping in verbose mode
        if verbose {
            let mut word_pos_mapping: Vec<String> = Vec::new();
            let mut word_idx = 0;
            let mut current_payload_idx = payload_i_before;
            for &slot in slots.iter() {
                if slot == Pos::Dot {
                    continue; // Skip Dot, punctuation is attached to previous word
                }
                if word_idx < sentence_words.len() {
                    let word_with_punct = &sentence_words[word_idx];
                    let word_clean = word_with_punct.trim_end_matches('.').to_lowercase();
                    let pos_str = match slot {
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
                        Pos::Conj => "Conj",
                        Pos::Dot => "Dot", // Shouldn't happen here
                        Pos::Prefix => "Prefix",
                    };
                    // Mark payload words with * and show their allowed POS tags
                    if payload_set.contains(&word_clean) && current_payload_idx < payload.len() {
                        let payload_tok = &payload[current_payload_idx];
                        let allowed_pos: Vec<String> = payload_tok.allowed.iter().map(|p| {
                            match p {
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
                                Pos::Conj => "Conj",
                                Pos::Dot => "Dot",
                                Pos::Prefix => "Prefix",
                            }.to_string()
                        }).collect();
                        word_pos_mapping.push(format!("{}*:{}[{}]", word_clean, pos_str, allowed_pos.join(",")));
                        current_payload_idx += 1;
                    } else {
                        word_pos_mapping.push(format!("{}:{}", word_clean, pos_str));
                    }
                    word_idx += 1;
                }
            }
            eprintln!("Words:   {}", word_pos_mapping.join(" "));
        }

        // Only add the sentence if it contains at least one payload word
        if payload_i > payload_i_before {
            // Capitalize the first word of the sentence.
            if let Some(first) = sentence_words.first_mut() {
                *first = capitalize(first);
            }

            // Print sentence as it's generated if verbose and single variation
            if verbose {
                let sentence_text: String = sentence_words.iter()
                    .map(|w| {
                        let word_clean = normalize_token_for_bip39(w);
                        if !word_clean.is_empty() && payload_set.contains(&word_clean) {
                        match highlight_mode {
                            HighlightMode::None => w.clone(),
                            HighlightMode::Bars => wrap_payload_with_bars(w),
                            HighlightMode::Color(color) => wrap_payload_with_color(w, color),
                            HighlightMode::Madlib => {
                                    // Find the payload token to get its POS tag
                                    if let Some(payload_tok) = payload.iter().find(|t| t.word.to_lowercase() == word_clean) {
                                        // Use the first allowed POS tag for madlib
                                        if let Some(&first_pos) = payload_tok.allowed.iter().next() {
                                            let pos_str = match first_pos {
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
                                                Pos::Conj => "Conj",
                                                Pos::Dot => "Dot",
                                                Pos::Prefix => "Prefix",
                                            };
                                            let punct: String = w.chars().filter(|c| !c.is_alphabetic()).collect();
                                            format!("[{}]{}", pos_str, punct)
                                        } else {
                                            w.clone()
                                        }
                                    } else {
                                        w.clone()
                                    }
                                }
                            }
                        } else {
                            w.clone()
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(" ");
                eprintln!("{}", sentence_text);
            }

            // Add spacing between sentences.
            if !words.is_empty() {
                // ensure previous ended with punctuation. (We put '.' on last token)
            }
            words.append(&mut sentence_words);
        } else {
            // Sentence contained no payload words - skip it
            // Reset payload_i since we didn't actually use this sentence
            payload_i = payload_i_before;
            if verbose {
                eprintln!("Skipping sentence with no payload words");
            }
        }
        }
    }

    // Post-fix: ensure output ends with a period (only for body mode, not subject mode)
    if mode == GenerationMode::Body {
        if let Some(last) = words.last_mut() {
            if !last.ends_with('.') {
                last.push('.');
            }
        }
    }

    // Apply highlighting to BIP39 words according to highlight_mode.
    let rendered_words: Vec<String> = if highlight_mode == HighlightMode::Madlib {
        // For madlib mode, we need slot information which isn't available here
        // So we'll handle it differently - replace payload words with [POS] based on payload tokens
        words.iter().map(|word| {
            let word_clean = normalize_token_for_bip39(word);
            if !word_clean.is_empty() && payload_set.contains(&word_clean) {
                // Find the payload token to get its POS tag
                if let Some(payload_tok) = payload.iter().find(|t| t.word.to_lowercase() == word_clean) {
                    // Use the first allowed POS tag for madlib
                    if let Some(&first_pos) = payload_tok.allowed.iter().next() {
                        let pos_str = match first_pos {
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
                            Pos::Conj => "Conj",
                            Pos::Dot => "Dot",
                            Pos::Prefix => "Prefix",
                        };
                        // Preserve punctuation
                        let punct: String = word.chars().filter(|c| !c.is_alphabetic()).collect();
                        format!("[{}]{}", pos_str, punct)
                    } else {
                        word.clone()
                    }
                } else {
                    word.clone()
                }
            } else {
                word.clone()
            }
        }).collect()
    } else {
        words.iter().map(|word| {
            let word_clean = normalize_token_for_bip39(word);
            if !word_clean.is_empty() && payload_set.contains(&word_clean) {
                match highlight_mode {
                    HighlightMode::None => word.clone(),
                    HighlightMode::Bars => wrap_payload_with_bars(word),
                    HighlightMode::Color(color) => wrap_payload_with_color(word, color),
                    HighlightMode::Madlib => unreachable!(), // Handled above
                }
            } else {
                word.clone()
            }
        }).collect()
    };

    (rendered_words.join(" "), payload_set)
}

/// Word wrap text to a specified line width, preserving sentence boundaries
fn word_wrap(text: &str, width: usize) -> String {
    let mut result = Vec::new();
    let mut current_line = String::new();
    
    // Split by sentences first to preserve sentence boundaries
    let sentences: Vec<&str> = text.split_inclusive('.').collect();
    
    for sentence in sentences {
        let trimmed_sentence = sentence.trim();
        if trimmed_sentence.is_empty() {
            continue;
        }
        
        // If adding this sentence would exceed width, start a new line
        if !current_line.is_empty() {
            let test_line = format!("{} {}", current_line, trimmed_sentence);
            if test_line.len() > width {
                result.push(current_line.clone());
                current_line = trimmed_sentence.to_string();
            } else {
                current_line = test_line;
            }
        } else {
            current_line = trimmed_sentence.to_string();
        }
        
        // If current line exceeds width, wrap it word by word
        if current_line.len() > width {
            let words: Vec<String> = current_line.split_whitespace().map(|s| s.to_string()).collect();
            current_line.clear();
            
            for word in words {
                if current_line.is_empty() {
                    current_line = word;
                } else {
                    let test_line = format!("{} {}", current_line, word);
                    if test_line.len() > width {
                        result.push(current_line.clone());
                        current_line = word;
                    } else {
                        current_line = test_line;
                    }
                }
            }
        }
    }
    
    // Add any remaining line
    if !current_line.is_empty() {
        result.push(current_line);
    }
    
    result.join("\n")
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn normalize_token_for_bip39(s: &str) -> String {
    // Decoder is case-insensitive; BIP39 words are lowercase ASCII a-z.
    // Strip ANSI escape codes (e.g., \x1b[1m, \x1b[0m), highlighting bars (|), and punctuation.
    let mut result = s.to_string();
    
    // Remove ANSI escape codes: ESC[ followed by digits and 'm'
    // Pattern: \x1b[ or ESC[ followed by optional digits and 'm'
    result = regex::Regex::new(r"\x1b\[[0-9;]*m")
        .unwrap()
        .replace_all(&result, "")
        .to_string();
    
    // Remove highlighting bars
    result = result.replace('|', "");
    
    // Strip leading/trailing non-letters to tolerate punctuation/quotes
    result = result.trim()
        .trim_matches(|c: char| !c.is_ascii_alphabetic())
        .to_lowercase();
    
    result
}

fn wrap_payload_with_bars(word_with_punct: &str) -> String {
    // Surround the word token with | | while keeping trailing punctuation outside the bars.
    // Example: "abandon." -> "|abandon|."
    // Example: "Abandon"  -> "|Abandon|"
    let mut core = word_with_punct.to_string();
    let mut suffix = String::new();
    while let Some(last) = core.chars().last() {
        if last.is_ascii_alphabetic() {
            break;
        }
        core.pop();
        suffix.insert(0, last);
    }
    format!("|{core}|{suffix}")
}

fn wrap_payload_with_color(word_with_punct: &str, color_code: u8) -> String {
    // Highlight the word using ANSI escape codes while keeping trailing punctuation outside.
    // Example: "abandon." -> "\x1b[32mabandon\x1b[0m."
    // Example: "Abandon"  -> "\x1b[32mAbandon\x1b[0m"
    // ANSI color codes: 30 = black, 31 = red, 32 = green, 33 = yellow, 34 = blue, 35 = magenta, 36 = cyan, 37 = white
    let mut core = word_with_punct.to_string();
    let mut suffix = String::new();
    while let Some(last) = core.chars().last() {
        if last.is_ascii_alphabetic() {
            break;
        }
        core.pop();
        suffix.insert(0, last);
    }
    format!("\x1b[{}m{core}\x1b[0m{suffix}", color_code)
}

/// Build comprehensive POS mapping for all BIP39 words.
/// Returns a HashMap mapping each word to its allowed POS tags.
/// Reads POS tags from the file format: word|POS1,POS2,...
fn build_pos_mapping(language: &str) -> Result<HashMap<String, Vec<Pos>>, String> {
    let wordlist_path = get_wordlist_path(language)?;
    let wordlist = std::fs::read_to_string(&wordlist_path)
        .map_err(|e| format!("Failed to read wordlist file '{}': {}", wordlist_path, e))?;
    let mut mapping = HashMap::new();
    
    for line in wordlist.lines() {
        let line: &str = line.trim();
        if line.is_empty() {
            continue;
        }
        
        // Parse format: word|POS1,POS2,...
        // Require explicit POS tags - skip words without tags
        let (word, pos_tags) = if let Some(pipe_idx) = line.find('|') {
            let word = line[..pipe_idx].trim().to_lowercase();
            let pos_str = line[pipe_idx + 1..].trim();
            let pos_tags = parse_pos_tags(pos_str);
            (word, pos_tags)
        } else {
            // No POS tags - skip this word (require explicit tags)
            continue;
        };
        
        // Only add words with valid POS tags
        if !word.is_empty() && !pos_tags.is_empty() {
            mapping.insert(word, pos_tags);
        }
    }
    
    Ok(mapping)
}

/// Parse POS tags from a comma-separated string (e.g., "Prep,Adv" -> [Pos::Prep, Pos::Adv]).
fn parse_pos_tags(pos_str: &str) -> Vec<Pos> {
    pos_str
        .split(',')
        .map(|s| s.trim())
        .filter_map(|s| match s {
            "Det" => Some(Pos::Det),
            "Adj" => Some(Pos::Adj),
            "N" => Some(Pos::N),
            "V" => Some(Pos::V),
            "Modal" => Some(Pos::Modal),
            "Aux" => Some(Pos::Aux),
            "Cop" => Some(Pos::Cop),
            "To" => Some(Pos::To),
            "Prep" => Some(Pos::Prep),
            "Adv" => Some(Pos::Adv),
            "Conj" => Some(Pos::Conj),
            "Dot" => Some(Pos::Dot),
            "Prefix" => Some(Pos::Prefix),
            _ => None,
        })
        .collect()
}


/// Get POS tags for a word from the comprehensive mapping.
/// Returns a vector of allowed POS tags.
/// Used for tagging payload (BIP39) words. Cover words use explicit POS tags from cover_POS.txt.
fn tag_word(word: &str) -> Vec<Pos> {
    // For now, use English POS mapping for tagging payload words
    // TODO: Make this language-aware if needed
    static POS_MAP: OnceLock<HashMap<String, Vec<Pos>>> = OnceLock::new();
    
    let mapping = POS_MAP.get_or_init(|| {
        // Default to English for POS tagging
        build_pos_mapping("english").unwrap_or_else(|_| HashMap::new())
    });
    
    let word_lower = word.to_lowercase();
    // Require explicit POS tags - return empty if word not found
    mapping.get(&word_lower).cloned().unwrap_or_default()
}

/// Load all BIP39 words from the wordlist file.
/// Handles both formats: word|POS (new) and word (old, backward compatibility).
fn load_bip39_words(language: &str) -> Result<Vec<String>, String> {
    let wordlist_path = get_wordlist_path(language)?;
    let wordlist = std::fs::read_to_string(&wordlist_path)
        .map_err(|e| format!("Failed to read wordlist file '{}': {}", wordlist_path, e))?;
    Ok(wordlist
        .lines()
        .map(|line: &str| {
            let line = line.trim();
            // Extract word part before | if present
            if let Some(pipe_idx) = line.find('|') {
                line[..pipe_idx].trim().to_string()
            } else {
                line.to_string()
            }
        })
        .filter(|word: &String| !word.is_empty())
        .collect())
}


/// Load cover words with POS tags from cover_POS.txt
/// Returns a HashMap mapping POS to Vec of words
fn load_cover_words_by_pos(bip39_set: &HashSet<String>) -> HashMap<Pos, Vec<String>> {
    let cover_words = include_str!("../languages/english/cover_POS.txt");
    let mut by_pos: HashMap<Pos, Vec<String>> = HashMap::new();
    
    for line in cover_words.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        
        // Parse format: word|POS1,POS2,... or just word
        let (word, pos_tags) = if let Some(pipe_idx) = line.find('|') {
            let word = line[..pipe_idx].trim();
            let pos_str = line[pipe_idx + 1..].trim();
            let pos_tags = parse_pos_tags(pos_str);
            // If no POS tags specified, skip (we want explicit tags for function words)
            if pos_tags.is_empty() {
                continue;
            }
            (word.to_string(), pos_tags)
        } else {
            // No explicit POS tags - skip (we want explicit tags for function words)
            continue;
        };
        
        // Skip if word is in BIP39 set
        if bip39_set.contains(&word.to_lowercase()) {
            continue;
        }
        
        // Add word to each POS category
        for pos in pos_tags {
            by_pos.entry(pos).or_insert_with(Vec::new).push(word.clone());
        }
    }
    
    // Deduplicate and sort each category
    for words in by_pos.values_mut() {
        words.sort();
        words.dedup();
    }
    
    by_pos
}

/// Randomly select N words from the BIP39 wordlist.
fn select_random_words<R: Rng>(rng: &mut R, count: usize, language: &str) -> Result<Vec<String>, String> {
    let all_words = load_bip39_words(language)?;
    if all_words.is_empty() || count == 0 {
        return Ok(Vec::new());
    }

    // Sample WITH replacement so duplicates are possible (and therefore decodable).
    let mut selected = Vec::with_capacity(count);
    for _ in 0..count {
        selected.push(all_words.choose(rng).unwrap().clone());
    }
    Ok(selected)
}

/// Validate that cover words do not overlap with BIP39 words.
/// Panics if any overlap is found.
fn validate_cover_words(cover_words: &[&str], bip39_set: &HashSet<String>) {
    let mut overlaps = Vec::new();
    
    for word in cover_words {
        let word_lower = word.to_lowercase();
        if bip39_set.contains(&word_lower) {
            overlaps.push(word_lower.clone());
        }
    }
    
    if !overlaps.is_empty() {
        eprintln!("ERROR: Cover words overlap with BIP39 words:");
        for word in &overlaps {
            eprintln!("  - '{}'", word);
        }
        eprintln!();
        eprintln!("This would break decoding! Remove these words from the cover lexicon.");
        std::process::exit(1);
    }
}

fn print_usage(program_name: &str) {
    eprintln!("Usage: {} [OPTIONS] [<word1> <word2> ... <wordN>]", program_name);
    eprintln!();
    eprintln!("Generate natural sentences embedding BIP39 words in-order.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  <word1> <word2> ...    BIP39 words to embed (positional, optional if --random used)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --random <N>            Generate sentences from N random BIP39 words");
    eprintln!("  --grammar <grammar>      Grammar to use: 'subject' (default) or 'body'");
    eprintln!("                          subject: Short sentences, may include prefixes (Re:, Fwd:, etc.)");
    eprintln!("                          body: Longer sentences, no prefixes");
    eprintln!("  --highlight <color>      Highlight BIP39 words with color (default: bars | |)");
    eprintln!("                          Colors: black, red, green, yellow, blue, magenta, cyan, white");
    eprintln!("                          Or ANSI codes: 30-37");
    eprintln!("                          Use 'none' to disable highlighting");
    eprintln!("  --madlib                Replace BIP39 words with [POS] placeholders");
    eprintln!("  --seed <N>              Seed for deterministic random generation");
    eprintln!("  --variations <N>         Generate N variations and select the most compact (default: 1)");
    eprintln!("  --language, -l <lang>    Language for wordlist: 'english' (default), 'french', 'german'");
    eprintln!("  --k-min <N>              Minimum sentence length in POS slots including Dot (default: 3)");
    eprintln!("  --k-max <N>              Maximum sentence length in POS slots including Dot (default: 20)");
    eprintln!("  --length-mode <mode>      Sentence length selection: 'compact' or 'natural'");
    eprintln!("                          default: subject -> compact, body -> natural");
    eprintln!("                          compact: Try k from k_min to k_max, shortest first");
    eprintln!("                          natural: Sample k from grammar's length distribution");
    eprintln!("  --show-grammar           Display the grammar rules (then continue execution)");
    eprintln!("  --verbose, -v           Show detailed debugging information");
    eprintln!("  --help                  Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {} abandon ability able about above absent", program_name);
    eprintln!("  {} --random 10", program_name);
    eprintln!("  {} --random 5 --grammar subject --highlight none", program_name);
    eprintln!("  {} --random 5 --grammar body --highlight highlight", program_name);
}

fn parse_args() -> Result<(Vec<String>, Option<usize>, bool, Option<u64>, usize, HighlightMode, GenerationMode, String, bool, usize, usize, SentenceLengthMode), String> {
    let args: Vec<String> = env::args().collect();
    let program_name = args[0].clone();
    
    if args.len() < 2 {
        return Err("No words provided. Use --random <N> or provide words as arguments.".to_string());
    }
    
    let mut words = Vec::new();
    let mut random_count: Option<usize> = None;
    let mut verbose = false;
    let mut seed: Option<u64> = None;
    let mut variations = 1;
    let mut highlight_mode = HighlightMode::Bars;
    let mut generation_mode = GenerationMode::Subject;
    let mut language = "english".to_string();
    let mut show_grammar = false;
    let mut k_min = 3;
    let mut k_max = 20;
    let mut length_mode = SentenceLengthMode::Compact;
    let mut length_mode_explicit = false;
    let mut i = 1;
    
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_usage(&program_name);
                std::process::exit(0);
            }
            "--verbose" | "-v" => {
                verbose = true;
                i += 1;
            }
            "--random" => {
                if i + 1 >= args.len() {
                    return Err("--random requires a value".to_string());
                }
                random_count = Some(args[i + 1].parse()
                    .map_err(|_| format!("Invalid number for --random: {}", args[i + 1]))?);
                i += 2;
            }
            "--grammar" => {
                if i + 1 >= args.len() {
                    return Err("--grammar requires a value".to_string());
                }
                generation_mode = match args[i + 1].as_str() {
                    "subject" => GenerationMode::Subject,
                    "body" => GenerationMode::Body,
                    _ => return Err(format!("Invalid grammar: {}. Use 'subject' or 'body'", args[i + 1])),
                };
                i += 2;
            }
            "--mode" => {
                // Deprecated: support old --mode flag for backward compatibility
                if i + 1 >= args.len() {
                    return Err("--mode requires a value (use --grammar instead)".to_string());
                }
                eprintln!("Warning: --mode is deprecated, use --grammar instead");
                generation_mode = match args[i + 1].as_str() {
                    "subject" => GenerationMode::Subject,
                    "body" => GenerationMode::Body,
                    _ => return Err(format!("Invalid grammar: {}. Use 'subject' or 'body'", args[i + 1])),
                };
                i += 2;
            }
            "--highlight" => {
                if i + 1 >= args.len() {
                    return Err("--highlight requires a value".to_string());
                }
                highlight_mode = match args[i + 1].as_str() {
                    "none" => HighlightMode::None,
                    "bars" => HighlightMode::Bars,
                    color_str => {
                        match parse_color(color_str) {
                            Ok(color_code) => HighlightMode::Color(color_code),
                            Err(e) => return Err(e),
                        }
                    }
                };
                i += 2;
            }
            "--madlib" => {
                highlight_mode = HighlightMode::Madlib;
                i += 1;
            }
            "--seed" => {
                if i + 1 >= args.len() {
                    return Err("--seed requires a value".to_string());
                }
                seed = Some(args[i + 1].parse()
                    .map_err(|_| format!("Invalid number for --seed: {}", args[i + 1]))?);
                i += 2;
            }
            "--variations" => {
                if i + 1 >= args.len() {
                    return Err("--variations requires a value".to_string());
                }
                variations = args[i + 1].parse()
                    .map_err(|_| format!("Invalid number for --variations: {}", args[i + 1]))?;
                if variations == 0 {
                    return Err("--variations must be at least 1".to_string());
                }
                i += 2;
            }
            "--language" | "-l" => {
                if i + 1 >= args.len() {
                    return Err("--language requires a value".to_string());
                }
                language = args[i + 1].clone();
                i += 2;
            }
            "--show-grammar" => {
                show_grammar = true;
                i += 1;
            }
            "--k-min" => {
                if i + 1 >= args.len() {
                    return Err("--k-min requires a value".to_string());
                }
                k_min = args[i + 1].parse()
                    .map_err(|_| format!("Invalid number for --k-min: {}", args[i + 1]))?;
                if k_min < 3 {
                    return Err("--k-min must be at least 3".to_string());
                }
                i += 2;
            }
            "--k-max" => {
                if i + 1 >= args.len() {
                    return Err("--k-max requires a value".to_string());
                }
                k_max = args[i + 1].parse()
                    .map_err(|_| format!("Invalid number for --k-max: {}", args[i + 1]))?;
                if k_max < k_min {
                    return Err(format!("--k-max ({}) must be >= --k-min ({})", k_max, k_min));
                }
                if k_max > 20 {
                    return Err("--k-max cannot exceed 20 (sequences only precomputed up to k=20)".to_string());
                }
                i += 2;
            }
            "--length-mode" => {
                if i + 1 >= args.len() {
                    return Err("--length-mode requires a value".to_string());
                }
                length_mode = match args[i + 1].as_str() {
                    "compact" => SentenceLengthMode::Compact,
                    "natural" => SentenceLengthMode::Natural,
                    _ => return Err(format!("Invalid length mode: {}. Use 'compact' or 'natural'", args[i + 1])),
                };
                length_mode_explicit = true;
                i += 2;
            }
            arg if arg.starts_with("--") => {
                return Err(format!("Unknown option: {}", arg));
            }
            word => {
                words.push(word.to_string());
                i += 1;
            }
        }
    }
    
    if random_count.is_some() && !words.is_empty() {
        return Err("Cannot use --random with explicit words. Use one or the other.".to_string());
    }
    
    if random_count.is_none() && words.is_empty() && !show_grammar {
        return Err("No words provided. Use --random <N> or provide words as arguments.".to_string());
    }

    // Default length mode depends on grammar mode unless explicitly overridden:
    // - subject: compact (shortest-first)
    // - body: natural (sample from grammar length distribution)
    if !length_mode_explicit {
        length_mode = match generation_mode {
            GenerationMode::Subject => SentenceLengthMode::Compact,
            GenerationMode::Body => SentenceLengthMode::Natural,
        };
    }
    
    Ok((words, random_count, verbose, seed, variations, highlight_mode, generation_mode, language, show_grammar, k_min, k_max, length_mode))
}

/// Get the wordlist file path for a given language.
/// Returns the path to the POS-tagged wordlist file.
/// Exits with error if POS-tagged file doesn't exist.
fn get_wordlist_path(language: &str) -> Result<String, String> {
    let pos_path = format!("languages/{}/{}_bip39_POS.txt", language, language);
    
    if std::path::Path::new(&pos_path).exists() {
        return Ok(pos_path);
    }
    
    Err(format!("POS-tagged wordlist file not found for language '{}'. Expected: {}\nOnly languages with POS-tagged wordlists are supported.", 
                language, pos_path))
}

// --- CLI usage ---
fn main() {
    // Add blank lines at program start
    println!();
    println!();
    
    let (mut words, random_count, verbose, seed, variations, highlight_mode, generation_mode, language, show_grammar, k_min, k_max, length_mode) = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!();
            print_usage(&env::args().next().unwrap_or_else(|| "bip39-encode".to_string()));
            std::process::exit(1);
        }
    };
    
    // If --show-grammar flag is set, display grammar and continue
    if show_grammar {
        let grammar = match generation_mode {
            GenerationMode::Subject => Grammar::subject(),
            GenerationMode::Body => Grammar::default(),
        };
        
        match grammar {
            Ok(g) => {
                let mode_str = match generation_mode {
                    GenerationMode::Subject => "subject",
                    GenerationMode::Body => "body",
                };
                println!("Grammar: {}", mode_str);
                let grammar_output = g.format_concise();
                print!("{}", grammar_output);
                let start_symbols = if generation_mode == GenerationMode::Body {
                    vec!["S"]
                } else {
                    vec!["S", "S_N", "S_V", "S_Adj", "S_Adv", "S_Prep", "S_Det"]
                };
                let mut by_start_symbol = HashMap::new();
                for start_symbol in start_symbols {
                    let sequences_by_k = g.precompute_sequences_with_probability(start_symbol, k_max);
                    if !sequences_by_k.is_empty() {
                        by_start_symbol.insert(start_symbol.to_string(), sequences_by_k);
                    }
                }
                print_sentence_kinds_once(mode_str, k_max, &by_start_symbol, true);
                println!(); // Add blank line after grammar
            }
            Err(e) => {
                eprintln!("Error loading grammar: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Use seeded RNG if seed provided, otherwise generate random seed from thread_rng
    let seed_value = if let Some(s) = seed {
        s
    } else {
        // Generate a random seed from thread_rng for non-deterministic behavior
        rand::thread_rng().gen::<u64>()
    };
    let mut rng = StdRng::seed_from_u64(seed_value);
    
    if verbose {
        if seed.is_some() {
            eprintln!("Using seed: {}", seed_value);
        }
        let mode_str = match generation_mode {
            GenerationMode::Subject => "subject",
            GenerationMode::Body => "body",
        };
        eprintln!("Mode: {}", mode_str);
        eprintln!("Language: {}", language);
    }

    // Validate language and get wordlist path early
    match get_wordlist_path(&language) {
        Ok(path) => {
            if verbose {
                eprintln!("Using wordlist: {}", path);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // If random words requested, select them now
    if let Some(count) = random_count {
        words = match select_random_words(&mut rng, count, &language) {
            Ok(selected_words) => {
                if verbose {
                    eprintln!("Selected {} random BIP39 words: {}", count, selected_words.join(" "));
                }
                selected_words
            }
            Err(e) => {
                eprintln!("Error loading wordlist: {}", e);
                std::process::exit(1);
            }
        };
    }

    // Tag each word with POS tags
    let payload: Vec<PayloadTok> = words
        .iter()
        .map(|word| {
            let tags = tag_word(word);
            PayloadTok::new(word.clone(), &tags)
        })
        .collect();

    let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
    let payload_set_clone = payload_set.clone(); // Keep a copy for later statistics

    // Load BIP39 words for validation
    let bip39_words = match load_bip39_words(&language) {
        Ok(words) => words,
        Err(e) => {
            eprintln!("Error loading wordlist: {}", e);
            std::process::exit(1);
        }
    };
    let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();
    
    // Load cover words with explicit POS tags from cover_POS.txt
    let cover_by_pos = load_cover_words_by_pos(&bip39_set);
    
    // Extract function words from cover_POS.txt (with fallbacks for backward compatibility)
    let det_words: Vec<&str> = cover_by_pos
        .get(&Pos::Det)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["the", "a", "an", "each", "some"]);
    
    let modal_words: Vec<&str> = cover_by_pos
        .get(&Pos::Modal)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["should", "could", "would", "might", "may"]);
    
    let aux_words: Vec<&str> = cover_by_pos
        .get(&Pos::Aux)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["do", "does"]);
    
    let cop_words: Vec<&str> = cover_by_pos
        .get(&Pos::Cop)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["is", "are"]);
    
    let to_words: Vec<&str> = cover_by_pos
        .get(&Pos::To)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["to"]);

    let conj_words: Vec<&str> = cover_by_pos
        .get(&Pos::Conj)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["and", "but", "or"]);
    
    // Subject grammar can emit Prefix (e.g., "re", "fwd") so ensure we always have
    // some safe defaults even if cover_POS.txt doesn't provide them.
    let prefix_words: Vec<&str> = cover_by_pos
        .get(&Pos::Prefix)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["re", "fwd", "fw", "update"]);
    
    // Extract content words from cover_POS.txt (with fallbacks for backward compatibility)
    let adj_words: Vec<&str> = cover_by_pos
        .get(&Pos::Adj)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["clear", "simple", "bright", "quiet", "steady"]);
    
    let n_words: Vec<&str> = cover_by_pos
        .get(&Pos::N)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["user", "note", "server", "system"]);
    
    let v_words: Vec<&str> = cover_by_pos
        .get(&Pos::V)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["check", "send", "hold", "verify", "process"]);
    
    let prep_words: Vec<&str> = cover_by_pos
        .get(&Pos::Prep)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["about", "above", "along", "beneath", "throughout"]);
    
    let adv_words: Vec<&str> = cover_by_pos
        .get(&Pos::Adv)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| vec!["soon", "well", "quite", "very"]);
    
    // Validate all cover words against BIP39 wordlist
    let all_cover_words: Vec<&str> = det_words.iter()
        .chain(modal_words.iter())
        .chain(aux_words.iter())
        .chain(cop_words.iter())
        .chain(to_words.iter())
        .chain(conj_words.iter())
        .chain(prefix_words.iter())
        .chain(adj_words.iter())
        .chain(n_words.iter())
        .chain(v_words.iter())
        .chain(prep_words.iter())
        .chain(adv_words.iter())
        .copied()
        .collect();
    
    validate_cover_words(&all_cover_words, &bip39_set);
    
    if verbose {
        eprintln!("Loaded cover words from cover_POS.txt:");
        eprintln!("  Adjectives: {}", adj_words.len());
        eprintln!("  Nouns: {}", n_words.len());
        eprintln!("  Verbs: {}", v_words.len());
        eprintln!("  Prepositions: {}", prep_words.len());
        eprintln!("  Adverbs: {}", adv_words.len());
        if !prefix_words.is_empty() {
            eprintln!("  Prefixes: {}", prefix_words.len());
        }
    }
    
    let lex = Lexicon::new(payload_set, bip39_set.clone())
        .with_words(Pos::Det, &det_words)
        .with_words(Pos::Modal, &modal_words)
        .with_words(Pos::Aux, &aux_words)
        .with_words(Pos::Cop, &cop_words)
        .with_words(Pos::To, &to_words)
        .with_words(Pos::Conj, &conj_words)
        .with_words(Pos::Prefix, &prefix_words)
        .with_words(Pos::Adj, &adj_words)
        .with_words(Pos::N, &n_words)
        .with_words(Pos::V, &v_words)
        .with_words(Pos::Prep, &prep_words)
        .with_words(Pos::Adv, &adv_words);

    let input_word_count = payload.len();
    let expected_words: Vec<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();

    // Calculate input statistics by POS
    let mut input_pos_counts: HashMap<Pos, usize> = HashMap::new();
    for tok in &payload {
        for pos in &tok.allowed {
            *input_pos_counts.entry(*pos).or_insert(0) += 1;
        }
    }

    // Generate multiple variations and select the most compact one
    let mut best_text: Option<String> = None;
    let mut best_compactness = 0.0;
    let mut best_output_count = 0;
    let mut variation_stats: Vec<f64> = Vec::new();
    let mut valid_variation_texts: Vec<String> = Vec::new();

    if verbose && variations > 1 {
        eprintln!("Generating {} variations to maximize compactness...", variations);
    }

    let generation_start = Instant::now();
    for variation in 0..variations {
        // Use different seeds for each variation (increment base seed)
        let variation_seed = seed_value.wrapping_add(variation as u64);
        let mut variation_rng = StdRng::seed_from_u64(variation_seed);
        
        let variation_start = Instant::now();
        let (text, payload_set_from_gen) = generate_text(&mut variation_rng, &lex, &payload, highlight_mode, variations == 1 && verbose, generation_mode, k_min, k_max, length_mode);
        let variation_elapsed = variation_start.elapsed();
        
        // Validate that the generated text contains exactly the input BIP39 words in order
        // Skip validation in madlib mode since words are replaced with [POS] placeholders
        if highlight_mode != HighlightMode::Madlib {
            let extracted_bip39_words: Vec<String> = {
                text
                    .split_whitespace()
                    .map(normalize_token_for_bip39)
                    .filter(|w| !w.is_empty() && payload_set_from_gen.contains(w))
                    .collect()
            };
            
            if extracted_bip39_words != expected_words {
                if verbose || variations == 1 {
                    eprintln!("Variation {}: ERROR - Generated BIP39 words do not match input words!", variation + 1);
                    eprintln!("  Expected: {:?}", expected_words);
                    eprintln!("  Got:      {:?}", extracted_bip39_words);
                    eprintln!("  Text:     {}", text.chars().take(500).collect::<String>());
                    eprintln!("  Payload set size: {}", payload_set_from_gen.len());
                    eprintln!("  Sample payload words: {:?}", payload_set_from_gen.iter().take(5).collect::<Vec<_>>());
                    // Debug: show what words are being extracted
                    let all_normalized: Vec<String> = text
                        .split_whitespace()
                        .map(normalize_token_for_bip39)
                        .filter(|w| !w.is_empty())
                        .collect();
                    eprintln!("  All normalized words (first 20): {:?}", all_normalized.iter().take(20).collect::<Vec<_>>());
                    let matching_words: Vec<String> = all_normalized
                        .iter()
                        .filter(|w| payload_set_from_gen.contains(*w))
                        .cloned()
                        .collect();
                    eprintln!("  Matching words: {:?}", matching_words);
                }
                continue;
            }
        }

        // Save this valid variation for optional printing
        valid_variation_texts.push(text.clone());
        
        // Calculate compactness score for this variation (based on character counts)
        let words: Vec<&str> = text.split_whitespace().collect();
        let output_word_count = words.len();
        let mut bip39_chars = 0;
        let mut non_bip39_chars = 0;
        
        // In madlib mode, count [POS] placeholders as BIP39 chars
        if highlight_mode == HighlightMode::Madlib {
            for word in &words {
                if word.starts_with('[') && word.contains(']') {
                    // Count [POS] placeholder as BIP39 (approximate length)
                    bip39_chars += 5; // Approximate: "[N]" = 3 chars, but use 5 for weighting
                } else {
                    let normalized = normalize_token_for_bip39(word);
                    non_bip39_chars += normalized.chars().count();
                }
            }
        } else {
            for word in &words {
                let normalized = normalize_token_for_bip39(word);
                if !normalized.is_empty() && payload_set_from_gen.contains(&normalized) {
                    // Count characters in BIP39 word (use normalized length)
                    bip39_chars += normalized.chars().count();
                } else {
                    // Count characters in non-BIP39 word (use normalized length, excluding punctuation)
                    non_bip39_chars += normalized.chars().count();
                }
            }
        }
        
        let total_chars = bip39_chars + non_bip39_chars;
        let compactness = if total_chars > 0 {
            bip39_chars as f64 / total_chars as f64
        } else {
            0.0
        };

        variation_stats.push(compactness);

        if verbose && variations > 1 {
            eprintln!("Variation {}: compactness {:.3} ({} BIP39 chars / {} total chars) [{:.3}s]", 
                      variation + 1, compactness, bip39_chars, total_chars, variation_elapsed.as_secs_f64());
        }

        // Keep track of the best (most compact) variation
        if compactness > best_compactness {
            best_compactness = compactness;
            best_text = Some(text);
            best_output_count = output_word_count;
        }
    }
    let total_generation_time = generation_start.elapsed();

    // Output timing information (only in verbose mode)
    if verbose {
        eprintln!("Generation time: {:.3}s ({} variation(s))", total_generation_time.as_secs_f64(), variations);
        if variations > 1 {
            eprintln!("Average time per variation: {:.3}s", total_generation_time.as_secs_f64() / variations as f64);
        }
    }

    // Output variations (if requested) or the best variation (default)
    let text = match best_text {
        Some(t) => t,
        None => {
            eprintln!("Error: Failed to generate any valid variations after {} attempts.", variations);
            eprintln!("This may happen if:");
            eprintln!("  - The grammar cannot accommodate all input words");
            eprintln!("  - There are POS tagging issues with some words");
            eprintln!("  - The word extraction logic is failing");
            if highlight_mode == HighlightMode::Madlib {
                eprintln!("  Note: Validation is skipped in madlib mode, so this error should not occur.");
            }
            eprintln!("\nTry running with --verbose to see detailed error messages.");
            std::process::exit(1);
        }
    };
    
    // Word wrap the output to 80 characters
    if variations > 1 {
        for (i, vtext) in valid_variation_texts.iter().enumerate() {
            if i > 0 {
                println!();
                println!();
            }
            println!("{}", word_wrap(vtext, 80));
        }
    } else {
        println!("{}", word_wrap(&text, 80));
    }

    // Calculate detailed statistics from the best text
    let sentences: Vec<&str> = text.split('.').filter(|s| !s.trim().is_empty()).collect();
    let sentence_count = sentences.len();
    let avg_words_per_sentence = if sentence_count > 0 {
        best_output_count as f64 / sentence_count as f64
    } else {
        0.0
    };

    // Count payload vs cover words and characters
    let payload_words_in_output: HashSet<String> = payload_set_clone.iter().cloned().collect();
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut payload_word_count = 0;
    let mut cover_word_count = 0;
    let mut bip39_char_count = 0;
    let mut non_bip39_char_count = 0;
    
    for word in &words {
        let normalized = normalize_token_for_bip39(word);
        if !normalized.is_empty() && payload_words_in_output.contains(&normalized) {
            payload_word_count += 1;
            bip39_char_count += normalized.chars().count();
        } else {
            cover_word_count += 1;
            non_bip39_char_count += normalized.chars().count();
        }
    }
    
    let total_output_chars = bip39_char_count + non_bip39_char_count;

    // Print comprehensive statistics (only when generating multiple variations)
    if variations > 1 {
        eprintln!("\n=== Statistics ===");
        eprintln!("Input:");
        eprintln!("  Total words: {}", input_word_count);
        if !input_pos_counts.is_empty() {
            eprintln!("  POS breakdown:");
            let mut pos_vec: Vec<_> = input_pos_counts.iter().collect();
            pos_vec.sort_by_key(|(pos, _)| {
                match pos {
                    Pos::N => 1,
                    Pos::V => 2,
                    Pos::Adj => 3,
                    Pos::Adv => 4,
                    Pos::Prep => 5,
                    Pos::Det => 6,
                    _ => 7,
                }
            });
            for (pos, count) in pos_vec {
                let pos_name = match pos {
                    Pos::Det => "Determiners",
                    Pos::Adj => "Adjectives",
                    Pos::N => "Nouns",
                    Pos::V => "Verbs",
                    Pos::Modal => "Modals",
                    Pos::Aux => "Auxiliaries",
                    Pos::Cop => "Copulas",
                    Pos::To => "To",
                    Pos::Prep => "Prepositions",
                    Pos::Adv => "Adverbs",
                    Pos::Conj => "Conjunctions",
                    Pos::Dot => "Punctuation",
                    Pos::Prefix => "Prefixes",
                };
                eprintln!("    {}: {}", pos_name, count);
            }
        }
        
        eprintln!("Output:");
        eprintln!("  Total words: {}", best_output_count);
        eprintln!("  Payload words: {} ({:.1}%)", payload_word_count, 
                  (payload_word_count as f64 / best_output_count as f64) * 100.0);
        eprintln!("  Cover words: {} ({:.1}%)", cover_word_count,
                  (cover_word_count as f64 / best_output_count as f64) * 100.0);
        eprintln!("  Sentences: {}", sentence_count);
        eprintln!("  Avg words per sentence: {:.1}", avg_words_per_sentence);
        
        eprintln!("Compactness:");
        eprintln!("  Score: {:.3} ({} BIP39 chars / {} total chars)", 
                  best_compactness, bip39_char_count, total_output_chars);
        eprintln!("  Efficiency: {:.1}% BIP39 characters", (best_compactness * 100.0));
        
        if !variation_stats.is_empty() {
        let min_compactness = variation_stats.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_compactness = variation_stats.iter().fold(0.0_f64, |a, &b| a.max(b));
        let avg_compactness = variation_stats.iter().sum::<f64>() / variation_stats.len() as f64;
        eprintln!("Variations:");
        eprintln!("  Tested: {}", variations);
        eprintln!("  Min compactness: {:.3}", min_compactness);
        eprintln!("  Max compactness: {:.3}", max_compactness);
        eprintln!("  Avg compactness: {:.3}", avg_compactness);
        eprintln!("  Improvement: {:.1}% better than average", 
                  ((best_compactness - avg_compactness) / avg_compactness * 100.0).max(0.0));
        }
    }

    // Decoding: split on whitespace/punct, keep only tokens that are in the BIP39 set.
    // The payload_set contains all the BIP39 words, so filtering is straightforward.
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;
    use rand::SeedableRng;
    use bip39_encode::GrammarChecker;

    #[test]
    fn test_max_subsequence_embedding() {
        // Test basic embedding: slots [Det, N, V, Dot], payload [N, V]
        let slots = vec![Pos::Det, Pos::N, Pos::V, Pos::Dot];
        let payload = vec![
            PayloadTok::new("word1", &[Pos::N]),
            PayloadTok::new("word2", &[Pos::V]),
        ];
        
        let placement = max_subsequence_embedding(&slots, &payload, 0, 2);
        assert!(placement.is_some());
        let placement = placement.unwrap();
        assert_eq!(placement.len(), 2);
        assert_eq!(placement.get(&1), Some(&0)); // N slot -> payload[0]
        assert_eq!(placement.get(&2), Some(&1)); // V slot -> payload[1]
        
        // Test with gaps: slots [Det, N, Adj, V, Dot], payload [N, V]
        let slots = vec![Pos::Det, Pos::N, Pos::Adj, Pos::V, Pos::Dot];
        let placement = max_subsequence_embedding(&slots, &payload, 0, 2);
        assert!(placement.is_some());
        let placement = placement.unwrap();
        assert_eq!(placement.len(), 2);
        assert_eq!(placement.get(&1), Some(&0)); // N slot -> payload[0]
        assert_eq!(placement.get(&3), Some(&1)); // V slot -> payload[1]
        
        // Test impossible: slots [Det, N, Dot], payload [N, V] - can't fit V
        let impossible_slots = vec![Pos::Det, Pos::N, Pos::Dot];
        let placement = max_subsequence_embedding(&impossible_slots, &payload, 0, 2);
        assert!(placement.is_none());
        
        // Test j=0
        let placement = max_subsequence_embedding(&slots, &payload, 0, 0);
        assert!(placement.is_some());
        assert!(placement.unwrap().is_empty());
    }

    #[test]
    fn test_plan_sentence_max_j() {
        // Load cache
        let cache = SequenceCache::load(GenerationMode::Body, 20, false).expect("Failed to load cache");
        let mut rng = StdRng::seed_from_u64(42);
        
        // Create payload with N, V, N
        let payload = vec![
            PayloadTok::new("noun1", &[Pos::N]),
            PayloadTok::new("verb1", &[Pos::V]),
            PayloadTok::new("noun2", &[Pos::N]),
        ];
        
        // Plan for k=5 (should fit at least 2 words, maybe 3)
        let result = plan_sentence(&mut rng, &cache, "S", 5, &payload, 0, false);
        assert!(result.is_some());
        let (slots, forced_placements, j) = result.unwrap();
        assert!(j >= 1, "Should embed at least 1 word");
        assert_eq!(forced_placements.len(), j);
        
        // Verify placements are valid
        for (&slot_idx, &payload_idx) in &forced_placements {
            assert!(slot_idx < slots.len());
            assert!(payload_idx < payload.len());
            assert!(payload_fits(&payload[payload_idx], slots[slot_idx]));
        }
        
        // Verify order preservation: if slot_i < slot_j, then payload_i < payload_j
        let mut sorted_slots: Vec<(usize, usize)> = forced_placements.iter().map(|(&s, &p)| (s, p)).collect();
        sorted_slots.sort_by_key(|(s, _)| *s);
        for i in 1..sorted_slots.len() {
            assert!(sorted_slots[i-1].1 < sorted_slots[i].1, "Placements must preserve payload order");
        }
    }

    #[test]
    fn test_ordered_payload_extraction() {
        // Smoke test: generate text and verify payload words appear in order
        let mut rng = StdRng::seed_from_u64(42);
        let words = vec!["abandon".to_string(), "ability".to_string(), "able".to_string()];
        
        let payload: Vec<PayloadTok> = words
            .iter()
            .map(|word| {
                let tags = tag_word(word);
                PayloadTok::new(word.clone(), &tags)
            })
            .collect();

        let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
        let bip39_words = load_bip39_words("english").unwrap();
        let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();

        let lex = setup_test_lexicon(payload_set.clone(), bip39_set);
        let (text, _) = generate_text(&mut rng, &lex, &payload, HighlightMode::None, false, GenerationMode::Body, 3, 20, SentenceLengthMode::Compact);

        // Extract BIP39 words in order
        let extracted: Vec<String> = text
            .split_whitespace()
            .map(normalize_token_for_bip39)
            .filter(|w| !w.is_empty() && payload_set.contains(w))
            .collect();
        
        assert_eq!(extracted, words.iter().map(|w| w.to_lowercase()).collect::<Vec<_>>(), 
                   "Extracted words should match input words in order");
    }

    #[derive(Default)]
    struct ZeroRng;

    impl RngCore for ZeroRng {
        fn next_u32(&mut self) -> u32 {
            0
        }
        fn next_u64(&mut self) -> u64 {
            0
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            dest.fill(0);
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    #[test]
    fn test_article_selection_uses_forced_next_word_and_normalizes() {
        // Ensure a/an selection uses the *actual* next token in forced-placement mode
        // (body/subject planner) and that highlight/bars don't break vowel detection.
        assert!(starts_with_vowel_sound("ivory"));
        assert!(starts_with_vowel_sound("|ivory|"));
        assert!(starts_with_vowel_sound("\x1b[32mivory\x1b[0m"));

        // Slots: N Det Adj N Dot
        let slots = vec![Pos::N, Pos::Det, Pos::Adj, Pos::N, Pos::Dot];

        // Payload: force "ivory" into the Adj slot (index 2). It starts with a vowel, so determiner should be "an".
        let payload = vec![PayloadTok::new("ivory", &[Pos::Adj])];
        let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
        let bip39_set: HashSet<String> = HashSet::new();
        let lex = Lexicon::new(payload_set, bip39_set)
            .with_words(Pos::Det, &["the", "a", "an", "each", "some"])
            .with_words(Pos::Adj, &["clear", "simple"])
            .with_words(Pos::N, &["user", "note"]);

        let mut forced: HashMap<usize, usize> = HashMap::new();
        forced.insert(2, 0);

        let mut rng = ZeroRng::default();
        let mut payload_i = 0usize;
        let out = fill_slots(
            &mut rng,
            &lex,
            &slots,
            &payload,
            &mut payload_i,
            &[],
            None,
            Some(&forced),
        );

        assert_eq!(out[1], "an", "Expected 'an' before forced 'ivory'");
        assert_eq!(out[2], "ivory");
    }

    #[test]
    fn test_modal_requires_bare_verb_cover_choice() {
        // Ensure our lightweight constraint "Modal -> bare V only" is applied when choosing cover verbs.
        let payload: Vec<PayloadTok> = vec![];
        let payload_set: HashSet<String> = HashSet::new();
        let bip39_set: HashSet<String> = HashSet::new();

        let lex = Lexicon::new(payload_set, bip39_set)
            .with_words(Pos::Modal, &["can"])
            .with_words(Pos::V, &["walked", "walking", "walk"]);

        let slots = vec![Pos::Modal, Pos::V, Pos::Dot];
        let mut rng = ZeroRng::default();
        let mut payload_i = 0usize;
        let out = fill_slots(&mut rng, &lex, &slots, &payload, &mut payload_i, &[], None, None);

        assert_eq!(out[0], "can");
        assert_eq!(out[1].trim_end_matches('.'), "walk", "Expected bare verb after modal");
    }

    #[test]
    fn test_v_np_prefers_transitive_cover_verb() {
        // Ensure "V -> NP only if transitive" is applied as a bias for cover verbs.
        // Slots: Det N V Det N Dot  (so V is followed by an NP start)
        let payload: Vec<PayloadTok> = vec![];
        let payload_set: HashSet<String> = HashSet::new();
        let bip39_set: HashSet<String> = HashSet::new();

        let lex = Lexicon::new(payload_set, bip39_set)
            .with_words(Pos::Det, &["the"])
            .with_words(Pos::N, &["user", "note"])
            .with_words(Pos::V, &["sleep", "send"]);

        let slots = vec![Pos::Det, Pos::N, Pos::V, Pos::Det, Pos::N, Pos::Dot];
        let mut rng = ZeroRng::default();
        let mut payload_i = 0usize;
        let out = fill_slots(&mut rng, &lex, &slots, &payload, &mut payload_i, &[], None, None);

        assert_eq!(out[2], "send", "Expected transitive verb before NP object");
    }

    /// Fixed seed for reproducible tests
    const TEST_SEED: u64 = 42;

    /// Helper function to set up a test lexicon with minimal cover words
    fn setup_test_lexicon(payload_set: HashSet<String>, bip39_set: HashSet<String>) -> Lexicon {
        let det_words = ["the", "a", "an", "each", "some"];
        let modal_words = ["should", "could", "would", "might", "may"];
        let aux_words = ["do", "does"];
        let cop_words = ["is", "are"];
        let to_words = ["to"];
        let conj_words = ["and", "but", "or"];
        let prefix_words = ["re", "fwd", "fw", "update"];
        let adj_words = ["bright", "clear", "simple", "secure", "quiet", "steady"];
        let n_words = ["wallet", "user", "server", "system", "note"];
        let v_words = ["check", "send", "hold", "verify", "process"];
        let prep_words = ["about", "above", "along", "beneath", "throughout"];
        let adv_words = ["soon", "well", "quite", "very"];

        Lexicon::new(payload_set, bip39_set)
            .with_words(Pos::Det, &det_words)
            .with_words(Pos::Modal, &modal_words)
            .with_words(Pos::Aux, &aux_words)
            .with_words(Pos::Cop, &cop_words)
            .with_words(Pos::To, &to_words)
            .with_words(Pos::Conj, &conj_words)
            .with_words(Pos::Prefix, &prefix_words)
            .with_words(Pos::Adj, &adj_words)
            .with_words(Pos::N, &n_words)
            .with_words(Pos::V, &v_words)
            .with_words(Pos::Prep, &prep_words)
            .with_words(Pos::Adv, &adv_words)
    }

    /// Extract individual sentences from generated text
    fn extract_sentences(text: &str) -> Vec<String> {
        text.split('.')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                // Remove BIP39 markers (|word|)
                s.replace('|', "")
            })
            .collect()
    }

    #[test]
    fn test_generated_sentences_grammar() {
        // Skip if grammar checker files are not available
        let grammar_checker = match GrammarChecker::from_language(bip39_encode::Language::English) {
            Ok(checker) => checker,
            Err(_) => {
                eprintln!("Skipping grammar test: nlprule binary files not found");
                return;
            }
        };

        // Generate sentences with random BIP39 words (using fixed seed for reproducibility)
        let mut rng = StdRng::seed_from_u64(TEST_SEED);
        let words = select_random_words(&mut rng, 10, "english").unwrap();
        
        let payload: Vec<PayloadTok> = words
            .iter()
            .map(|word| {
                let tags = tag_word(word);
                PayloadTok::new(word.clone(), &tags)
            })
            .collect();

        let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
        let bip39_words = load_bip39_words("english").unwrap();
        let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();

        let lex = setup_test_lexicon(payload_set.clone(), bip39_set);
        let (text, _) = generate_text(&mut rng, &lex, &payload, HighlightMode::None, false, GenerationMode::Subject, 3, 20, SentenceLengthMode::Compact);

        // Extract individual sentences
        let sentences = extract_sentences(&text);

        assert!(!sentences.is_empty(), "Should generate at least one sentence");

        // Check grammar for each sentence
        let mut total_suggestions = 0;
        let mut sentences_with_errors = 0;

        for sentence in &sentences {
            let suggestions = grammar_checker.check(sentence);
            total_suggestions += suggestions.len();
            
            if !suggestions.is_empty() {
                sentences_with_errors += 1;
                eprintln!("Sentence with grammar issues: \"{}\"", sentence);
                for suggestion in &suggestions {
                    eprintln!("  Suggestion: {:?}", suggestion);
                }
            }
        }

        // Report results
        eprintln!("\nGrammar check results:");
        eprintln!("  Total sentences: {}", sentences.len());
        eprintln!("  Sentences with errors: {}", sentences_with_errors);
        eprintln!("  Total suggestions: {}", total_suggestions);
        eprintln!("  Average suggestions per sentence: {:.2}", 
                  total_suggestions as f64 / sentences.len() as f64);

        // Allow some grammar errors (generated sentences may not be perfect)
        // But we want most sentences to be reasonably correct
        let error_rate = sentences_with_errors as f64 / sentences.len() as f64;
        assert!(
            error_rate < 0.5,
            "More than 50% of sentences have grammar errors (error rate: {:.2}%)",
            error_rate * 100.0
        );
    }

    #[test]
    fn test_grammar_with_different_payload_sizes() {
        // Skip if grammar checker files are not available
        let grammar_checker = match GrammarChecker::from_language(bip39_encode::Language::English) {
            Ok(checker) => checker,
            Err(_) => {
                eprintln!("Skipping grammar test: nlprule binary files not found");
                return;
            }
        };

        // Test with different payload sizes (using same seed for reproducibility)
        for word_count in [5, 8, 12] {
            let mut rng = StdRng::seed_from_u64(TEST_SEED);
            let words = select_random_words(&mut rng, word_count, "english").unwrap();
            
            let payload: Vec<PayloadTok> = words
                .iter()
                .map(|word| {
                    let tags = tag_word(word);
                    PayloadTok::new(word.clone(), &tags)
                })
                .collect();

            let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
            let bip39_words = load_bip39_words("english").unwrap();
            let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();

            let lex = setup_test_lexicon(payload_set, bip39_set);
            let (text, _) = generate_text(&mut rng, &lex, &payload, HighlightMode::None, false, GenerationMode::Subject, 3, 20, SentenceLengthMode::Compact);

            let sentences = extract_sentences(&text);
            
            // Check that at least some sentences are grammatically reasonable
            let mut correct_sentences = 0;
            for sentence in &sentences {
                let suggestions = grammar_checker.check(sentence);
                if suggestions.is_empty() {
                    correct_sentences += 1;
                }
            }

            eprintln!("Payload size {}: {}/{} sentences grammatically correct", 
                     word_count, correct_sentences, sentences.len());
            
            // At least one sentence should be correct
            assert!(
                correct_sentences > 0 || sentences.is_empty(),
                "No grammatically correct sentences generated for payload size {}",
                word_count
            );
        }
    }

    #[test]
    fn test_sentence_structure() {
        // Test that generated sentences have reasonable structure (using fixed seed for reproducibility)
        let mut rng = StdRng::seed_from_u64(TEST_SEED);
        let words = select_random_words(&mut rng, 5, "english").unwrap();
        
        let payload: Vec<PayloadTok> = words
            .iter()
            .map(|word| {
                let tags = tag_word(word);
                PayloadTok::new(word.clone(), &tags)
            })
            .collect();

        let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
        let bip39_words = load_bip39_words("english").unwrap();
        let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();

        let lex = setup_test_lexicon(payload_set, bip39_set);
        let (text, _) = generate_text(&mut rng, &lex, &payload, HighlightMode::None, false, GenerationMode::Subject, 3, 20, SentenceLengthMode::Compact);

        let sentences = extract_sentences(&text);
        
        // Each sentence should have at least a few words
        for sentence in &sentences {
            let word_count = sentence.split_whitespace().count();
            assert!(
                word_count >= 3,
                "Sentence too short ({} words): \"{}\"",
                word_count,
                sentence
            );
        }

        // Should have generated at least one sentence
        assert!(!sentences.is_empty(), "Should generate at least one sentence");
    }

    #[test]
    fn test_compute_k_candidates_compact_mode() {
        let cache = SequenceCache::load(GenerationMode::Body, 20, false).expect("Failed to load cache");
        let mut rng = StdRng::seed_from_u64(42);
        
        // Compact mode should return k_min..=k_max in order
        let candidates = compute_k_candidates(
            &mut rng,
            &cache,
            "S",
            3,
            10,
            SentenceLengthMode::Compact,
            false,
        );
        
        assert_eq!(candidates, vec![3, 4, 5, 6, 7, 8, 9, 10]);
        
        // With require_prefix, should start at k_min+1
        let candidates_prefix = compute_k_candidates(
            &mut rng,
            &cache,
            "S",
            3,
            10,
            SentenceLengthMode::Compact,
            true,
        );
        
        assert_eq!(candidates_prefix, vec![4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn test_compute_k_candidates_natural_mode() {
        let cache = SequenceCache::load(GenerationMode::Body, 20, false).expect("Failed to load cache");
        let mut rng = StdRng::seed_from_u64(42);
        
        // Natural mode should ignore k_min and sample from all available k values
        let candidates = compute_k_candidates(
            &mut rng,
            &cache,
            "S",
            3,  // k_min is ignored in natural mode
            10,
            SentenceLengthMode::Natural,
            false,
        );
        
        // Verify all candidates are in valid range (k_min is ignored, so can be < k_min)
        for &k in &candidates {
            assert!(k >= 1 && k <= 10, "k={} should be in range [1, 10] (k_min ignored)", k);
        }
        
        // Verify no duplicates
        let mut seen = std::collections::HashSet::new();
        for &k in &candidates {
            assert!(!seen.contains(&k), "Duplicate k={} in candidates", k);
            seen.insert(k);
        }
        
        // Verify sampled k is first
        assert!(!candidates.is_empty(), "Should have at least one candidate");
        let sampled_k = candidates[0];
        assert!(sampled_k >= 1 && sampled_k <= 10, "Sampled k should be in valid range");
    }

    #[test]
    fn test_compute_k_candidates_natural_mode_prefix() {
        let cache = SequenceCache::load(GenerationMode::Subject, 20, false).expect("Failed to load cache");
        let mut rng = StdRng::seed_from_u64(42);
        
        // Natural mode with require_prefix should start at k=2 (k_min ignored)
        let candidates = compute_k_candidates(
            &mut rng,
            &cache,
            "S",
            3,  // k_min is ignored in natural mode
            10,
            SentenceLengthMode::Natural,
            true,
        );
        
        // Verify all candidates are >= 2 (need at least Prefix + one more terminal)
        for &k in &candidates {
            assert!(k >= 2 && k <= 10, "k={} should be in range [2, 10] when require_prefix=true (k_min ignored)", k);
        }
        
        // Verify no duplicates
        let mut seen = std::collections::HashSet::new();
        for &k in &candidates {
            assert!(!seen.contains(&k), "Duplicate k={} in candidates", k);
            seen.insert(k);
        }
        
        // Verify sampled k is first
        assert!(!candidates.is_empty(), "Should have at least one candidate");
        let sampled_k = candidates[0];
        assert!(sampled_k >= 2 && sampled_k <= 10, "Sampled k should be in valid range");
    }

    #[test]
    fn test_compute_k_candidates_natural_mode_weight_ordering() {
        let cache = SequenceCache::load(GenerationMode::Body, 20, false).expect("Failed to load cache");
        let mut rng = StdRng::seed_from_u64(42);
        
        // Test that after the sampled k, remaining k's are in descending weight order
        let candidates = compute_k_candidates(
            &mut rng,
            &cache,
            "S",
            3,
            8,
            SentenceLengthMode::Natural,
            false,
        );
        
        // Compute weights for verification
        let mut k_weights: Vec<(usize, f64)> = Vec::new();
        for k in 3..=8 {
            if let Some(sequences) = cache.get("S", k) {
                let weight: f64 = sequences.iter()
                    .map(|seq_prob| seq_prob.probability)
                    .sum();
                if weight > 0.0 {
                    k_weights.push((k, weight));
                }
            }
        }
        
        // Find the sampled k (first in candidates)
        let sampled_k = candidates[0];
        
        // Extract remaining k's (excluding sampled_k)
        let remaining: Vec<usize> = candidates.iter()
            .skip(1)
            .filter(|&&k| k != sampled_k)
            .cloned()
            .collect();
        
        // Verify remaining k's are in descending weight order
        let mut prev_weight = f64::INFINITY;
        for &k in &remaining {
            if let Some((_, weight)) = k_weights.iter().find(|(k_val, _)| *k_val == k) {
                assert!(*weight <= prev_weight, 
                    "k={} has weight {} which should be <= previous weight {}", 
                    k, weight, prev_weight);
                prev_weight = *weight;
            }
        }
    }
}

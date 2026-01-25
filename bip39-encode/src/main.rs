// Proof-of-concept: tiny CFG sentence generator that embeds a POS-tagged payload
// (e.g., BIP39 words) in-order, inserting only SFW cover words.
//
// Cargo.toml:
// [dependencies]
// rand = "0.8"

use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand::rngs::StdRng;
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HighlightMode {
    None,
    Bars,
    Highlight,
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
    Dot,
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

}

#[derive(Clone, Debug)]
enum Sym {
    NT(&'static str),
    T(Pos),
}

fn payload_fits(tok: &PayloadTok, slot: Pos) -> bool {
    // Since BIP39 words now have only one POS tag, strict matching only
    tok.allowed.contains(&slot)
}

/// Expand a tiny CFG into a stream of POS slots.
/// We bias expansion to be longer until we reach min_len slots (excluding Dot).
/// If start_pos is Some, the sentence will start with that POS instead of the normal grammar.
fn expand_cfg<R: Rng>(rng: &mut R, sym: Sym, out: &mut Vec<Pos>, min_len: usize, start_pos: Option<Pos>) {
    match sym {
        Sym::T(p) => out.push(p),
        Sym::NT(nt) => {
            let want_long = out.iter().filter(|&&p| p != Pos::Dot).count() < min_len;

            let prod: Vec<Sym> = match nt {
                "S" => {
                    // If we need to start with a specific POS, handle it specially
                    if let Some(pos) = start_pos {
                        match pos {
                            Pos::N => {
                                // Always start directly with noun when start_pos is N to guarantee payload word placement
                                // This ensures the payload word is placed in the first slot, avoiding retries
                                vec![Sym::T(Pos::N), Sym::NT("VP"), Sym::T(Pos::Dot)]
                            }
                            Pos::V => {
                                // Start with verb: V NP Dot (imperative)
                                vec![Sym::T(Pos::V), Sym::NT("NP"), Sym::T(Pos::Dot)]
                            }
                            Pos::Adj => {
                                // Start with adjective: Adj N VP Dot
                                vec![Sym::T(Pos::Adj), Sym::T(Pos::N), Sym::NT("VP"), Sym::T(Pos::Dot)]
                            }
                            Pos::Modal | Pos::Aux | Pos::Cop | Pos::To => {
                                // These are cover-only helpers; don't force the sentence to start with them.
                                vec![Sym::NT("NP"), Sym::NT("VP"), Sym::T(Pos::Dot)]
                            }
                            Pos::Adv => {
                                // Start with adverb: Adv V NP Dot
                                vec![Sym::T(Pos::Adv), Sym::T(Pos::V), Sym::NT("NP"), Sym::T(Pos::Dot)]
                            }
                            Pos::Prep => {
                                // Start with preposition: Prep NP VP Dot
                                vec![Sym::T(Pos::Prep), Sym::NT("NP"), Sym::NT("VP"), Sym::T(Pos::Dot)]
                            }
                            Pos::Det => {
                                // Start with determiner: Det NP VP Dot (NP will expand to Adj? N)
                                vec![Sym::T(Pos::Det), Sym::NT("NP"), Sym::NT("VP"), Sym::T(Pos::Dot)]
                            }
                            Pos::Dot => {
                                // Shouldn't happen, but fallback to normal
                                vec![Sym::NT("NP"), Sym::NT("VP"), Sym::T(Pos::Dot)]
                            }
                        }
                    } else {
                        // Normal case: NP VP Dot
                        vec![Sym::NT("NP"), Sym::NT("VP"), Sym::T(Pos::Dot)]
                    }
                }
                "NP" => {
                    // NP -> Det Adj? N (Adj? = 0 or 1, max 1 adjective for naturalness)
                    let has_adj = rng.gen_bool(0.5); // 50% chance of having one adjective

                    let mut p = vec![Sym::T(Pos::Det)];
                    if has_adj {
                        p.push(Sym::T(Pos::Adj));
                    }
                    p.push(Sym::T(Pos::N));
                    p
                }
                "VP" => {
                    // Simplified VP patterns for naturalness:
                    //  - Modal V NP        (e.g., "could check the wallet")
                    //  - Cop Adj            (e.g., "is secure")
                    //  - Cop Adj PP         (e.g., "is secure in the system")
                    //  - V NP               (imperative/intransitive, when natural)
                    // Removed awkward patterns:
                    //  - Modal V (no object) - produces "should everything"
                    //  - Aux V NP           - produces "does legitimate" which is awkward
                    if want_long {
                        // Long mode: Modal V NP (40%), Modal V NP PP (20%), Cop Adj (20%), Cop Adj PP (20%)
                        match rng.gen_range(0..10) {
                            0..=3 => vec![Sym::T(Pos::Modal), Sym::T(Pos::V), Sym::NT("NP")],
                            4..=5 => vec![Sym::T(Pos::Modal), Sym::T(Pos::V), Sym::NT("NP"), Sym::NT("PP")],
                            6..=7 => vec![Sym::T(Pos::Cop), Sym::T(Pos::Adj)],
                            _ => vec![Sym::T(Pos::Cop), Sym::T(Pos::Adj), Sym::NT("PP")],
                        }
                    } else {
                        // Short mode: Modal V NP (60%), Cop Adj (30%), V NP (10%)
                        match rng.gen_range(0..10) {
                            0..=5 => vec![Sym::T(Pos::Modal), Sym::T(Pos::V), Sym::NT("NP")],
                            6..=8 => vec![Sym::T(Pos::Cop), Sym::T(Pos::Adj)],
                            _ => vec![Sym::T(Pos::V), Sym::NT("NP")],
                        }
                    }
                }
                "PP" => vec![Sym::T(Pos::Prep), Sym::NT("NP")],
                _ => panic!("unknown nonterminal: {nt}"),
            };

            for s in prod {
                expand_cfg(rng, s, out, min_len, None);
            }
        }
    }
}

/// Check if a word starts with a vowel sound (needed for a/an selection).
fn starts_with_vowel_sound(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let first_char = word.chars().next().unwrap().to_lowercase().next().unwrap();
    matches!(first_char, 'a' | 'e' | 'i' | 'o' | 'u')
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
fn fill_slots<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    slots: &[Pos],
    payload: &[PayloadTok],
    payload_i: &mut usize,
    prev_words: &[&str],
    expected_first_pos: Option<Pos>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    const REPETITION_WINDOW: usize = 3; // Check last 3 words to avoid repetition
    // Cache for words picked early (for a/an selection) to reuse later
    let mut word_cache: HashMap<usize, String> = HashMap::new();
    // Noun number chosen per NP, keyed by the noun slot index within this sentence.
    let mut noun_number: HashMap<usize, Number> = HashMap::new();
    // Only used for agreement when we introduce Aux/Cop later; keep it stable now.
    let mut subject_number: Option<Number> = None;

    for (i, &slot) in slots.iter().enumerate() {
        // If this is the first non-Dot slot and we have an expected_first_pos, ensure we place the payload word
        // Find the first non-Dot slot index
        let first_non_dot_idx = slots.iter().position(|&s| s != Pos::Dot).unwrap_or(0);
        let is_first_slot = i == first_non_dot_idx;
        let should_force_placement = is_first_slot && 
                                      expected_first_pos.is_some() && 
                                      expected_first_pos.unwrap() == slot &&
                                      *payload_i < payload.len() &&
                                      payload_fits(&payload[*payload_i], slot);
        
        match slot {
            Pos::Dot => {
                if let Some(last) = out.last_mut() {
                    last.push('.');
                } else {
                    out.push(".".to_string());
                }
            }
            Pos::Det => {
                // Allow embedding payload determiners (e.g., "this", "that") without mutation.
                // Force placement if this is the expected first slot
                if should_force_placement || (*payload_i < payload.len() && payload_fits(&payload[*payload_i], Pos::Det)) {
                    out.push(payload[*payload_i].word.clone());
                    *payload_i += 1;
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
                let next_word_str = if *payload_i < payload.len() {
                    // Check if next slot would fit the next payload word
                    if let Some(next_slot) = slots.get(i + 1) {
                        if payload_fits(&payload[*payload_i], *next_slot) {
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
                                if starts_with_vowel_sound(next) {
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
                // Force placement if this is the expected first slot, otherwise check normally
                if should_force_placement || (*payload_i < payload.len() && payload_fits(&payload[*payload_i], slot)) {
                    out.push(payload[*payload_i].word.clone());
                    *payload_i += 1;
                } else {
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
                        } else if slot == Pos::To {
                            "to".to_string()
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

    out
}

/// Generate sentences until all payload tokens are embedded.
/// Returns (formatted_text, payload_set) where formatted_text has BIP39 words highlighted according to highlight_mode.
fn generate_text<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    payload: &[PayloadTok],
    highlight_mode: HighlightMode,
    verbose: bool,
) -> (String, HashSet<String>) {
    let mut words: Vec<String> = Vec::new();
    let mut payload_i: usize = 0;

    // Build payload set for highlighting
    let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();

    // Keep generating sentences until all payload tokens are embedded
    while payload_i < payload.len() {
        // Make each sentence size adapt to remaining needs.
        let remaining_payload = payload.len().saturating_sub(payload_i);
        // Adapt sentence length based on remaining payload
        let sentence_min = if remaining_payload > 10 {
            18
        } else if remaining_payload > 5 {
            14
        } else {
            5
        };

        // Prioritize input words for ALL sentences by starting with the next payload word's POS
        let start_pos = if payload_i < payload.len() {
            // Get the next payload word's allowed POS tags
            // Prefer N, V, Adj in that order for better grammar
            let next_word = &payload[payload_i];
            
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
            
            if next_word.allowed.contains(&Pos::N) {
                Some(Pos::N)
            } else if next_word.allowed.contains(&Pos::V) {
                Some(Pos::V)
            } else if next_word.allowed.contains(&Pos::Adj) {
                Some(Pos::Adj)
            } else if next_word.allowed.contains(&Pos::Adv) {
                Some(Pos::Adv)
            } else if next_word.allowed.contains(&Pos::Prep) {
                Some(Pos::Prep)
            } else {
                // Fallback to first available POS (should always exist since we checked for empty above)
                Some(next_word.allowed.iter().next().copied().expect("Payload word should have at least one POS tag"))
            }
        } else {
            None
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
        
        // Generate a sentence - it MUST contain at least one payload word
        // If start_pos is set, the grammar and fill_slots are designed to guarantee placement
        let mut slots = Vec::new();
        expand_cfg(rng, Sym::NT("S"), &mut slots, sentence_min, start_pos);
        
        let mut current_payload_i = payload_i;
        let mut sentence_words = fill_slots(
            rng, 
            lex, 
            &slots, 
            payload, 
            &mut current_payload_i,
            &prev_words_refs,
            start_pos  // Pass start_pos to ensure payload word placement
        );
        
        // Panic if no payload word was placed - this indicates a bug in the grammar or fill_slots logic
        if current_payload_i == payload_i_before {
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
                    Pos::Dot => "Dot".to_string(),
                }
            }).collect();
            
            let next_payload_word = if payload_i < payload.len() {
                format!("{} (allowed POS: {:?})", payload[payload_i].word, payload[payload_i].allowed)
            } else {
                "none".to_string()
            };
            
            panic!(
                "BUG: Generated sentence with no payload words!\n\
                 Expected start_pos: {:?}\n\
                 Next payload word: {}\n\
                 Generated slots: {}\n\
                 Sentence: {}\n\
                 This should never happen - the grammar and fill_slots should guarantee payload word placement.",
                start_pos,
                next_payload_word,
                slots_str.join(" "),
                sentence_words.join(" ")
            );
        }
        
        payload_i = current_payload_i;

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
                    Pos::Dot => "Dot".to_string(),
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
                        Pos::Dot => "Dot", // Shouldn't happen here
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
                                Pos::Dot => "Dot",
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
                                HighlightMode::Highlight => wrap_payload_with_highlight(w),
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

    // Post-fix: ensure output ends with a period.
    if let Some(last) = words.last_mut() {
        if !last.ends_with('.') {
            last.push('.');
        }
    }

    // Apply highlighting to BIP39 words according to highlight_mode.
    let rendered_words: Vec<String> = words
        .iter()
        .map(|word| {
            let word_clean = normalize_token_for_bip39(word);
            if !word_clean.is_empty() && payload_set.contains(&word_clean) {
                match highlight_mode {
                    HighlightMode::None => word.clone(),
                    HighlightMode::Bars => wrap_payload_with_bars(word),
                    HighlightMode::Highlight => wrap_payload_with_highlight(word),
                }
            } else {
                word.clone()
            }
        })
        .collect();

    (rendered_words.join(" "), payload_set)
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

fn wrap_payload_with_highlight(word_with_punct: &str) -> String {
    // Highlight the word using ANSI escape codes (green color) while keeping trailing punctuation outside.
    // Example: "abandon." -> "\x1b[32mabandon\x1b[0m."
    // Example: "Abandon"  -> "\x1b[32mAbandon\x1b[0m"
    // ANSI color codes: 32 = green, 33 = yellow, 34 = blue, 35 = magenta, 36 = cyan
    let mut core = word_with_punct.to_string();
    let mut suffix = String::new();
    while let Some(last) = core.chars().last() {
        if last.is_ascii_alphabetic() {
            break;
        }
        core.pop();
        suffix.insert(0, last);
    }
    format!("\x1b[32m{core}\x1b[0m{suffix}")
}

/// Build comprehensive POS mapping for all BIP39 words.
/// Returns a HashMap mapping each word to its allowed POS tags.
/// Reads POS tags from the file format: word|POS1,POS2,...
fn build_pos_mapping() -> HashMap<String, Vec<Pos>> {
    let wordlist = include_str!("../bip39_POS.txt");
    let mut mapping = HashMap::new();
    
    for line in wordlist.lines() {
        let line: &str = line.trim();
        if line.is_empty() {
            continue;
        }
        
        // Parse format: word|POS1,POS2,... or just word (backward compatibility)
        let (word, pos_tags) = if let Some(pipe_idx) = line.find('|') {
            let word = line[..pipe_idx].trim().to_lowercase();
            let pos_str = line[pipe_idx + 1..].trim();
            let pos_tags = parse_pos_tags(pos_str);
            // If POS tags are empty (e.g., "word|"), fall back to heuristics
            if pos_tags.is_empty() {
                (word.clone(), assign_pos_tags(&word))
            } else {
                (word, pos_tags)
            }
        } else {
            // Backward compatibility: no POS tags, use heuristics
            let word = line.to_lowercase();
            let pos_tags = assign_pos_tags(&word);
            (word, pos_tags)
        };
        
        if !word.is_empty() {
            mapping.insert(word, pos_tags);
        }
    }
    
    mapping
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
            "Dot" => Some(Pos::Dot),
            _ => None,
        })
        .collect()
}

/// Assign POS tags to a word based on comprehensive heuristics.
/// Returns a vector of allowed POS tags (can be multiple).
fn assign_pos_tags(word: &str) -> Vec<Pos> {
    let word_lower = word.to_lowercase();
    
    // Prepositions and adverbs (spatial/temporal)
    let prep_adv_words: HashSet<&str> = [
        "about", "above", "across", "after", "against", "along", "around",
        "before", "behind", "below", "beneath", "beside", "between", "beyond",
        "during", "inside", "outside", "through", "throughout", "under",
        "underneath", "until", "within", "without", "again", "ahead", "almost",
        "already", "also", "apart", "away", "else", "even", "ever", "here",
        "now", "once", "only", "over", "quite", "rather", "since", "so",
        "still", "then", "there", "thus", "together", "too", "very", "well",
        "when", "where", "while", "yet"
    ].iter().copied().collect();
    
    if prep_adv_words.contains(word_lower.as_str()) {
        // Many of these can be both prep and adv
        if ["about", "above", "across", "after", "against", "along", "around",
            "before", "behind", "below", "beneath", "beside", "between", "beyond",
            "during", "inside", "outside", "through", "throughout", "under",
            "underneath", "until", "within", "without", "over", "since"].contains(&word_lower.as_str()) {
            return vec![Pos::Prep, Pos::Adv];
        }
        return vec![Pos::Adv];
    }
    
    // Determiners
    let det_words: HashSet<&str> = [
        "all", "any", "each", "either", "few", "more", "most", "much",
        "neither", "other", "some", "such", "that", "this", "what", "which",
        "several",
    ].iter().copied().collect();
    
    if det_words.contains(word_lower.as_str()) {
        // Treat determiners as determiners only. This prevents cover-Adj lists from
        // producing phrases like "a several ..." and forces payload determiners to
        // embed via Det slots (which is now supported).
        return vec![Pos::Det];
    }
    
    // Verbs (action words, often can be nouns too)
    let verb_endings = ["ed", "ing", "ate", "ify", "ize", "ise"];
    let is_verb_ending = verb_endings.iter().any(|ending| word_lower.ends_with(ending));
    
    // Common verb patterns
    let verb_patterns = [
        "abandon", "absorb", "abuse", "accuse", "achieve", "acquire", "act", "adapt",
        "add", "adjust", "admit", "advance", "afford", "agree", "aim", "allow", "alter",
        "announce", "answer", "appear", "approve", "argue", "arrange", "arrest", "arrive",
        "ask", "assault", "assist", "assume", "attack", "attend", "attract", "avoid",
        "awake", "become", "begin", "behave", "believe", "betray", "bind", "blame",
        "bless", "boil", "boost", "borrow", "bounce", "bring", "build", "burst",
        "call", "cancel", "carry", "catch", "cause", "change", "charge", "chase",
        "chat", "check", "choose", "claim", "clap", "clarify", "clean", "click",
        "climb", "close", "collect", "combine", "come", "conduct", "confirm", "connect",
        "consider", "control", "convince", "cook", "copy", "correct", "cost", "cover",
        "crack", "crash", "crawl", "create", "cross", "crouch", "cruise", "crush",
        "cry", "dance", "dash", "deal", "debate", "decide", "decline", "decorate",
        "decrease", "define", "defy", "delay", "deliver", "demand", "deny", "depart",
        "depend", "deposit", "derive", "describe", "design", "destroy", "detect",
        "develop", "devote", "dial", "differ", "direct", "disagree", "discover",
        "dismiss", "display", "divert", "divide", "divorce", "draw", "dream", "dress",
        "drift", "drill", "drink", "drive", "drop", "dry", "earn", "eat", "edit",
        "educate", "embark", "embody", "embrace", "emerge", "employ", "empower",
        "enable", "enact", "end", "endorse", "enforce", "engage", "enhance", "enjoy",
        "enlist", "enrich", "enroll", "ensure", "enter", "equal", "equip", "erase",
        "erode", "erupt", "escape", "evoke", "evolve", "exchange", "excite", "exclude",
        "excuse", "execute", "exercise", "exhaust", "exhibit", "exist", "exit",
        "expand", "expect", "expire", "explain", "expose", "express", "extend",
        "face", "fade", "fall", "feed", "feel", "fetch", "fight", "fill", "find",
        "finish", "fire", "fit", "fix", "flash", "flee", "flip", "float", "fly",
        "focus", "fold", "follow", "forget", "found", "frame", "freeze", "gain",
        "gather", "gaze", "get", "give", "glance", "glare", "glide", "glow", "go",
        "govern", "grab", "grant", "grow", "grunt", "guard", "guess", "guide",
        "handle", "hang", "happen", "harvest", "have", "head", "hear", "help",
        "hide", "hire", "hit", "hold", "hope", "hover", "hunt", "hurry", "hurt",
        "identify", "ignore", "imitate", "impose", "improve", "include", "increase",
        "indicate", "inflict", "inform", "inhale", "inherit", "inject", "inspire",
        "install", "invest", "invite", "involve", "isolate", "join", "judge", "jump",
        "keep", "kick", "kill", "kiss", "knock", "know", "label", "laugh", "launch",
        "lay", "lead", "learn", "leave", "lend", "let", "level", "lift", "light",
        "like", "limit", "link", "list", "listen", "live", "load", "lock", "look",
        "lose", "love", "make", "manage", "march", "mark", "marry", "match", "matter",
        "mean", "measure", "meet", "melt", "mention", "merge", "mind", "miss", "mix",
        "modify", "monitor", "move", "multiply", "name", "need", "neglect", "obey",
        "oblige", "observe", "obtain", "occur", "offer", "open", "operate", "oppose",
        "order", "organize", "orient", "own", "pack", "paddle", "paint", "park",
        "part", "pass", "pause", "pay", "pave", "perform", "permit", "pick", "place",
        "plan", "plant", "play", "pledge", "pluck", "plug", "plunge", "point",
        "polish", "pop", "pose", "position", "possess", "post", "pour", "practice",
        "praise", "pray", "predict", "prefer", "prepare", "present", "press",
        "prevent", "print", "process", "produce", "profit", "program", "progress",
        "project", "promise", "promote", "proof", "propose", "protect", "protest",
        "prove", "provide", "pull", "pump", "punch", "purchase", "push", "put",
        "qualify", "question", "quit", "quote", "race", "raise", "rally", "range",
        "rank", "rate", "reach", "react", "read", "realize", "recall", "receive",
        "recognize", "record", "recover", "recycle", "reduce", "refer", "reflect",
        "refuse", "regret", "reject", "relate", "relax", "release", "rely", "remain",
        "remember", "remind", "remove", "render", "renew", "rent", "repair", "repeat",
        "replace", "reply", "report", "represent", "require", "rescue", "resemble",
        "resist", "resolve", "respond", "rest", "restore", "restrict", "result",
        "retire", "retreat", "return", "reveal", "review", "reward", "ride", "ring",
        "rise", "risk", "roll", "rotate", "round", "rule", "run", "rush", "sail",
        "save", "say", "scale", "scan", "scare", "scatter", "schedule", "scheme",
        "score", "scrape", "scratch", "scream", "screen", "screw", "script", "scrub",
        "seal", "search", "seat", "see", "seek", "seem", "select", "sell", "send",
        "sense", "separate", "serve", "set", "settle", "shake", "shape", "share",
        "shed", "shell", "shelter", "shift", "shine", "ship", "shock", "shoot",
        "shop", "shoulder", "shout", "shove", "show", "shrug", "shuffle", "shut",
        "sigh", "sign", "signal", "sing", "sink", "sit", "situate", "skate", "sketch",
        "ski", "skill", "skip", "slap", "slam", "sleep", "slice", "slide", "slip",
        "slow", "smash", "smell", "smile", "smoke", "snap", "sniff", "snow", "soak",
        "solve", "sort", "sound", "spare", "spawn", "speak", "specify", "speed",
        "spell", "spend", "spin", "spit", "split", "spoil", "sponsor", "spot",
        "spray", "spread", "spring", "squeeze", "stabilize", "stack", "staff",
        "stage", "stamp", "stand", "start", "state", "stay", "steal", "steer",
        "stem", "step", "stick", "sting", "stir", "stock", "stop", "store", "storm",
        "strain", "strand", "strap", "stream", "strengthen", "stress", "stretch",
        "strike", "string", "strip", "strive", "stroke", "structure", "struggle",
        "study", "stuff", "stumble", "style", "subject", "submit", "substitute",
        "succeed", "suck", "suffer", "suggest", "suit", "sum", "summarize", "supply",
        "support", "suppose", "suppress", "surface", "surge", "surprise", "surrender",
        "surround", "survey", "survive", "suspect", "suspend", "sustain", "swallow",
        "swap", "swear", "sweep", "swell", "swim", "swing", "switch", "symbolize",
        "sympathize", "synchronize", "synthesize", "systematize", "table", "tackle",
        "tag", "tail", "take", "talk", "tame", "tap", "target", "task", "taste",
        "tax", "teach", "team", "tear", "tease", "tell", "tempt", "tend", "term",
        "test", "testify", "thank", "think", "thrive", "throw", "thrust", "thumb",
        "tick", "tide", "tie", "tighten", "tilt", "time", "tip", "tire", "title",
        "toast", "tolerate", "tone", "top", "topple", "toss", "total", "touch",
        "tour", "tow", "trace", "track", "trade", "trail", "train", "transfer",
        "transform", "translate", "transmit", "transport", "trap", "travel", "treat",
        "tremble", "trend", "trial", "trick", "trigger", "trim", "trip", "triumph",
        "trouble", "trust", "try", "tuck", "tug", "tumble", "tune", "turn", "tutor",
        "twist", "type", "undergo", "understand", "undertake", "undo", "unfold",
        "unify", "unite", "unlock", "unpack", "unveil", "update", "upgrade",
        "uphold", "upset", "urge", "use", "utilize", "utter", "vacate", "validate",
        "value", "vanish", "vary", "venture", "verify", "veto", "vibrate", "view",
        "violate", "visit", "visualize", "voice", "void", "volunteer", "vote",
        "voyage", "wade", "wage", "wait", "wake", "walk", "wall", "wander", "want",
        "warm", "warn", "wash", "waste", "watch", "water", "wave", "weaken", "wear",
        "weave", "wed", "weed", "weep", "weigh", "welcome", "weld", "wet", "whip",
        "whirl", "whisper", "whistle", "win", "wind", "wink", "wipe", "wire",
        "wish", "withdraw", "withhold", "withstand", "witness", "wonder", "work",
        "worry", "worship", "worth", "wrap", "wreck", "wrestle", "wring", "write",
        "wrong"
    ];
    
    let is_verb = verb_patterns.iter().any(|&v| word_lower == v) || is_verb_ending;
    
    // Common BIP39 nouns that should always be tagged as nouns (not adjectives)
    // These words are incorrectly matched by adjective patterns
    let noun_exceptions: HashSet<&str> = [
        "donkey", "west", "slot", "crane", "horn", "danger", "city", "country",
        "valley", "journey", "money", "story", "history", "victory", "factory",
        "library", "category", "theory", "memory", "discovery", "delivery",
        "company", "family", "army", "enemy", "party", "body", "copy", "key",
        "way", "day", "may", "say", "play", "stay", "pay", "ray", "tray",
        "delay", "display", "essay", "highway", "holiday", "relay", "spray",
        "survey", "turkey", "valley", "alley", "attorney", "chimney", "honey",
        "journey", "kidney", "money", "monkey", "turkey", "volley"
    ].iter().copied().collect();
    
    // Check noun exceptions first (before adjective patterns)
    if noun_exceptions.contains(word_lower.as_str()) {
        return vec![Pos::N];
    }
    
    // Nouns (things, people, places, concepts) - check BEFORE adjectives
    let noun_endings = ["tion", "sion", "ness", "ment", "ity", "ty", "er", "or",
                        "ist", "ism", "age", "ance", "ence", "dom", "hood", "ship",
                        "ure", "ture", "sure"];
    let is_noun_ending = noun_endings.iter().any(|ending| word_lower.ends_with(ending));
    
    // Check for consonant+y ending (like "donkey", "city") - these are usually nouns
    let is_consonant_y = word_lower.len() >= 2 && word_lower.ends_with('y') && {
        let prev_char = word_lower.chars().rev().nth(1).unwrap();
        !matches!(prev_char, 'a' | 'e' | 'i' | 'o' | 'u')
    };
    
    // Adjectives (descriptive words)
    // Remove "y" from adj_endings since we handle it specially for consonant+y
    let adj_endings = ["able", "ible", "ful", "less", "ic", "ical", "al", "ary",
                       "ive", "ous", "ious", "ish", "ed", "ing", "en"];
    let is_adj_ending = adj_endings.iter().any(|ending| word_lower.ends_with(ending));
    
    // Handle "y" ending specially: consonant+y is usually a noun, vowel+y can be adjective
    let is_adj_y = word_lower.ends_with('y') && !is_consonant_y;
    
    let adj_patterns = [
        "able", "absent", "abstract", "absurd", "actual", "afraid", "ahead",
        "alone", "amazing", "ancient", "angry", "annual", "another", "anxious",
        "apart", "armed", "arctic", "artificial", "asleep", "athletic", "atomic",
        "attractive", "automatic", "available", "average", "awake", "aware",
        "awesome", "awful", "awkward", "bad", "bare", "basic", "beautiful",
        "best", "better", "big", "bitter", "black", "blank", "bleak", "blind",
        "blue", "bold", "boring", "brave", "brief", "bright", "brilliant",
        "brisk", "broken", "brown", "busy", "calm", "capable", "careful",
        "casual", "certain", "cheap", "chief", "chosen", "chronic", "civil",
        "clean", "clear", "clever", "close", "cloudy", "cold", "common",
        "complete", "complex", "confident", "conscious", "considerable",
        "constant", "cool", "correct", "crucial", "cruel", "curious", "current",
        "cute", "damp", "dangerous", "daring", "dark", "dead", "dear", "decent",
        "deep", "definite", "delicate", "delicious", "dense", "dependent",
        "desperate", "determined", "different", "difficult", "digital", "direct",
        "dirty", "disabled", "disappointed", "disastrous", "discrete", "distant",
        "distinct", "distinguished", "disturbed", "diverse", "divine", "divorced",
        "domestic", "dominant", "double", "doubtful", "dramatic", "drastic",
        "dreadful", "dry", "dumb", "durable", "dutch", "dying", "dynamic",
        "eager", "early", "east", "easy", "economic", "edible", "effective",
        "efficient", "elder", "elderly", "electric", "elegant", "eligible",
        "elite", "embarrassed", "empty", "endless", "enormous", "enough",
        "entire", "equal", "equivalent", "essential", "eternal", "ethnic",
        "even", "eventual", "every", "evident", "evil", "exact", "excellent",
        "exceptional", "excessive", "excited", "exciting", "exclusive",
        "exotic", "expensive", "experienced", "explicit", "express", "extensive",
        "extra", "extraordinary", "extreme", "faint", "fair", "faithful",
        "fake", "false", "familiar", "famous", "fantastic", "far", "fascinating",
        "fast", "fat", "fatal", "faulty", "favorite", "federal", "female",
        "few", "fierce", "final", "financial", "fine", "firm", "fiscal",
        "fit", "fixed", "flat", "flexible", "fluent", "fluid", "focused",
        "foolish", "foreign", "formal", "former", "fortunate", "forward",
        "fragile", "free", "frequent", "fresh", "friendly", "frightened",
        "front", "frozen", "full", "fun", "funny", "furious", "future",
        "gay", "general", "generous", "gentle", "genuine", "giant", "gifted",
        "glad", "glorious", "golden", "good", "gorgeous", "grateful", "grave",
        "gray", "great", "greedy", "green", "gross", "growing", "guilty",
        "half", "handsome", "happy", "hard", "harmful", "harsh", "hateful",
        "head", "healthy", "heavy", "helpful", "helpless", "hidden", "high",
        "hilarious", "holy", "honest", "honorable", "hopeful", "hopeless",
        "horrible", "hot", "huge", "human", "humble", "hungry", "hurt",
        "ideal", "identical", "idle", "ignorant", "ill", "illegal", "illiterate",
        "imaginary", "immediate", "immense", "immune", "impatient", "imperfect",
        "impersonal", "important", "impossible", "impressed", "impressive",
        "improved", "inadequate", "inappropriate", "incapable", "incompetent",
        "incomplete", "incredible", "indeed", "independent", "indian", "indifferent",
        "indirect", "indispensable", "individual", "indoor", "industrial",
        "inevitable", "inexpensive", "infant", "inferior", "infinite", "influential",
        "informal", "inherent", "initial", "injured", "innocent", "innovative",
        "insecure", "inside", "insightful", "insignificant", "inspired",
        "instant", "institutional", "instrumental", "insufficient", "intact",
        "integral", "intellectual", "intelligent", "intended", "intense",
        "intensive", "intentional", "interested", "interesting", "interim",
        "interior", "intermediate", "internal", "international", "intimate",
        "intolerant", "intricate", "intrigued", "intrinsic", "introductory",
        "invalid", "invaluable", "invasive", "inventive", "invisible", "involved",
        "irrelevant", "irresponsible", "irritated", "isolated", "jealous",
        "joint", "jolly", "joyful", "judicial", "junior", "just", "keen",
        "key", "kind", "known", "large", "last", "late", "latin", "latter",
        "lazy", "leading", "left", "legal", "legitimate", "leisure", "lengthy",
        "less", "level", "liable", "liberal", "light", "like", "likely",
        "limited", "linear", "linguistic", "liquid", "literary", "little",
        "live", "lively", "living", "local", "logical", "lonely", "long",
        "loose", "lost", "loud", "lovely", "low", "loyal", "lucky", "lunar",
        "luxury", "mad", "magic", "magnetic", "magnificent", "main", "major",
        "male", "mammal", "manageable", "managerial", "mandatory", "manipulative",
        "manual", "many", "marginal", "marine", "marked", "married", "marvelous",
        "mass", "massive", "master", "maternal", "mathematical", "mature",
        "maximum", "mean", "meaningful", "mechanical", "medical", "medieval",
        "medium", "melodic", "memorable", "mental", "mere", "merry", "messy",
        "metallic", "methodical", "meticulous", "middle", "mighty", "mild",
        "military", "minimal", "minimum", "minor", "minute", "miraculous",
        "miserable", "misleading", "missing", "mistaken", "mixed", "mobile",
        "moderate", "modern", "modest", "molecular", "momentary", "monetary",
        "monthly", "moral", "more", "most", "mother", "motion", "motivated",
        "motor", "mountain", "moving", "much", "multiple", "musical", "mutual",
        "mysterious", "naive", "naked", "narrow", "nasty", "national", "native",
        "natural", "naughty", "near", "nearby", "neat", "necessary", "negative",
        "neglected", "neighboring", "nervous", "net", "neutral", "new", "next",
        "nice", "noble", "noisy", "nominal", "non", "normal", "north", "notable",
        "noted", "notorious", "novel", "nuclear", "numerous", "obedient",
        "objective", "obligatory", "obscure", "observant", "obsolete", "obvious",
        "occasional", "occupational", "odd", "off", "offensive", "official",
        "okay", "old", "olympic", "only", "open", "operational", "opposite",
        "optical", "optimistic", "optional", "oral", "orange", "ordinary",
        "organic", "organizational", "original", "other", "outdoor", "outer",
        "outgoing", "outstanding", "outside", "outward", "oval", "over",
        "overall", "overhead", "overseas", "overwhelming", "own", "pacific",
        "packed", "painful", "pale", "parallel", "parental", "parliamentary",
        "partial", "particular", "passionate", "passive", "past", "patient",
        "peaceful", "peculiar", "perfect", "permanent", "persistent", "personal",
        "pet", "philosophical", "physical", "pink", "plain", "planned",
        "plastic", "pleasant", "pleased", "plenty", "plus", "poetic", "poignant",
        "polar", "polite", "political", "poor", "popular", "portable", "positive",
        "possible", "post", "potential", "powerful", "practical", "precious",
        "precise", "predictable", "preferred", "pregnant", "preliminary",
        "premium", "prepared", "present", "presidential", "pretty", "previous",
        "primary", "prime", "primitive", "principal", "prior", "private",
        "privileged", "probable", "productive", "professional", "profitable",
        "profound", "progressive", "prominent", "promising", "proper", "proposed",
        "prosperous", "proud", "provincial", "psychiatric", "psychological",
        "public", "pure", "purple", "purposeful", "puzzled", "qualified",
        "quantitative", "quantum", "quick", "quiet", "racial", "radical",
        "random", "rapid", "rare", "rational", "raw", "ready", "real", "realistic",
        "rear", "reasonable", "recent", "reckless", "recognized", "recommended",
        "record", "recovered", "red", "reduced", "redundant", "regional",
        "registered", "regular", "regulatory", "related", "relative", "relevant",
        "reliable", "relieved", "religious", "reluctant", "remarkable",
        "remote", "renewed", "representative", "republican", "required",
        "residential", "resistant", "respectable", "respective", "responsible",
        "restless", "restricted", "resulting", "retired", "revolutionary",
        "rich", "ridiculous", "right", "rigid", "ripe", "rising", "rival",
        "romantic", "rotten", "rough", "round", "royal", "rubber", "rude",
        "ruling", "rural", "sad", "safe", "satisfied", "satisfying", "savage",
        "scared", "scary", "scientific", "seasonal", "secondary", "secret",
        "secure", "select", "selected", "selective", "self", "senior",
        "sensible", "sensitive", "separate", "serious", "several", "severe",
        "sexual", "shallow", "sharp", "sheer", "shiny", "shocked", "short",
        "shy", "sick", "significant", "silent", "silly", "silver", "similar",
        "simple", "simultaneous", "sincere", "single", "skilled", "sleepy",
        "slight", "slim", "slow", "small", "smart", "smooth", "so", "social",
        "soft", "solar", "sole", "solid", "solo", "some", "sophisticated",
        "sorry", "sound", "south", "southern", "spare", "spatial", "special",
        "specific", "spectacular", "speculative", "spiritual", "splendid",
        "spontaneous", "square", "stable", "stale", "standard", "standing",
        "static", "statistical", "steady", "steep", "sticky", "stiff", "still",
        "stirring", "stock", "stolen", "straight", "strange", "strategic",
        "strict", "striking", "strong", "structural", "stuck", "stupid",
        "subject", "subjective", "subsequent", "substantial", "subtle",
        "successful", "successive", "such", "sudden", "sufficient", "suitable",
        "sunny", "super", "superb", "superior", "supervised", "supplementary",
        "supreme", "sure", "surprised", "surprising", "surrounding", "suspicious",
        "sustainable", "sweet", "swift", "symbolic", "sympathetic", "synthetic",
        "systematic", "tall", "tame", "tan", "tangible", "tart", "tasteful",
        "tasty", "taxable", "technical", "technological", "tedious", "teenage",
        "temporary", "tender", "tense", "terminal", "terrible", "terrific",
        "territorial", "terrorist", "test", "thankful", "that", "theoretical",
        "thick", "thin", "thirsty", "thorough", "thoughtful", "threatening",
        "tight", "tiny", "tired", "tiresome", "tolerant", "top", "total",
        "tough", "tourist", "toxic", "traditional", "tragic", "trained",
        "transparent", "traumatic", "tremendous", "tricky", "tropical",
        "troubled", "true", "trusted", "trusting", "trustworthy", "truthful",
        "turbulent", "typical", "ugly", "ultimate", "unable", "unacceptable",
        "unaware", "uncertain", "uncomfortable", "uncommon", "unconscious",
        "under", "underground", "underlying", "understandable", "understood",
        "undesirable", "uneasy", "unemployed", "unequal", "unexpected",
        "unfair", "unfamiliar", "unfortunate", "unhappy", "unhealthy",
        "uniform", "uninterested", "unique", "universal", "unknown", "unlawful",
        "unlike", "unlikely", "unnecessary", "unpleasant", "unprecedented",
        "unrealistic", "unreasonable", "unrelated", "unreliable", "unsatisfied",
        "unsuccessful", "unsuitable", "unsure", "unusual", "unwilling", "upper",
        "upset", "upstairs", "urban", "urgent", "useful", "useless", "usual",
        "vague", "valid", "valuable", "variable", "varied", "various", "vast",
        "verbal", "vertical", "very", "viable", "vibrant", "vicious", "victorious",
        "video", "vigorous", "violent", "virtual", "visible", "visual", "vital",
        "vivid", "vocal", "voluntary", "vulnerable", "warm", "wary", "wasteful",
        "watery", "weak", "wealthy", "weary", "weekly", "weird", "welcome",
        "well", "western", "wet", "what", "whatever", "which",
        "whichever", "white", "whole", "wide", "widespread", "wild", "willing",
        "wise", "witty", "wonderful", "wooden", "woolen", "working", "world",
        "worldwide", "worried", "worse", "worst", "worth", "worthwhile",
        "worthy", "wounded", "wrong", "yellow", "young", "youthful", "zealous"
    ];
    
    // Exclude noun exceptions from adj_patterns check
    let is_adj_pattern = adj_patterns.iter().any(|&a| word_lower == a) && !noun_exceptions.contains(word_lower.as_str());
    let is_adj = is_adj_pattern || is_adj_ending || is_adj_y;
    
    // Return only one POS tag per word, prioritizing: Verb > Noun > Adjective
    // Changed priority: check nouns before adjectives to fix incorrect tagging
    if is_verb {
        return vec![Pos::V];
    }
    
    // Check nouns before adjectives (fixes issues like "donkey", "west", "slot", etc.)
    if is_noun_ending || is_consonant_y {
        return vec![Pos::N];
    }
    
    if is_adj {
        return vec![Pos::Adj];
    }
    
    // Default to noun if no pattern matches
    vec![Pos::N]
}

/// Get POS tags for a word from the comprehensive mapping.
/// Returns a vector of allowed POS tags.
fn tag_word(word: &str) -> Vec<Pos> {
    static POS_MAP: OnceLock<HashMap<String, Vec<Pos>>> = OnceLock::new();
    
    let mapping = POS_MAP.get_or_init(|| {
        build_pos_mapping()
    });
    
    let word_lower = word.to_lowercase();
    mapping.get(&word_lower)
        .cloned()
        .unwrap_or_else(|| {
            // Fallback: use heuristics if word not found
            assign_pos_tags(&word_lower)
        })
}

/// Load all BIP39 words from the wordlist file.
/// Handles both formats: word|POS (new) and word (old, backward compatibility).
fn load_bip39_words() -> Vec<String> {
    let wordlist = include_str!("../bip39_POS.txt");
    wordlist
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
        .collect()
}

/// Load safe words from the safe-word-list file.
/// Returns words filtered to exclude BIP39 words.
fn load_safe_words(bip39_set: &HashSet<String>) -> Vec<String> {
    let safe_words = include_str!("../safe-words.txt");
    safe_words
        .lines()
        .map(|line| line.trim().to_lowercase())
        .filter(|word| !word.is_empty())
        .filter(|word| !bip39_set.contains(word))
        .collect()
}

/// Randomly select N words from the BIP39 wordlist.
fn select_random_words<R: Rng>(rng: &mut R, count: usize) -> Vec<String> {
    let all_words = load_bip39_words();
    if all_words.is_empty() || count == 0 {
        return Vec::new();
    }

    // Sample WITH replacement so duplicates are possible (and therefore decodable).
    let mut selected = Vec::with_capacity(count);
    for _ in 0..count {
        selected.push(all_words.choose(rng).unwrap().clone());
    }
    selected
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
    eprintln!("  --highlight <mode>      Highlight BIP39 words: 'none', 'bars' (default), or 'highlight'");
    eprintln!("  --seed <N>              Seed for deterministic random generation");
    eprintln!("  --variations <N>         Generate N variations and select the most compact (default: 1)");
    eprintln!("  --verbose, -v           Show detailed debugging information");
    eprintln!("  --help                  Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {} abandon ability able about above absent", program_name);
    eprintln!("  {} --random 10", program_name);
    eprintln!("  {} --random 5 --highlight none", program_name);
    eprintln!("  {} --random 5 --highlight highlight", program_name);
}

fn parse_args() -> Result<(Vec<String>, Option<usize>, bool, Option<u64>, usize, HighlightMode), String> {
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
            "--highlight" => {
                if i + 1 >= args.len() {
                    return Err("--highlight requires a value".to_string());
                }
                highlight_mode = match args[i + 1].as_str() {
                    "none" => HighlightMode::None,
                    "bars" => HighlightMode::Bars,
                    "highlight" => HighlightMode::Highlight,
                    _ => return Err(format!("Invalid highlight mode: {}. Use 'none', 'bars', or 'highlight'", args[i + 1])),
                };
                i += 2;
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
    
    if random_count.is_none() && words.is_empty() {
        return Err("No words provided. Use --random <N> or provide words as arguments.".to_string());
    }
    
    Ok((words, random_count, verbose, seed, variations, highlight_mode))
}

// --- CLI usage ---
fn main() {
    let (mut words, random_count, verbose, seed, variations, highlight_mode) = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!();
            print_usage(&env::args().next().unwrap_or_else(|| "bip39-encode".to_string()));
            std::process::exit(1);
        }
    };

    // Use seeded RNG if seed provided, otherwise generate random seed from thread_rng
    let seed_value = if let Some(s) = seed {
        s
    } else {
        // Generate a random seed from thread_rng for non-deterministic behavior
        rand::thread_rng().gen::<u64>()
    };
    let mut rng = StdRng::seed_from_u64(seed_value);
    
    if verbose && seed.is_some() {
        eprintln!("Using seed: {}", seed_value);
    }

    // If random words requested, select them now
    if let Some(count) = random_count {
        words = select_random_words(&mut rng, count);
        eprintln!("Selected {} random BIP39 words: {}", count, words.join(" "));
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
    let bip39_words = load_bip39_words();
    let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();
    
    // Load safe words from safe-word-list (filtered to exclude BIP39 words)
    let safe_words = load_safe_words(&bip39_set);
    
    // Categorize safe words by POS
    let mut adj_words: Vec<String> = Vec::new();
    let mut n_words: Vec<String> = Vec::new();
    let mut v_words: Vec<String> = Vec::new();
    let mut prep_words: Vec<String> = Vec::new();
    let mut adv_words: Vec<String> = Vec::new();
    
    for word in &safe_words {
        let tags = tag_word(word);
        // Skip words with no POS tags
        if tags.is_empty() {
            continue;
        }
        for tag in tags {
            match tag {
                Pos::Adj => adj_words.push(word.clone()),
                Pos::N => n_words.push(word.clone()),
                Pos::V => v_words.push(word.clone()),
                Pos::Prep => prep_words.push(word.clone()),
                Pos::Adv => adv_words.push(word.clone()),
                _ => {} // Skip Det, Dot
            }
        }
    }
    
    // Remove duplicates while preserving order (words can have multiple POS tags)
    adj_words.sort();
    adj_words.dedup();
    n_words.sort();
    n_words.dedup();
    v_words.sort();
    v_words.dedup();
    prep_words.sort();
    prep_words.dedup();
    adv_words.sort();
    adv_words.dedup();
    
    // Keep function words as small fixed sets.
    // IMPORTANT: these must NOT overlap the BIP39 list (validated below).
    let det_words = ["the", "a", "an", "these", "those", "each", "some"];
    // Modal verbs that are NOT in the BIP39 list (e.g., "can"/"will"/"must" are BIP39!).
    let modal_words = ["should", "could", "would", "might", "may"];
    // Aux/Cop/To are inserted with agreement logic (not randomized) but still validated here.
    let aux_words = ["do", "does"];
    let cop_words = ["is", "are"];
    let to_words = ["to"];
    
    // Convert to slices for the lexicon
    let adj_words_slice: Vec<&str> = adj_words.iter().map(|s| s.as_str()).collect();
    let n_words_slice: Vec<&str> = n_words.iter().map(|s| s.as_str()).collect();
    let v_words_slice: Vec<&str> = v_words.iter().map(|s| s.as_str()).collect();
    let prep_words_slice: Vec<&str> = prep_words.iter().map(|s| s.as_str()).collect();
    let adv_words_slice: Vec<&str> = adv_words.iter().map(|s| s.as_str()).collect();
    
    // Validate all cover words against BIP39 wordlist
    let all_cover_words: Vec<&str> = det_words.iter()
        .chain(modal_words.iter())
        .chain(aux_words.iter())
        .chain(cop_words.iter())
        .chain(to_words.iter())
        .chain(adj_words_slice.iter())
        .chain(n_words_slice.iter())
        .chain(v_words_slice.iter())
        .chain(prep_words_slice.iter())
        .chain(adv_words_slice.iter())
        .copied()
        .collect();
    
    validate_cover_words(&all_cover_words, &bip39_set);
    
    if verbose {
        eprintln!("Loaded {} safe words (excluding BIP39):", safe_words.len());
        eprintln!("  Adjectives: {}", adj_words.len());
        eprintln!("  Nouns: {}", n_words.len());
        eprintln!("  Verbs: {}", v_words.len());
        eprintln!("  Prepositions: {}", prep_words.len());
        eprintln!("  Adverbs: {}", adv_words.len());
    }
    
    let lex = Lexicon::new(payload_set, bip39_set.clone())
        .with_words(Pos::Det, &det_words)
        .with_words(Pos::Modal, &modal_words)
        .with_words(Pos::Adj, &adj_words_slice)
        .with_words(Pos::N, &n_words_slice)
        .with_words(Pos::V, &v_words_slice)
        .with_words(Pos::Prep, &prep_words_slice)
        .with_words(Pos::Adv, &adv_words_slice);

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

    if verbose && variations > 1 {
        eprintln!("Generating {} variations to maximize compactness...", variations);
    }

    for variation in 0..variations {
        // Use different seeds for each variation (increment base seed)
        let variation_seed = seed_value.wrapping_add(variation as u64);
        let mut variation_rng = StdRng::seed_from_u64(variation_seed);
        
        let (text, payload_set_from_gen) = generate_text(&mut variation_rng, &lex, &payload, highlight_mode, variations == 1 && verbose);
        
        // Validate that the generated text contains exactly the input BIP39 words in order
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
        
        // Calculate compactness score for this variation (based on character counts)
        let words: Vec<&str> = text.split_whitespace().collect();
        let output_word_count = words.len();
        let mut bip39_chars = 0;
        let mut non_bip39_chars = 0;
        
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
        
        let total_chars = bip39_chars + non_bip39_chars;
        let compactness = if total_chars > 0 {
            bip39_chars as f64 / total_chars as f64
        } else {
            0.0
        };

        variation_stats.push(compactness);

        if verbose && variations > 1 {
            eprintln!("Variation {}: compactness {:.3} ({} BIP39 chars / {} total chars)", 
                      variation + 1, compactness, bip39_chars, total_chars);
        }

        // Keep track of the best (most compact) variation
        if compactness > best_compactness {
            best_compactness = compactness;
            best_text = Some(text);
            best_output_count = output_word_count;
        }
    }

    // Output the best variation
    let text = match best_text {
        Some(t) => t,
        None => {
            eprintln!("Error: Failed to generate any valid variations after {} attempts.", variations);
            eprintln!("This may happen if:");
            eprintln!("  - The grammar cannot accommodate all input words");
            eprintln!("  - There are POS tagging issues with some words");
            eprintln!("  - The word extraction logic is failing");
            eprintln!("\nTry running with --verbose to see detailed error messages.");
            std::process::exit(1);
        }
    };
    
    println!("{}", text);

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
                    Pos::Dot => "Punctuation",
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
    use bip39_encode::GrammarChecker;

    /// Fixed seed for reproducible tests
    const TEST_SEED: u64 = 42;

    /// Helper function to set up a test lexicon with minimal cover words
    fn setup_test_lexicon(payload_set: HashSet<String>, bip39_set: HashSet<String>) -> Lexicon {
        let det_words = ["the", "a", "an", "each", "some"];
        let modal_words = ["should", "could", "would", "might", "may"];
        let adj_words = ["bright", "clear", "simple", "secure", "quiet", "steady"];
        let n_words = ["wallet", "user", "server", "system", "note"];
        let v_words = ["check", "send", "hold", "verify", "process"];
        let prep_words = ["about", "above", "along", "beneath", "throughout"];
        let adv_words = ["soon", "well", "quite", "very"];

        Lexicon::new(payload_set, bip39_set)
            .with_words(Pos::Det, &det_words)
            .with_words(Pos::Modal, &modal_words)
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
        let words = select_random_words(&mut rng, 10);
        
        let payload: Vec<PayloadTok> = words
            .iter()
            .map(|word| {
                let tags = tag_word(word);
                PayloadTok::new(word.clone(), &tags)
            })
            .collect();

        let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
        let bip39_words = load_bip39_words();
        let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();

        let lex = setup_test_lexicon(payload_set.clone(), bip39_set);
        let (text, _) = generate_text(&mut rng, &lex, &payload, 5, false);

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
            let words = select_random_words(&mut rng, word_count);
            
            let payload: Vec<PayloadTok> = words
                .iter()
                .map(|word| {
                    let tags = tag_word(word);
                    PayloadTok::new(word.clone(), &tags)
                })
                .collect();

            let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
            let bip39_words = load_bip39_words();
            let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();

            let lex = setup_test_lexicon(payload_set, bip39_set);
            let (text, _) = generate_text(&mut rng, &lex, &payload, 5, false);

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
        let words = select_random_words(&mut rng, 5);
        
        let payload: Vec<PayloadTok> = words
            .iter()
            .map(|word| {
                let tags = tag_word(word);
                PayloadTok::new(word.clone(), &tags)
            })
            .collect();

        let payload_set: HashSet<String> = payload.iter().map(|t| t.word.to_lowercase()).collect();
        let bip39_words = load_bip39_words();
        let bip39_set: HashSet<String> = bip39_words.iter().map(|w| w.to_lowercase()).collect();

        let lex = setup_test_lexicon(payload_set, bip39_set);
        let (text, _) = generate_text(&mut rng, &lex, &payload, 5, false);

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
}

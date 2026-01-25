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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pos {
    Det,
    Adj,
    N,
    V,
    Prep,
    Adv,
    Dot,
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
    payload_set: HashSet<String>,
}

impl Lexicon {
    fn new(payload_set: HashSet<String>) -> Self {
        Self {
            by_pos: HashMap::new(),
            payload_set,
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
                !self.payload_set.contains(*w) && 
                !recent_words.iter().any(|&rw| rw == w.as_str())
            })
            .collect();

        if available.is_empty() {
            // If all words would be repeats, fall back to any non-payload word
            let fallback: Vec<&String> = list
                .iter()
                .filter(|w| !self.payload_set.contains(*w))
                .collect();
            if fallback.is_empty() {
                panic!("No available cover words for {:?}", pos);
            }
            return fallback.choose(rng).unwrap().to_string();
        }

        available.choose(rng).unwrap().to_string()
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
                                // Start with determiner + noun: Det N VP Dot
                                vec![Sym::T(Pos::Det), Sym::T(Pos::N), Sym::NT("VP"), Sym::T(Pos::Dot)]
                            }
                            Pos::V => {
                                // Start with verb: V NP Dot (imperative)
                                vec![Sym::T(Pos::V), Sym::NT("NP"), Sym::T(Pos::Dot)]
                            }
                            Pos::Adj => {
                                // Start with adjective: Adj N VP Dot
                                vec![Sym::T(Pos::Adj), Sym::T(Pos::N), Sym::NT("VP"), Sym::T(Pos::Dot)]
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
                                // Normal case: Det NP VP Dot
                                vec![Sym::NT("NP"), Sym::NT("VP"), Sym::T(Pos::Dot)]
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
                    // NP -> Det Adj* N (Adj* = 0..2, but increase if we want length)
                    let extra = if want_long { 2 } else { 0 };
                    let k = rng.gen_range(0..=2) + extra;

                    let mut p = vec![Sym::T(Pos::Det)];
                    for _ in 0..k {
                        // occasional adverb-as-intensifier in adj slot? keep it simple: just Adj
                        p.push(Sym::T(Pos::Adj));
                    }
                    p.push(Sym::T(Pos::N));
                    p
                }
                "VP" => {
                    // VP choices:
                    //  - V
                    //  - V NP
                    //  - Adv V NP
                    //  - V NP PP
                    // Bias to longer if want_long.
                    if want_long {
                        match rng.gen_range(0..3) {
                            0 => vec![Sym::T(Pos::V), Sym::NT("NP")],
                            1 => vec![Sym::T(Pos::Adv), Sym::T(Pos::V), Sym::NT("NP")],
                            _ => vec![Sym::T(Pos::V), Sym::NT("NP"), Sym::NT("PP")],
                        }
                    } else {
                        match rng.gen_range(0..4) {
                            0 => vec![Sym::T(Pos::V)],
                            1 => vec![Sym::T(Pos::V), Sym::NT("NP")],
                            2 => vec![Sym::T(Pos::Adv), Sym::T(Pos::V), Sym::NT("NP")],
                            _ => vec![Sym::T(Pos::V), Sym::NT("NP"), Sym::NT("PP")],
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

/// Fill a slot stream with cover words + payload words (in-order).
/// Returns (words, payload_embedded_count).
/// `prev_words` are the last few words from the previous sentence (if any), to prevent repetition across sentences.
fn fill_slots<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    slots: &[Pos],
    payload: &[PayloadTok],
    payload_i: &mut usize,
    prev_words: &[&str],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    const REPETITION_WINDOW: usize = 3; // Check last 3 words to avoid repetition
    // Cache for words picked early (for a/an selection) to reuse later
    let mut word_cache: HashMap<usize, String> = HashMap::new();

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
                // Build recent words list: prev_words + last few words from current output
                let mut recent_words: Vec<&str> = prev_words.to_vec();
                let start_idx = out.len().saturating_sub(REPETITION_WINDOW);
                recent_words.extend(out[start_idx..].iter().map(|s| s.as_str()));
                
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

                let det_word = if let Some(next) = next_word_str {
                    // Choose a/an based on next word
                    if starts_with_vowel_sound(next) {
                        "an".to_string()
                    } else {
                        "a".to_string()
                    }
                } else {
                    // Default: pick randomly from determiners (excluding a/an to avoid conflicts)
                    let det_options = ["the", "these", "those", "each", "some"];
                    det_options.choose(rng).unwrap().to_string()
                };

                // Check if this determiner would repeat a recent word
                // If so and it's not a/an, pick a different one
                let final_det = if recent_words.contains(&det_word.as_str()) && det_word != "a" && det_word != "an" {
                    // Avoid repetition for non-a/an determiners
                    let det_options = ["the", "these", "those", "each", "some"];
                    det_options.iter()
                        .find(|&&d| !recent_words.contains(&d))
                        .copied()
                        .unwrap_or_else(|| det_options.choose(rng).unwrap())
                        .to_string()
                } else {
                    det_word
                };
                
                out.push(final_det);
            }
            _ => {
                if *payload_i < payload.len() && payload_fits(&payload[*payload_i], slot) {
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
                        let cover_word = lex.pick_cover(rng, slot, &recent_words);
                        out.push(cover_word);
                    }
                }
            }
        }
    }

    out
}

/// Generate sentences until all payload tokens are embedded.
/// Each sentence will have at least min_words_per_sentence words.
/// Returns (formatted_text, payload_set) where formatted_text has BIP39 words highlighted.
fn generate_text<R: Rng>(
    rng: &mut R,
    lex: &Lexicon,
    payload: &[PayloadTok],
    min_words_per_sentence: usize,
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
        // Use the user-specified minimum, but adapt if many payload tokens remain
        let sentence_min = if remaining_payload > 10 {
            min_words_per_sentence.max(18)
        } else if remaining_payload > 5 {
            min_words_per_sentence.max(14)
        } else {
            min_words_per_sentence
        };

        // For the first sentence, start with the first payload word's POS
        let start_pos = if payload_i == 0 && !payload.is_empty() {
            // Get the first allowed POS for the first payload word
            // Prefer N, V, Adj in that order for better grammar
            let first_word = &payload[0];
            if first_word.allowed.contains(&Pos::N) {
                Some(Pos::N)
            } else if first_word.allowed.contains(&Pos::V) {
                Some(Pos::V)
            } else if first_word.allowed.contains(&Pos::Adj) {
                Some(Pos::Adj)
            } else if first_word.allowed.contains(&Pos::Adv) {
                Some(Pos::Adv)
            } else if first_word.allowed.contains(&Pos::Prep) {
                Some(Pos::Prep)
            } else {
                // Fallback to first available POS
                first_word.allowed.iter().next().copied()
            }
        } else {
            None
        };

        let mut slots = Vec::new();
        expand_cfg(rng, Sym::NT("S"), &mut slots, sentence_min, start_pos);

        // Print grammar structure in verbose mode
        if verbose {
            let grammar_str: Vec<String> = slots.iter().map(|pos| {
                match pos {
                    Pos::Det => "Det".to_string(),
                    Pos::Adj => "Adj".to_string(),
                    Pos::N => "N".to_string(),
                    Pos::V => "V".to_string(),
                    Pos::Prep => "Prep".to_string(),
                    Pos::Adv => "Adv".to_string(),
                    Pos::Dot => "Dot".to_string(),
                }
            }).collect();
            eprintln!("Grammar: {}", grammar_str.join(" "));
        }

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
        let mut sentence_words = fill_slots(
            rng, 
            lex, 
            &slots, 
            payload, 
            &mut payload_i,
            &prev_words_refs
        );

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

        // Capitalize the first word of the sentence.
        if let Some(first) = sentence_words.first_mut() {
            *first = capitalize(first);
        }

        // Add spacing between sentences.
        if !words.is_empty() {
            // ensure previous ended with punctuation. (We put '.' on last token)
        }
        words.append(&mut sentence_words);
    }

    // Post-fix: ensure output ends with a period.
    if let Some(last) = words.last_mut() {
        if !last.ends_with('.') {
            last.push('.');
        }
    }

    // Highlight BIP39 words with ANSI color codes
    let highlighted_words: Vec<String> = words
        .iter()
        .map(|word| {
            // Remove punctuation temporarily to check if it's a payload word
            let word_lower = word.to_lowercase();
            let word_clean = word_lower.trim_end_matches('.');
            
            if payload_set.contains(word_clean) {
                // Use ANSI color codes: \x1b[1;32m for bold green, \x1b[0m to reset
                format!("\x1b[1;32m{}\x1b[0m", word)
            } else {
                word.clone()
            }
        })
        .collect();

    (highlighted_words.join(" "), payload_set)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Build comprehensive POS mapping for all BIP39 words.
/// Returns a HashMap mapping each word to its allowed POS tags.
fn build_pos_mapping() -> HashMap<String, Vec<Pos>> {
    let wordlist = include_str!("../english.txt");
    let mut mapping = HashMap::new();
    
    for word in wordlist.lines() {
        let word = word.trim().to_lowercase();
        if word.is_empty() {
            continue;
        }
        
        let pos_tags = assign_pos_tags(&word);
        mapping.insert(word, pos_tags);
    }
    
    mapping
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
        "neither", "other", "some", "such", "that", "this", "what", "which"
    ].iter().copied().collect();
    
    if det_words.contains(word_lower.as_str()) {
        return vec![Pos::Det, Pos::Adj]; // Many determiners can also be adjectives
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
    
    // Adjectives (descriptive words)
    let adj_endings = ["able", "ible", "ful", "less", "ic", "ical", "al", "ary",
                       "ive", "ous", "ious", "y", "ish", "ed", "ing", "en"];
    let is_adj_ending = adj_endings.iter().any(|ending| word_lower.ends_with(ending));
    
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
        "well", "west", "western", "wet", "what", "whatever", "which",
        "whichever", "white", "whole", "wide", "widespread", "wild", "willing",
        "wise", "witty", "wonderful", "wooden", "woolen", "working", "world",
        "worldwide", "worried", "worse", "worst", "worth", "worthwhile",
        "worthy", "wounded", "wrong", "yellow", "young", "youthful", "zealous"
    ];
    
    let is_adj = adj_patterns.iter().any(|&a| word_lower == a) || is_adj_ending;
    
    // Nouns (things, people, places, concepts)
    let noun_endings = ["tion", "sion", "ness", "ment", "ity", "ty", "er", "or",
                        "ist", "ism", "age", "ance", "ence", "dom", "hood", "ship",
                        "ure", "ture", "sure"];
    let is_noun_ending = noun_endings.iter().any(|ending| word_lower.ends_with(ending));
    
    // Return only one POS tag per word, prioritizing: Verb > Adjective > Noun
    if is_verb {
        return vec![Pos::V];
    }
    
    if is_adj {
        return vec![Pos::Adj];
    }
    
    if is_noun_ending {
        return vec![Pos::N];
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
fn load_bip39_words() -> Vec<String> {
    let wordlist = include_str!("../english.txt");
    wordlist
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|word| !word.is_empty())
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
    if count > all_words.len() {
        eprintln!("Warning: Requested {} words but only {} available. Using all words.", count, all_words.len());
        return all_words;
    }
    
    // Use choose_multiple if available, otherwise fall back to manual selection
    let selected: Vec<String> = all_words
        .choose_multiple(rng, count)
        .cloned()
        .collect();
    
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
    eprintln!("  --min-words <N>         Minimum words per sentence (default: 5)");
    eprintln!("  --seed <N>              Seed for deterministic random generation");
    eprintln!("  --verbose, -v           Show detailed debugging information");
    eprintln!("  --help                  Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {} abandon ability able about above absent", program_name);
    eprintln!("  {} --min-words 15 word1 word2 word3", program_name);
    eprintln!("  {} --random 10", program_name);
    eprintln!("  {} --random 5 --min-words 12", program_name);
}

fn parse_args() -> Result<(Vec<String>, usize, Option<usize>, bool, Option<u64>), String> {
    let args: Vec<String> = env::args().collect();
    let program_name = args[0].clone();
    
    if args.len() < 2 {
        return Err("No words provided. Use --random <N> or provide words as arguments.".to_string());
    }
    
    let mut words = Vec::new();
    let mut min_words_per_sentence = 5;
    let mut random_count: Option<usize> = None;
    let mut verbose = false;
    let mut seed: Option<u64> = None;
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
            "--min-words" => {
                if i + 1 >= args.len() {
                    return Err("--min-words requires a value".to_string());
                }
                min_words_per_sentence = args[i + 1].parse()
                    .map_err(|_| format!("Invalid number for --min-words: {}", args[i + 1]))?;
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
    
    Ok((words, min_words_per_sentence, random_count, verbose, seed))
}

// --- CLI usage ---
fn main() {
    let (mut words, min_words_per_sentence, random_count, verbose, seed) = match parse_args() {
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

    let payload_set: HashSet<String> = payload.iter().map(|t| t.word.clone()).collect();

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
    
    // Keep determiners as a small fixed set (function words)
    let det_words = ["the", "a", "an", "these", "those", "each", "some"];
    
    // Convert to slices for the lexicon
    let adj_words_slice: Vec<&str> = adj_words.iter().map(|s| s.as_str()).collect();
    let n_words_slice: Vec<&str> = n_words.iter().map(|s| s.as_str()).collect();
    let v_words_slice: Vec<&str> = v_words.iter().map(|s| s.as_str()).collect();
    let prep_words_slice: Vec<&str> = prep_words.iter().map(|s| s.as_str()).collect();
    let adv_words_slice: Vec<&str> = adv_words.iter().map(|s| s.as_str()).collect();
    
    // Validate all cover words against BIP39 wordlist
    let all_cover_words: Vec<&str> = det_words.iter()
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
    
    let lex = Lexicon::new(payload_set)
        .with_words(Pos::Det, &det_words)
        .with_words(Pos::Adj, &adj_words_slice)
        .with_words(Pos::N, &n_words_slice)
        .with_words(Pos::V, &v_words_slice)
        .with_words(Pos::Prep, &prep_words_slice)
        .with_words(Pos::Adv, &adv_words_slice);

    let (text, payload_set_from_gen) = generate_text(&mut rng, &lex, &payload, min_words_per_sentence, verbose);
    
    // Validate that the generated text contains exactly the input BIP39 words in order
    // Extract BIP39 words from the generated text (handling ANSI color codes)
    let extracted_bip39_words: Vec<String> = {
        // Remove ANSI color codes first
        let text_clean = text
            .replace("\x1b[1;32m", "")
            .replace("\x1b[0m", "");
        
        text_clean
            .split_whitespace()
            .map(|w| {
                // Remove punctuation
                w.trim_end_matches('.')
                 .trim_end_matches(',')
                 .trim_end_matches('!')
                 .trim_end_matches('?')
                 .to_lowercase()
            })
            .filter(|w| !w.is_empty() && payload_set_from_gen.contains(w))
            .collect()
    };
    
    let expected_words: Vec<String> = payload.iter().map(|t| t.word.clone()).collect();
    
    if extracted_bip39_words != expected_words {
        eprintln!("ERROR: Generated BIP39 words do not match input words!");
        eprintln!("Expected: {:?}", expected_words);
        eprintln!("Got:      {:?}", extracted_bip39_words);
        eprintln!();
        eprintln!("Generated text: {}", text);
        panic!("BIP39 word mismatch: generated words do not match input words exactly");
    }
    
    println!("{}", text);

    // Decoding: split on whitespace/punct, keep only tokens that are in the BIP39 set.
    // The payload_set contains all the BIP39 words, so filtering is straightforward.
}

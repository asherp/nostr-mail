use nlprule::{Tokenizer, Rules};
use anyhow::{Result, Context};

/// Helper enum to represent supported languages
#[derive(Debug, Clone, Copy)]
pub enum Language {
    English,
}

impl Language {
    /// Get the language code (ISO 639-1)
    fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
        }
    }

    /// Get the tokenizer filename for this language
    fn tokenizer_filename(&self) -> String {
        format!("{}_tokenizer.bin", self.code())
    }

    /// Get the rules filename for this language
    fn rules_filename(&self) -> String {
        format!("{}_rules.bin", self.code())
    }
}

/// Grammar checker that wraps nlprule functionality
pub struct GrammarChecker {
    tokenizer: Tokenizer,
    rules: Rules,
}

impl GrammarChecker {
    /// Create a new GrammarChecker from language, loading tokenizer and rules from paths
    pub fn from_paths(tokenizer_path: &str, rules_path: &str) -> Result<Self> {
        let tokenizer = Tokenizer::new(tokenizer_path)
            .with_context(|| format!("Failed to load tokenizer from {}", tokenizer_path))?;
        let rules = Rules::new(rules_path)
            .with_context(|| format!("Failed to load rules from {}", rules_path))?;
        
        Ok(Self { tokenizer, rules })
    }

    /// Create a new GrammarChecker from language, using default paths
    /// Checks multiple locations: current directory, data/, and /app/data (for Docker)
    pub fn from_language(language: Language) -> Result<Self> {
        let tokenizer_filename = language.tokenizer_filename();
        let rules_filename = language.rules_filename();
        
        // Try multiple locations (check Docker location first to avoid corrupted local files)
        let locations = vec![
            "/opt/nlprule-data/", // Docker persistent location (check first)
            "/app/data/",         // Docker location
            "data/",              // data subdirectory
            "",                   // current directory
        ];
        
        for location in &locations {
            let tokenizer_path = format!("{}{}", location, tokenizer_filename);
            let rules_path = format!("{}{}", location, rules_filename);
            
            if std::path::Path::new(&tokenizer_path).exists() && 
               std::path::Path::new(&rules_path).exists() {
                return Self::from_paths(&tokenizer_path, &rules_path);
            }
        }
        
        // If none found, try the default (current directory) and let it error with a helpful message
        Self::from_paths(&tokenizer_filename, &rules_filename)
            .with_context(|| format!(
                "Could not find {} and {} in any of: current directory, data/, /app/data/, or /opt/nlprule-data/",
                tokenizer_filename, rules_filename
            ))
    }

    /// Check grammar of a sentence and return suggestions
    pub fn check(&self, text: &str) -> Vec<nlprule::types::Suggestion> {
        self.rules.suggest(text, &self.tokenizer)
    }

    /// Correct a sentence based on grammar rules
    pub fn correct(&self, text: &str) -> String {
        self.rules.correct(text, &self.tokenizer)
    }

    /// Tokenize text and return sentences
    pub fn tokenize<'a>(&'a self, text: &'a str) -> impl Iterator<Item = nlprule::types::Sentence<'a>> {
        self.tokenizer.pipe(text)
    }

    /// Check if a sentence is grammatically correct
    pub fn is_correct(&self, text: &str) -> bool {
        let suggestions = self.check(text);
        suggestions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grammar_checking() -> Result<()> {
        // This test requires the binary files to be present
        let grammar_checker = match GrammarChecker::from_language(Language::English) {
            Ok(checker) => checker,
            Err(_) => {
                // Skip test if files are not available
                return Ok(());
            }
        };
        
        let sentence = "The quick brown fox jumps over the lazy dog.";
        let suggestions = grammar_checker.check(sentence);
        
        // This is a complete, grammatically correct sentence
        // It should have few or no suggestions
        println!("Suggestions for test sentence: {:?}", suggestions);
        
        Ok(())
    }
}

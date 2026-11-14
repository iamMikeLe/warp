//! Replace for global search results.
//!
//! Rather than reusing the byte offsets the search reported (which are display-trimmed and go stale
//! as soon as the file changes), replacing re-runs the search's own regex over each file's current
//! content, line by line like ripgrep. A single-match replace also checks that the match it is
//! about to change is still the text the search showed.

use std::path::Path;

use regex::{Regex, RegexBuilder};

use super::SearchConfig;

/// Finds and replaces matches of one search.
pub struct Replacer {
    regex: Regex,
    replacement: String,
    /// Regex mode: `$1` / `${name}` in the replacement expand to capture groups. Otherwise the
    /// replacement is inserted literally.
    expand_captures: bool,
    /// The pattern spans lines, so it runs over the whole file rather than line by line.
    multiline: bool,
}

/// Which matches of a file to replace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplaceTarget {
    All,
    /// The `occurrence`-th match (0-based) on 1-based line `line_number`, which must still read
    /// `expected`.
    One {
        line_number: u32,
        occurrence: usize,
        expected: String,
    },
}

impl Replacer {
    pub fn new(
        pattern: &str,
        config: SearchConfig,
        replacement: &str,
    ) -> Result<Self, regex::Error> {
        let effective = config.effective_pattern(pattern);
        let regex = RegexBuilder::new(&effective)
            .case_insensitive(!config.use_case_sensitivity)
            .build()?;
        Ok(Self {
            regex,
            replacement: replacement.to_string(),
            expand_captures: config.use_regex,
            multiline: effective.contains('\n'),
        })
    }

    /// Whether single matches can be replaced (multi-line patterns only support replacing all).
    pub fn supports_single_match(&self) -> bool {
        !self.multiline
    }

    /// Applies `target` to `content`. Returns the new content and how many matches were replaced,
    /// or `None` when a single-match target no longer matches.
    pub fn apply(&self, content: &str, target: &ReplaceTarget) -> Option<(String, usize)> {
        match target {
            ReplaceTarget::All => Some(self.replace_all(content)),
            ReplaceTarget::One {
                line_number,
                occurrence,
                expected,
            } => self
                .replace_one(content, *line_number, *occurrence, expected)
                .map(|content| (content, 1)),
        }
    }

    fn replace_all(&self, content: &str) -> (String, usize) {
        if self.multiline {
            return self.replace_in(content);
        }
        let mut out = String::with_capacity(content.len());
        let mut count = 0;
        for line in content.split_inclusive('\n') {
            let (body, terminator) = split_terminator(line);
            let (replaced, n) = self.replace_in(body);
            out.push_str(&replaced);
            out.push_str(terminator);
            count += n;
        }
        (out, count)
    }

    fn replace_in(&self, text: &str) -> (String, usize) {
        let count = self.regex.find_iter(text).count();
        if count == 0 {
            return (text.to_string(), 0);
        }
        let replaced = if self.expand_captures {
            self.regex.replace_all(text, self.replacement.as_str())
        } else {
            self.regex
                .replace_all(text, regex::NoExpand(&self.replacement))
        };
        (replaced.into_owned(), count)
    }

    fn replace_one(
        &self,
        content: &str,
        line_number: u32,
        occurrence: usize,
        expected: &str,
    ) -> Option<String> {
        if self.multiline || line_number == 0 {
            return None;
        }
        let line_index = usize::try_from(line_number - 1).ok()?;
        let mut line_start = 0;
        for (index, line) in content.split_inclusive('\n').enumerate() {
            if index == line_index {
                let (body, _) = split_terminator(line);
                let captures = self.regex.captures_iter(body).nth(occurrence)?;
                let found = captures.get(0)?;
                if found.as_str() != expected {
                    return None;
                }
                let mut replacement = String::new();
                if self.expand_captures {
                    captures.expand(&self.replacement, &mut replacement);
                } else {
                    replacement.push_str(&self.replacement);
                }
                let start = line_start + found.start();
                let end = line_start + found.end();
                let mut out = String::with_capacity(content.len());
                out.push_str(&content[..start]);
                out.push_str(&replacement);
                out.push_str(&content[end..]);
                return Some(out);
            }
            line_start += line.len();
        }
        None
    }
}

/// Splits a line into its text and its terminator (`\r\n`, `\n`, or none).
fn split_terminator(line: &str) -> (&str, &str) {
    if let Some(body) = line.strip_suffix("\r\n") {
        (body, "\r\n")
    } else if let Some(body) = line.strip_suffix('\n') {
        (body, "\n")
    } else {
        (line, "")
    }
}

/// Why a file was left unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// The file has unsaved changes in an open editor.
    UnsavedChanges,
    /// The match changed since the search ran.
    Changed,
    /// The file couldn't be read or written (e.g. not UTF-8, permissions).
    Io(String),
}

/// Applies `target` to the file at `path` and writes it back if anything changed. Returns the
/// number of replaced matches.
pub fn replace_in_file(
    path: &Path,
    replacer: &Replacer,
    target: &ReplaceTarget,
) -> Result<usize, SkipReason> {
    let content = std::fs::read_to_string(path).map_err(|err| SkipReason::Io(err.to_string()))?;
    let (new_content, count) = replacer
        .apply(&content, target)
        .ok_or(SkipReason::Changed)?;
    if count > 0 && new_content != content {
        std::fs::write(path, new_content).map_err(|err| SkipReason::Io(err.to_string()))?;
    }
    Ok(count)
}

#[cfg(test)]
#[path = "replace_tests.rs"]
mod tests;

//! Here Document support.
//!
//! Here Documents allow embedding multi-line text in commands.
//!
//! Syntax:
//!   <<DELIMITER
//!   content
//!   DELIMITER
//!
//!   <<-DELIMITER
//!     content (tabs stripped)
//!   DELIMITER

use crate::ShellError;

/// A Here Document.
#[derive(Debug, Clone)]
pub struct HereDoc {
    /// The delimiter that marks the end of the document
    pub delimiter: String,
    /// The content of the document
    pub content: String,
    /// Whether to strip leading tabs (<<- syntax)
    pub strip_tabs: bool,
}

impl HereDoc {
    /// Create a new Here Document.
    pub fn new(delimiter: &str, content: &str, strip_tabs: bool) -> Self {
        Self {
            delimiter: delimiter.to_string(),
            content: content.to_string(),
            strip_tabs,
        }
    }

    /// Get the processed content (with tabs stripped if needed).
    pub fn processed_content(&self) -> String {
        if self.strip_tabs {
            self.content
                .lines()
                .map(|line| line.trim_start_matches('\t'))
                .collect::<Vec<&str>>()
                .join("\n")
        } else {
            self.content.clone()
        }
    }
}

/// Read a Here Document from input until the delimiter is found.
pub fn read_heredoc(
    delimiter: &str,
    strip_tabs: bool,
    lines: &mut impl Iterator<Item = String>,
) -> Result<HereDoc, ShellError> {
    let mut content = String::new();

    loop {
        match lines.next() {
            Some(line) => {
                // Check if this line is the delimiter
                let check_line = if strip_tabs {
                    line.trim_start_matches('\t').to_string()
                } else {
                    line.clone()
                };

                if check_line == delimiter {
                    return Ok(HereDoc::new(delimiter, &content, strip_tabs));
                }

                // Add the line to content
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&line);
            }
            None => {
                return Err(ShellError::unterminated(
                    format!("here-document (expected '{}')", delimiter),
                    0,
                ));
            }
        }
    }
}

/// Parse Here Documents from a script.
pub fn parse_heredocs(script: &str) -> Result<Vec<HereDoc>, ShellError> {
    let mut heredocs = Vec::new();
    let mut lines = script.lines().map(|s| s.to_string());

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        // Check for << or <<- anywhere in the line
        if let Some(pos) = trimmed.find("<<") {
            let after = &trimmed[pos + 2..];
            let (strip_tabs, delimiter) = if let Some(d) = after.strip_prefix('-') {
                (true, d.trim())
            } else {
                (false, after.trim())
            };

            // Remove quotes from delimiter if present
            let delimiter = if (delimiter.starts_with('"') && delimiter.ends_with('"'))
                || (delimiter.starts_with('\'') && delimiter.ends_with('\''))
            {
                &delimiter[1..delimiter.len() - 1]
            } else {
                delimiter
            };

            if !delimiter.is_empty() {
                let heredoc = read_heredoc(delimiter, strip_tabs, &mut lines)?;
                heredocs.push(heredoc);
            }
        }
    }

    Ok(heredocs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heredoc_basic() {
        let heredoc = HereDoc::new("EOF", "hello\nworld", false);
        assert_eq!(heredoc.processed_content(), "hello\nworld");
    }

    #[test]
    fn test_heredoc_strip_tabs() {
        let heredoc = HereDoc::new("EOF", "\thello\n\t\tworld", true);
        // strip_tabs strips ALL leading tabs from each line
        assert_eq!(heredoc.processed_content(), "hello\nworld");
    }

    #[test]
    fn test_read_heredoc() {
        let input = vec!["hello".to_string(), "world".to_string(), "EOF".to_string()];
        let mut iter = input.into_iter();
        let heredoc = read_heredoc("EOF", false, &mut iter).unwrap();
        assert_eq!(heredoc.content, "hello\nworld");
        assert_eq!(heredoc.delimiter, "EOF");
    }

    #[test]
    fn test_read_heredoc_strip_tabs() {
        let input = vec![
            "\thello".to_string(),
            "\t\tworld".to_string(),
            "EOF".to_string(),
        ];
        let mut iter = input.into_iter();
        let heredoc = read_heredoc("EOF", true, &mut iter).unwrap();
        // Content preserves original, processed_content strips tabs
        assert_eq!(heredoc.content, "\thello\n\t\tworld");
        assert_eq!(heredoc.processed_content(), "hello\nworld");
    }

    #[test]
    fn test_read_heredoc_unterminated() {
        let input = vec!["hello".to_string(), "world".to_string()];
        let mut iter = input.into_iter();
        let result = read_heredoc("EOF", false, &mut iter);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_heredocs() {
        let script = "cat <<EOF\nhello\nworld\nEOF";
        let heredocs = parse_heredocs(script).unwrap();
        assert_eq!(heredocs.len(), 1);
        assert_eq!(heredocs[0].content, "hello\nworld");
        assert_eq!(heredocs[0].delimiter, "EOF");
    }
}

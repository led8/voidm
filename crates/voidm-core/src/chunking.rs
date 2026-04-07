use crate::config::ChunkingConfig;

/// Split `content` into overlapping chunks suitable for embedding.
///
/// Strategy (in priority order):
/// 1. Paragraph boundaries ("\n\n")
/// 2. Sentence boundaries (". " / "! " / "? ")
/// 3. Word boundaries (" ")
/// 4. Hard character split as last resort
///
/// Returns an empty Vec if the content is blank or chunking is disabled.
pub fn chunk_text(content: &str, config: &ChunkingConfig) -> Vec<String> {
    if !config.enabled || content.trim().is_empty() {
        return vec![];
    }

    // Short content: emit as-is (one chunk)
    if content.len() <= config.chunk_max {
        let trimmed = content.trim().to_string();
        if !trimmed.is_empty() {
            return vec![trimmed];
        }
        return vec![];
    }

    // Split into paragraph segments first
    let paragraphs: Vec<&str> = content.split("\n\n").collect();

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for para in paragraphs {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        if current.is_empty() {
            current.push_str(para);
        } else if current.len() + 2 + para.len() <= config.chunk_size {
            current.push_str("\n\n");
            current.push_str(para);
        } else {
            // Flush current, start new
            flush_segment(&mut chunks, &current, config);
            // Apply overlap: carry the tail of the flushed segment
            let overlap_text = overlap_tail(&current, config.chunk_overlap);
            current = if overlap_text.is_empty() {
                para.to_string()
            } else {
                format!("{} {}", overlap_text, para)
            };
        }
    }
    if !current.trim().is_empty() {
        flush_segment(&mut chunks, &current, config);
    }

    // Drop undersized chunks unless it's the only one
    if chunks.len() > 1 {
        chunks.retain(|c| c.len() >= config.chunk_min);
    }

    chunks
}

/// Flush a segment: if it exceeds chunk_max, split at sentence boundaries.
fn flush_segment(chunks: &mut Vec<String>, text: &str, config: &ChunkingConfig) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if text.len() <= config.chunk_max {
        chunks.push(text.to_string());
        return;
    }
    // Need to split at sentence level
    split_by_sentences(chunks, text, config);
}

/// Split text at sentence boundaries (". ", "! ", "? ").
fn split_by_sentences(chunks: &mut Vec<String>, text: &str, config: &ChunkingConfig) {
    // Find sentence boundaries
    let mut boundaries: Vec<usize> = vec![0];
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if (bytes[i] == b'.' || bytes[i] == b'!' || bytes[i] == b'?')
            && i + 1 < len
            && bytes[i + 1] == b' '
        {
            boundaries.push(i + 2);
        }
        i += 1;
    }
    boundaries.push(len);
    boundaries.dedup();

    let mut current = String::new();
    for window in boundaries.windows(2) {
        let seg = text[window[0]..window[1]].trim();
        if seg.is_empty() {
            continue;
        }
        if current.is_empty() {
            current.push_str(seg);
        } else if current.len() + 1 + seg.len() <= config.chunk_size {
            current.push(' ');
            current.push_str(seg);
        } else {
            if !current.trim().is_empty() {
                split_by_words(chunks, current.trim(), config);
            }
            let overlap = overlap_tail(&current, config.chunk_overlap);
            current = if overlap.is_empty() {
                seg.to_string()
            } else {
                format!("{} {}", overlap, seg)
            };
        }
    }
    if !current.trim().is_empty() {
        split_by_words(chunks, current.trim(), config);
    }
}

/// Split text at word boundaries.
fn split_by_words(chunks: &mut Vec<String>, text: &str, config: &ChunkingConfig) {
    if text.len() <= config.chunk_max {
        chunks.push(text.to_string());
        return;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut current = String::new();
    for word in words {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= config.chunk_max {
            current.push(' ');
            current.push_str(word);
        } else {
            if !current.is_empty() {
                chunks.push(current.clone());
            }
            let overlap = overlap_tail(&current, config.chunk_overlap);
            current = if overlap.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", overlap, word)
            };
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
}

/// Return the last `n` characters of `text` at a word boundary for overlap.
fn overlap_tail(text: &str, n: usize) -> String {
    if n == 0 || text.len() <= n {
        return String::new();
    }
    let start = text.len() - n;
    // Advance to a word boundary
    let slice = &text[start..];
    let trimmed = if let Some(pos) = slice.find(' ') {
        &slice[pos + 1..]
    } else {
        slice
    };
    trimmed.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> ChunkingConfig {
        ChunkingConfig::default()
    }

    #[test]
    fn short_content_returns_single_chunk() {
        let config = default_config();
        let content = "A short memory.";
        let chunks = chunk_text(content, &config);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "A short memory.");
    }

    #[test]
    fn empty_content_returns_empty() {
        let config = default_config();
        assert!(chunk_text("", &config).is_empty());
        assert!(chunk_text("   ", &config).is_empty());
    }

    #[test]
    fn disabled_returns_empty() {
        let mut config = default_config();
        config.enabled = false;
        let content = "x".repeat(1000);
        assert!(chunk_text(&content, &config).is_empty());
    }

    #[test]
    fn long_content_produces_multiple_chunks() {
        let mut config = default_config();
        config.chunk_size = 200;
        config.chunk_min = 50;
        config.chunk_max = 300;

        let content = "Alpha beta gamma. ".repeat(50);
        let chunks = chunk_text(&content, &config);
        assert!(chunks.len() > 1, "Expected multiple chunks, got {}", chunks.len());
        for c in &chunks {
            assert!(c.len() <= config.chunk_max + 50, "Chunk too long: {}", c.len());
        }
    }
}

//! File chunking for the vector store.
//!
//! Splits text into overlapping chunks suitable for embedding. Supports
//! automatic file-type detection (PDF, Markdown, code, plain text) and
//! uses type-appropriate splitting strategies:
//!
//! - **PDF** — extract text via `oxidize_pdf`, then chunk as plain text.
//! - **Code** — split on blank-line boundaries (function/struct gaps).
//! - **Markdown / text** — split on paragraph boundaries (double newline).

use std::path::Path;

use crate::error::{Result, SafeAgentError};

// ---------------------------------------------------------------------------
// Chunking parameters
// ---------------------------------------------------------------------------

/// Target chunk size in characters (~512 tokens at ~4 chars/token).
const TARGET_CHUNK_SIZE: usize = 2048;

/// Overlap between consecutive chunks in characters (~50 tokens).
const OVERLAP: usize = 200;

// ---------------------------------------------------------------------------
// Chunk type
// ---------------------------------------------------------------------------

/// A single chunk of text with its positional index within the source.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// The chunk text content.
    pub text: String,
    /// Zero-based index of this chunk within the sequence produced from a
    /// single source.
    pub index: usize,
}

// ---------------------------------------------------------------------------
// File-type detection
// ---------------------------------------------------------------------------

/// Detect the logical file type from a path's extension.
///
/// Returns one of `"pdf"`, `"markdown"`, `"code"`, or `"text"`.
pub fn detect_file_type(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "pdf" => "pdf",
        "md" | "mdx" => "markdown",
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "go" | "c" | "cpp" | "h" | "hpp"
        | "java" | "rb" | "sh" | "toml" | "yaml" | "yml" | "json" | "sql" | "lua" | "zig"
        | "swift" | "kt" => "code",
        _ => "text",
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Read a file, detect its type, and split the content into chunks.
///
/// Returns a tuple of `(chunks, detected_type)` where `detected_type` is one
/// of `"pdf"`, `"markdown"`, `"code"`, or `"text"`.
///
/// Empty files produce an empty `Vec`.
pub fn chunk_file(path: &Path) -> Result<(Vec<Chunk>, &'static str)> {
    let file_type = detect_file_type(path);

    let text = if file_type == "pdf" {
        extract_pdf_text(path)?
    } else {
        std::fs::read_to_string(path).map_err(|e| {
            SafeAgentError::VectorStore(format!("failed to read {}: {e}", path.display()))
        })?
    };

    if text.is_empty() {
        return Ok((Vec::new(), file_type));
    }

    let segments = split_into_segments(&text, file_type);
    let chunks = merge_segments_into_chunks(&segments);

    Ok((chunks, file_type))
}

/// Chunk raw text (e.g. from the `vector_remember` tool).
///
/// Short texts that fit within a single chunk are returned as-is.
pub fn chunk_text(text: &str) -> Vec<Chunk> {
    if text.is_empty() {
        return Vec::new();
    }

    let segments = split_into_segments(text, "text");
    merge_segments_into_chunks(&segments)
}

// ---------------------------------------------------------------------------
// PDF text extraction
// ---------------------------------------------------------------------------

/// Extract all text from a PDF file using `oxidize_pdf`.
fn extract_pdf_text(path: &Path) -> Result<String> {
    use std::io::Cursor;

    let bytes = std::fs::read(path).map_err(|e| {
        SafeAgentError::VectorStore(format!("failed to read PDF {}: {e}", path.display()))
    })?;

    let reader = oxidize_pdf::parser::reader::PdfReader::new(Cursor::new(bytes)).map_err(|e| {
        SafeAgentError::VectorStore(format!("failed to parse PDF {}: {e}", path.display()))
    })?;

    let doc = oxidize_pdf::parser::document::PdfDocument::new(reader);

    let page_count = doc.page_count().map_err(|e| {
        SafeAgentError::VectorStore(format!(
            "failed to get page count for {}: {e}",
            path.display()
        ))
    })?;

    let mut all_text = String::new();
    for i in 0..page_count {
        match doc.extract_text_from_page(i) {
            Ok(extracted) => {
                if !all_text.is_empty() && !extracted.text.is_empty() {
                    all_text.push('\n');
                }
                all_text.push_str(&extracted.text);
            }
            Err(e) => {
                // Log but don't fail on individual page extraction errors —
                // scanned pages or pages with only images will fail here.
                tracing::warn!(page = i, error = %e, "skipping page with extraction error");
            }
        }
    }

    Ok(all_text)
}

// ---------------------------------------------------------------------------
// Segmentation strategies
// ---------------------------------------------------------------------------

/// Split text into logical segments based on file type.
///
/// - **code**: split on blank lines (function / struct boundaries).
/// - **markdown / text**: split on paragraph boundaries (double newline).
fn split_into_segments<'a>(text: &'a str, file_type: &str) -> Vec<&'a str> {
    let separator = match file_type {
        "code" => "\n\n",
        _ => "\n\n", // markdown and text both split on paragraphs
    };

    text.split(separator)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Segment merging with overlap
// ---------------------------------------------------------------------------

/// Merge small segments into chunks of approximately `TARGET_CHUNK_SIZE`
/// characters, with `OVERLAP` characters of overlap between consecutive
/// chunks.
fn merge_segments_into_chunks(segments: &[&str]) -> Vec<Chunk> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current = String::new();
    let mut chunk_index: usize = 0;

    for &segment in segments {
        // If appending this segment would exceed the target and we already
        // have content, flush the current chunk first.
        let would_be = if current.is_empty() {
            segment.len()
        } else {
            current.len() + 2 + segment.len() // +2 for "\n\n" joiner
        };

        if !current.is_empty() && would_be > TARGET_CHUNK_SIZE {
            chunks.push(Chunk {
                text: current.clone(),
                index: chunk_index,
            });
            chunk_index += 1;

            // Start next chunk with overlap from the end of the current one.
            current = overlap_tail(&current);
        }

        if current.is_empty() {
            current.push_str(segment);
        } else {
            current.push_str("\n\n");
            current.push_str(segment);
        }
    }

    // Flush the final chunk.
    if !current.is_empty() {
        chunks.push(Chunk {
            text: current,
            index: chunk_index,
        });
    }

    chunks
}

/// Return up to `OVERLAP` characters from the tail of `text`, breaking at a
/// word boundary so we don't split mid-word.
fn overlap_tail(text: &str) -> String {
    if text.len() <= OVERLAP {
        return text.to_string();
    }

    let start = text.len() - OVERLAP;
    // Find the next space after `start` so we don't split a word.
    let adjusted = text[start..]
        .find(' ')
        .map(|pos| start + pos + 1)
        .unwrap_or(start);

    text[adjusted..].to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // -- detect_file_type ----------------------------------------------------

    #[test]
    fn detect_rs_is_code() {
        assert_eq!(detect_file_type(Path::new("main.rs")), "code");
    }

    #[test]
    fn detect_md_is_markdown() {
        assert_eq!(detect_file_type(Path::new("README.md")), "markdown");
    }

    #[test]
    fn detect_mdx_is_markdown() {
        assert_eq!(detect_file_type(Path::new("page.mdx")), "markdown");
    }

    #[test]
    fn detect_pdf_is_pdf() {
        assert_eq!(detect_file_type(Path::new("doc.pdf")), "pdf");
    }

    #[test]
    fn detect_txt_is_text() {
        assert_eq!(detect_file_type(Path::new("notes.txt")), "text");
    }

    #[test]
    fn detect_no_extension_is_text() {
        assert_eq!(detect_file_type(Path::new("Makefile")), "text");
    }

    #[test]
    fn detect_all_code_extensions() {
        let code_exts = [
            "rs", "py", "js", "ts", "jsx", "tsx", "go", "c", "cpp", "h", "hpp", "java", "rb",
            "sh", "toml", "yaml", "yml", "json", "sql", "lua", "zig", "swift", "kt",
        ];
        for ext in code_exts {
            let path = format!("file.{ext}");
            assert_eq!(
                detect_file_type(Path::new(&path)),
                "code",
                "expected 'code' for .{ext}"
            );
        }
    }

    // -- chunk_text: short text ----------------------------------------------

    #[test]
    fn chunk_text_short_returns_single_chunk() {
        let text = "Hello, world! This is a short piece of text.";
        let chunks = chunk_text(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].text, text);
    }

    #[test]
    fn chunk_text_empty_returns_empty() {
        let chunks = chunk_text("");
        assert!(chunks.is_empty());
    }

    // -- chunk_text: long text -----------------------------------------------

    #[test]
    fn chunk_text_long_produces_multiple_chunks() {
        // Build a text that is clearly longer than TARGET_CHUNK_SIZE.
        let paragraph = "Lorem ipsum dolor sit amet. ".repeat(20); // ~560 chars
        let text = std::iter::repeat(paragraph.as_str())
            .take(10)
            .collect::<Vec<_>>()
            .join("\n\n"); // ~5800 chars across 10 paragraphs

        let chunks = chunk_text(&text);
        assert!(
            chunks.len() > 1,
            "expected multiple chunks, got {}",
            chunks.len()
        );

        // Verify indices are sequential.
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, i);
        }

        // Every chunk should be non-empty.
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
        }
    }

    #[test]
    fn chunk_text_preserves_all_content() {
        // Ensure no content is silently dropped (modulo overlap duplication).
        let paragraphs: Vec<String> = (0..8)
            .map(|i| format!("Paragraph {i}: {}", "word ".repeat(100)))
            .collect();
        let text = paragraphs.join("\n\n");

        let chunks = chunk_text(&text);
        // Every original paragraph should appear in at least one chunk.
        for para in &paragraphs {
            let trimmed = para.trim();
            let found = chunks.iter().any(|c| c.text.contains(trimmed));
            assert!(found, "paragraph not found in any chunk: {trimmed:.60}...");
        }
    }

    // -- chunk_file: text file -----------------------------------------------

    #[test]
    fn chunk_file_reads_text_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("test.txt");
        let mut f = std::fs::File::create(&path).expect("create file");
        writeln!(f, "Hello from a text file.").expect("write");
        drop(f);

        let (chunks, file_type) = chunk_file(&path).expect("chunk_file");
        assert_eq!(file_type, "text");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("Hello from a text file"));
    }

    // -- chunk_file: empty file ----------------------------------------------

    #[test]
    fn chunk_file_empty_returns_empty_vec() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("empty.txt");
        std::fs::File::create(&path).expect("create file");

        let (chunks, file_type) = chunk_file(&path).expect("chunk_file");
        assert_eq!(file_type, "text");
        assert!(chunks.is_empty());
    }

    // -- chunk_file: code file -----------------------------------------------

    #[test]
    fn chunk_file_code_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("example.rs");
        let code = r#"fn main() {
    println!("hello");
}

fn helper() {
    // does stuff
}

struct Foo {
    bar: i32,
}"#;
        std::fs::write(&path, code).expect("write");

        let (chunks, file_type) = chunk_file(&path).expect("chunk_file");
        assert_eq!(file_type, "code");
        assert!(!chunks.is_empty());
        // All three items should be present across chunks.
        let all_text: String = chunks.iter().map(|c| c.text.clone()).collect::<Vec<_>>().join(" ");
        assert!(all_text.contains("fn main()"));
        assert!(all_text.contains("fn helper()"));
        assert!(all_text.contains("struct Foo"));
    }

    // -- overlap_tail --------------------------------------------------------

    #[test]
    fn overlap_tail_short_text_returns_full() {
        let text = "short";
        assert_eq!(overlap_tail(text), "short");
    }

    #[test]
    fn overlap_tail_respects_word_boundary() {
        // "word1 word2 word3 ..." — 400 chars total.
        let text = "a ".repeat(200);
        let tail = overlap_tail(&text);
        // The tail should be roughly OVERLAP chars (give or take a word).
        assert!(tail.len() <= OVERLAP + 5, "tail too long: {}", tail.len());
        assert!(tail.len() >= OVERLAP - 5, "tail too short: {}", tail.len());
        // The function advances past the next space so the tail starts
        // cleanly at a word character, not mid-word.
        assert!(
            !tail.starts_with(' '),
            "tail should not start with a space"
        );
    }
}

use tracing::warn;

/// PII / sensitive data detector.
///
/// Scans text for common patterns of personally identifiable information,
/// passwords, API keys, and secrets. Returns a list of detections with
/// their categories and approximate positions.
pub struct PiiScanner {
    enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PiiDetection {
    /// Category of sensitive data found.
    pub category: PiiCategory,
    /// Brief description of what was found.
    pub description: String,
    /// Approximate character offset in the text.
    pub offset: usize,
    /// The matched text (redacted for display).
    pub redacted_match: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PiiCategory {
    Ssn,
    CreditCard,
    ApiKey,
    PrivateKey,
    Password,
    JwtToken,
    AwsKey,
}

impl std::fmt::Display for PiiCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PiiCategory::Ssn => write!(f, "SSN"),
            PiiCategory::CreditCard => write!(f, "credit card"),
            PiiCategory::ApiKey => write!(f, "API key"),
            PiiCategory::PrivateKey => write!(f, "private key"),
            PiiCategory::Password => write!(f, "password"),
            PiiCategory::JwtToken => write!(f, "JWT token"),
            PiiCategory::AwsKey => write!(f, "AWS access key"),
        }
    }
}

impl PiiScanner {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Scan text for sensitive data patterns. Returns all detections found.
    pub fn scan(&self, text: &str) -> Vec<PiiDetection> {
        if !self.enabled || text.is_empty() {
            return Vec::new();
        }

        let mut detections = Vec::new();

        // SSN: ###-##-####
        scan_pattern(text, &mut detections, is_ssn, PiiCategory::Ssn, "SSN pattern");

        // Credit card: 13-19 digit sequences (with optional dashes/spaces)
        scan_pattern(text, &mut detections, is_credit_card, PiiCategory::CreditCard, "credit card number");

        // API keys: long alphanumeric strings starting with common prefixes
        scan_pattern(text, &mut detections, is_api_key, PiiCategory::ApiKey, "API key");

        // AWS access keys: AKIA followed by 16 alphanumeric chars
        scan_pattern(text, &mut detections, is_aws_key, PiiCategory::AwsKey, "AWS access key");

        // Private keys: -----BEGIN (RSA|EC|PRIVATE|OPENSSH) KEY-----
        scan_pattern(text, &mut detections, is_private_key, PiiCategory::PrivateKey, "private key");

        // JWT tokens: eyJ... three base64 segments
        scan_pattern(text, &mut detections, is_jwt, PiiCategory::JwtToken, "JWT token");

        // Password patterns: password=, passwd:, etc. followed by non-whitespace
        scan_pattern(text, &mut detections, is_password_pattern, PiiCategory::Password, "password value");

        if !detections.is_empty() {
            warn!(
                count = detections.len(),
                categories = %detections.iter().map(|d| d.category.to_string()).collect::<Vec<_>>().join(", "),
                "PII/sensitive data detected"
            );
        }

        detections
    }

}

/// Scan text for a specific pattern type and add detections.
fn scan_pattern(
    text: &str,
    detections: &mut Vec<PiiDetection>,
    detector: fn(&str, usize) -> Option<(usize, usize)>,
    category: PiiCategory,
    description: &str,
) {
    let mut start = 0;
    while start < text.len() {
        if let Some((offset, end)) = detector(text, start) {
            let matched = &text[offset..end];
            let redacted = redact_match(matched);
            detections.push(PiiDetection {
                category: category.clone(),
                description: description.to_string(),
                offset,
                redacted_match: redacted,
            });
            start = end;
        } else {
            break;
        }
    }
}

fn redact_match(s: &str) -> String {
    if s.len() <= 4 {
        return "*".repeat(s.len());
    }
    let visible = s.len().min(4);
    format!("{}…{}", &s[..2], &s[s.len() - visible.min(2)..])
}

// -- Pattern detectors -------------------------------------------------------
// Each returns Some((start, end)) of the match, or None.

fn is_ssn(text: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i + 10 < bytes.len() {
        if i + 10 < bytes.len()
            && bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3] == b'-'
            && bytes[i + 4].is_ascii_digit()
            && bytes[i + 5].is_ascii_digit()
            && bytes[i + 6] == b'-'
            && bytes[i + 7].is_ascii_digit()
            && bytes[i + 8].is_ascii_digit()
            && bytes[i + 9].is_ascii_digit()
            && bytes[i + 10].is_ascii_digit()
        {
            // Not preceded or followed by a digit (avoid version numbers like 1.2.3)
            let preceded_by_digit = i > 0 && bytes[i - 1].is_ascii_digit();
            let followed_by_digit = i + 11 < bytes.len() && bytes[i + 11].is_ascii_digit();
            if !preceded_by_digit && !followed_by_digit {
                return Some((i, i + 11));
            }
        }
        i += 1;
    }
    None
}

fn is_credit_card(text: &str, from: usize) -> Option<(usize, usize)> {
    // Look for 4 groups of 4 digits separated by spaces or dashes
    let bytes = text.as_bytes();
    let mut i = from;
    while i + 18 < bytes.len() {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
            && (bytes[i + 4] == b' ' || bytes[i + 4] == b'-')
            && bytes[i + 5].is_ascii_digit()
            && bytes[i + 6].is_ascii_digit()
            && bytes[i + 7].is_ascii_digit()
            && bytes[i + 8].is_ascii_digit()
            && (bytes[i + 9] == b' ' || bytes[i + 9] == b'-')
            && bytes[i + 10].is_ascii_digit()
            && bytes[i + 11].is_ascii_digit()
            && bytes[i + 12].is_ascii_digit()
            && bytes[i + 13].is_ascii_digit()
            && (bytes[i + 14] == b' ' || bytes[i + 14] == b'-')
            && bytes[i + 15].is_ascii_digit()
            && bytes[i + 16].is_ascii_digit()
            && bytes[i + 17].is_ascii_digit()
            && bytes[i + 18].is_ascii_digit()
        {
            return Some((i, i + 19));
        }
        i += 1;
    }
    None
}

fn is_api_key(text: &str, from: usize) -> Option<(usize, usize)> {
    let prefixes = ["sk-", "pk-", "api-", "key-", "sk_live_", "sk_test_", "rk-", "xoxb-", "xoxp-"];
    for prefix in &prefixes {
        if let Some(pos) = text[from..].find(prefix) {
            let start = from + pos;
            let rest = &text[start + prefix.len()..];
            let key_len = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .count();
            if key_len >= 16 {
                return Some((start, start + prefix.len() + key_len));
            }
        }
    }
    None
}

fn is_aws_key(text: &str, from: usize) -> Option<(usize, usize)> {
    if let Some(pos) = text[from..].find("AKIA") {
        let start = from + pos;
        if start + 20 <= text.len() {
            let candidate = &text[start..start + 20];
            if candidate.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Some((start, start + 20));
            }
        }
    }
    None
}

fn is_private_key(text: &str, from: usize) -> Option<(usize, usize)> {
    let markers = [
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
    ];
    for marker in &markers {
        if let Some(pos) = text[from..].find(marker) {
            let start = from + pos;
            // Find the end marker
            let end_marker = marker.replace("BEGIN", "END");
            if let Some(end_pos) = text[start..].find(&end_marker) {
                return Some((start, start + end_pos + end_marker.len()));
            }
            // If no end marker, flag the beginning
            return Some((start, start + marker.len()));
        }
    }
    None
}

fn is_jwt(text: &str, from: usize) -> Option<(usize, usize)> {
    if let Some(pos) = text[from..].find("eyJ") {
        let start = from + pos;
        let rest = &text[start..];
        // JWT has 3 base64 segments separated by dots
        let token_len = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
            .count();
        let candidate = &text[start..start + token_len];
        let segments: Vec<&str> = candidate.split('.').collect();
        if segments.len() == 3 && segments.iter().all(|s| s.len() >= 4) {
            return Some((start, start + token_len));
        }
    }
    None
}

fn is_password_pattern(text: &str, from: usize) -> Option<(usize, usize)> {
    let lower = text[from..].to_lowercase();
    let patterns = ["password=", "password:", "passwd=", "passwd:", "pass=", "secret="];
    for pattern in &patterns {
        if let Some(pos) = lower.find(pattern) {
            let value_start = from + pos + pattern.len();
            let rest = &text[value_start..];
            // Take the non-whitespace value after the pattern
            let value_len = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'' && *c != ',')
                .count();
            if value_len >= 4 {
                return Some((from + pos, value_start + value_len));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_scanner() {
        let scanner = PiiScanner::new(false);
        assert!(scanner.scan("123-45-6789").is_empty());
    }

    #[test]
    fn test_ssn_detection() {
        let scanner = PiiScanner::new(true);
        let detections = scanner.scan("My SSN is 123-45-6789 please help");
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].category, PiiCategory::Ssn);
    }

    #[test]
    fn test_credit_card_detection() {
        let scanner = PiiScanner::new(true);
        let detections = scanner.scan("Card: 4111-1111-1111-1111 ok");
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].category, PiiCategory::CreditCard);

        let detections2 = scanner.scan("Card: 4111 1111 1111 1111 ok");
        assert_eq!(detections2.len(), 1);
    }

    #[test]
    fn test_api_key_detection() {
        let scanner = PiiScanner::new(true);
        let detections = scanner.scan("Use key sk-abc123def456ghi789jkl012mno345pq");
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].category, PiiCategory::ApiKey);
    }

    #[test]
    fn test_aws_key_detection() {
        let scanner = PiiScanner::new(true);
        let detections = scanner.scan("Access key: AKIAIOSFODNN7EXAMPLE");
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].category, PiiCategory::AwsKey);
    }

    #[test]
    fn test_private_key_detection() {
        let scanner = PiiScanner::new(true);
        let text = "Here is a key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpA...\n-----END RSA PRIVATE KEY-----\n";
        let detections = scanner.scan(text);
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].category, PiiCategory::PrivateKey);
    }

    #[test]
    fn test_jwt_detection() {
        let scanner = PiiScanner::new(true);
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4iLCJpYXQiOjE1MTYyMzkwMjJ9.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let detections = scanner.scan(&format!("Token: {token}"));
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].category, PiiCategory::JwtToken);
    }

    #[test]
    fn test_password_detection() {
        let scanner = PiiScanner::new(true);
        let detections = scanner.scan("password=supersecret123 next line");
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].category, PiiCategory::Password);
    }

    #[test]
    fn test_no_false_positives_clean_text() {
        let scanner = PiiScanner::new(true);
        let detections = scanner.scan("Hello, this is a normal message about coding.");
        assert!(detections.is_empty());
    }

    #[test]
    fn test_empty_text() {
        let scanner = PiiScanner::new(true);
        assert!(scanner.scan("").is_empty());
    }

    #[test]
    fn test_pii_category_display() {
        assert_eq!(PiiCategory::Ssn.to_string(), "SSN");
        assert_eq!(PiiCategory::CreditCard.to_string(), "credit card");
    }
}

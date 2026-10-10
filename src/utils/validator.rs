use url::Url;

pub fn is_valid_url(url: &str) -> bool {
    if let Ok(parsed) = Url::parse(url) {
        parsed.scheme() == "http" || parsed.scheme() == "https"
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_urls() {
        assert!(is_valid_url("https://youtube.com"));
        assert!(is_valid_url("http://example.com"));
    }

    #[test]
    fn test_invalid_urls() {
        assert!(!is_valid_url("htps://wrong.com"));
        assert!(!is_valid_url("random_string"));
        assert!(!is_valid_url("www.facebook.com"));
    }
}

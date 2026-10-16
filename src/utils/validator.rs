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
        assert!(is_valid_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=RDdQw4w9WgXcQ&index=1"
        ));
        assert!(is_valid_url("http://localhost:8080/stream"));
        assert!(is_valid_url("https://127.0.0.1:3000/video.mp4"));
        assert!(is_valid_url("HTTP://example.com"));
        assert!(is_valid_url("Https://YouTube.com/watch?v=123"));
    }

    #[test]
    fn test_invalid_urls() {
        assert!(!is_valid_url("htps://wrong.com"));
        assert!(!is_valid_url("random_string"));
        assert!(!is_valid_url("www.facebook.com"));
        assert!(!is_valid_url("ftp://example.com/file.mp4"));
        assert!(!is_valid_url("file:///home/user/video.mp4"));
        assert!(!is_valid_url("data:text/html;base64,AAAA"));
        assert!(!is_valid_url(""));
        assert!(!is_valid_url("   "));
        assert!(!is_valid_url("https://"));
        assert!(!is_valid_url("http://"));
    }
}

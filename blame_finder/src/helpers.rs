/// Helper function to extract owner/repo segments from a repository URL
pub fn extract_path_segments(url: &str, host: &str) -> String {
    if let Some(host_pos) = url.find(host) {
        let path_start = host_pos + host.len() + 1; // +1 for the trailing slash

        // Extract everything after the host
        if path_start < url.len() {
            let path = &url[path_start..];

            // Remove trailing slashes, .git extension, or query parameters
            let clean_path = path.trim_end_matches('/').trim_end_matches(".git");

            if let Some(query_pos) = clean_path.find('?') {
                return clean_path[..query_pos].to_string();
            }

            // Find first two path segments (owner/repo)
            let segments: Vec<&str> = clean_path.splitn(3, '/').collect();
            if segments.len() >= 2 {
                return format!("{}/{}", segments[0], segments[1]);
            }

            return clean_path.to_string();
        }
    }

    // Fallback
    url.to_string()
}

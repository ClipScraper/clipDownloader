use std::path::Path;

/// Extract Instagram (handle, id) from /reel/… or /p/…
pub fn ig_handle_and_id(url: &str) -> (Option<String>, Option<String>) {
    if let Some(pos) = url.find("instagram.com/") {
        let rest = &url[pos + "instagram.com/".len()..];
        let parts: Vec<&str> = rest.trim_matches('/').split('/').collect();
        if parts.len() >= 3 {
            let handle = parts[0].to_string();
            let typ = parts[1];
            let id = parts[2].to_string();
            if typ == "reel" || typ == "p" {
                return (Some(handle), Some(id));
            }
        }
    }
    (None, None)
}

/// Extract TikTok id token after /video/ or /photo/
pub fn tiktok_id_from_url(url: &str) -> Option<String> {
    for key in ["/video/", "/photo/"] {
        if let Some(idx) = url.find(key) {
            let tail = &url[idx + key.len()..];
            let id = tail.split(['/', '?', '&']).next().unwrap_or("").to_string();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    None
}

/// Extract YouTube video id from v=… or /shorts/…
pub fn youtube_id_from_url(url: &str) -> Option<String> {
    if let Some(qidx) = url.find('?') {
        for pair in url[qidx + 1..].split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                if k == "v" && !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    if let Some(idx) = url.find("/shorts/") {
        let tail = &url[idx + "/shorts/".len()..];
        let id = tail.split(['/', '?', '&']).next().unwrap_or("").to_string();
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}

/// Fallback last path segment without trailing slash/query
pub fn last_segment(url: &str) -> Option<String> {
    let base = url.split('?').next().unwrap_or(url).trim_end_matches('/');
    base.rsplit('/')
        .next()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// Parse multiple user_handle, clean_name, and file_path from tool output
/// Returns Vec<(user_handle, clean_name, full_file_path)>
pub fn parse_multiple_filenames_from_output(
    output: &str,
    processed_url: &str,
    yt_out_dir_hint: Option<&Path>,
) -> Vec<(String, String, String)> {
    use std::collections::HashSet;
    use std::path::Path as StdPath;

    let mut results = Vec::new();
    let mut candidate_paths: Vec<String> = Vec::new();

    let dir_hint_str = yt_out_dir_hint
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    for line in output.lines() {
        let trimmed = line.trim();

        // gallery-dl "# /abs/path/file.ext"
        if trimmed.starts_with('#') && trimmed.len() > 2 {
            candidate_paths.push(trimmed[2..].trim().to_string());
            continue;
        }
        // yt-dlp destination lines
        if let Some(idx) = trimmed.find("Destination: ") {
            candidate_paths.push(trimmed[idx + "Destination: ".len()..].trim().to_string());
            continue;
        }
        // yt-dlp merging line
        if trimmed.contains("Merging formats into") {
            if let Some(q1) = trimmed.find('"') {
                if let Some(q2) = trimmed[q1 + 1..].find('"') {
                    candidate_paths.push(trimmed[q1 + 1..q1 + 1 + q2].to_string());
                    continue;
                }
            }
            if let Some(after) = trimmed.split("into").nth(1) {
                candidate_paths.push(after.trim_matches(|c| c == '"' || c == ' ').to_string());
                continue;
            }
        }
        // explicit printed paths or joined from hint on "Skipping …"
        let looks_abs_unix = trimmed.starts_with('/');
        let looks_abs_win = trimmed.len() > 2
            && trimmed.as_bytes()[1] == b':'
            && (trimmed.as_bytes()[2] == b'\\' || trimmed.as_bytes()[2] == b'/');
        if looks_abs_unix
            || looks_abs_win
            || (!dir_hint_str.is_empty() && trimmed.starts_with(&dir_hint_str))
        {
            if trimmed.contains('.') {
                candidate_paths.push(trimmed.to_string());
                continue;
            }
        }
        if trimmed.starts_with("[download] ") && trimmed.contains(" has already been downloaded") {
            let name_part = trimmed
                .trim_start_matches("[download] ")
                .split(" has already been downloaded")
                .next()
                .unwrap_or("")
                .trim();
            if !name_part.is_empty() && !dir_hint_str.is_empty() {
                candidate_paths.push(format!("{}/{}", dir_hint_str, name_part));
            }
            continue;
        }
        if trimmed.starts_with("[download] Skipping")
            && trimmed.contains("has already been recorded in the archive")
        {
            if let Some(after) = trimmed.strip_prefix("[download] Skipping ") {
                let fname = after.split(':').next().unwrap_or("").trim();
                if !fname.is_empty() && !dir_hint_str.is_empty() {
                    candidate_paths.push(format!("{}/{}", dir_hint_str, fname));
                }
            }
            continue;
        }
    }

    // dedup
    let mut seen = HashSet::new();
    let mut unique_paths = Vec::new();
    for p in candidate_paths.into_iter() {
        if seen.insert(p.clone()) {
            unique_paths.push(p);
        }
    }

    for full_path in unique_paths.into_iter() {
        let full = StdPath::new(&full_path);
        let filename = full.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let mut clean_name = filename.to_string();
        let mut user_handle = "Unknown".to_string();

        // try "uploader - title [id].ext"
        if let Some(stem) = full.file_stem().and_then(|s| s.to_str()) {
            let s = stem.to_string();
            if let Some(bracket_start) = s.find('[') {
                let before_bracket = s[..bracket_start].trim();
                if let Some(dash_pos) = before_bracket.find(" - ") {
                    clean_name = before_bracket[dash_pos + 3..].trim().to_string();
                } else {
                    clean_name = before_bracket.to_string();
                }
            } else {
                clean_name = s;
            }
        }

        // prefer IG handle from URL; else try parent folders
        if processed_url.contains("instagram.com/") {
            if let (Some(h), _) = ig_handle_and_id(processed_url) {
                user_handle = h;
            }
        }
        if user_handle == "Unknown" {
            if let Some(parent) = full.parent() {
                if let Some(last) = parent.file_name().and_then(|s| s.to_str()) {
                    if last != "instagram" && last != "tiktok" && last != "youtube" {
                        user_handle = last.to_string();
                    } else if let Some(pp) = parent.parent() {
                        if let Some(prev) = pp.file_name().and_then(|s| s.to_str()) {
                            if prev != "instagram" && prev != "tiktok" && prev != "youtube" {
                                user_handle = prev.to_string();
                            }
                        }
                    }
                }
            }
        }

        // Force IG name = ID
        if processed_url.contains("instagram.com/") {
            if let (_, Some(id)) = ig_handle_and_id(processed_url) {
                clean_name = id;
            }
        }

        results.push((user_handle, clean_name, full_path));
    }

    if results.is_empty() {
        let (h, maybe_id) = ig_handle_and_id(processed_url);
        let handle = h.unwrap_or_else(|| "Unknown".to_string());
        let name = if processed_url.contains("instagram.com/") {
            maybe_id.unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Unknown".to_string()
        };
        results.push((handle, name, String::new()));
    }

    results
}

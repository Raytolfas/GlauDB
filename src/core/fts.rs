use std::collections::HashMap;

pub fn tokenize(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let chars: Vec<char> = normalized.chars().collect();
    let mut tokens = Vec::new();

    for word in normalized.split_whitespace() {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric());
        if !w.is_empty() {
            tokens.push(w.to_string());
        }
    }

    if chars.len() >= 3 {
        for i in 0..chars.len().saturating_sub(2) {
            let trigram: String = chars[i..i + 3].iter().collect();
            if trigram.chars().all(|c| c.is_alphanumeric() || c == ' ') {
                tokens.push(trigram);
            }
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

pub fn score(field_value: &str, query: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let field_lower = field_value.to_lowercase();

    if field_lower == query_lower {
        return 1.0;
    }

    if field_lower.contains(&query_lower) {
        return 0.9;
    }

    if field_lower.starts_with(&query_lower) {
        return 0.85;
    }

    let query_tokens = tokenize(query);
    let field_tokens = tokenize(field_value);

    let query_set: std::collections::HashSet<&String> = query_tokens.iter().collect();
    let field_set: std::collections::HashSet<&String> = field_tokens.iter().collect();

    let intersection = query_set.intersection(&field_set).count();
    let union = query_set.union(&field_set).count();

    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

pub fn search_docs(
    docs: &[HashMap<String, serde_json::Value>],
    query: &str,
    fields: &[String],
    min_score: f64,
) -> Vec<(f64, HashMap<String, serde_json::Value>)> {
    if query.trim().is_empty() {
        return docs.iter().map(|d| (1.0, d.clone())).collect();
    }

    let mut scored: Vec<(f64, HashMap<String, serde_json::Value>)> = docs.iter()
        .filter_map(|doc| {
            let doc_score: f64 = fields.iter()
                .filter_map(|field| {
                    doc.get(field).and_then(|v| v.as_str()).map(|text| score(text, query))
                })
                .fold(0.0_f64, f64::max);

            if doc_score >= min_score {
                Some((doc_score, doc.clone()))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

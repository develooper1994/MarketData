use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchType {
    Exact,
    Prefix,
    Suffix,
    Substring,
    Interleaved,
}

#[derive(Debug, Clone)]
pub struct CandidateMatch {
    pub candidate: String,
    pub match_type: MatchType,
    pub score: u32,
}

/// Normalize a query or candidate: keep only alphanumeric characters and uppercase.
pub fn normalize_symbol(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

fn build_interleaved_regex(q: &str) -> Option<Regex> {
    if q.is_empty() {
        return None;
    }
    let parts: Vec<String> = q.chars().map(|c| regex::escape(&c.to_string())).collect();
    let pattern = format!("(?i).*{}.*", parts.join(".*"));
    Regex::new(&pattern).ok()
}

/// Rank candidates for the given query. Returns matches ordered by descending score.
pub fn rank_matches(query: &str, candidates: &[String]) -> Vec<CandidateMatch> {
    let qn = normalize_symbol(query);
    let interleaved_re = build_interleaved_regex(&qn);
    let mut out: Vec<CandidateMatch> = Vec::new();

    for cand in candidates.iter() {
        let cn = normalize_symbol(cand);
        let mut base_score: u32 = 0;
        let mut mtype: Option<MatchType> = None;

        if cn == qn {
            base_score = 100;
            mtype = Some(MatchType::Exact);
        } else if cn.starts_with(&qn) {
            base_score = 80;
            mtype = Some(MatchType::Prefix);
        } else if cn.ends_with(&qn) {
            base_score = 70;
            mtype = Some(MatchType::Suffix);
        } else if cn.contains(&qn) {
            base_score = 60;
            mtype = Some(MatchType::Substring);
        } else if let Some(re) = &interleaved_re {
            if re.is_match(cand) || re.is_match(&cn) {
                base_score = 40;
                mtype = Some(MatchType::Interleaved);
            }
        }

        if let Some(mt) = mtype {
            let bonus = 100u32.saturating_sub(cand.len() as u32);
            let score = base_score * 10 + bonus; // emphasize match type, add small length bonus
            out.push(CandidateMatch {
                candidate: cand.clone(),
                match_type: mt,
                score,
            });
        }
    }

    // Sort by score desc, then shorter candidate first, then lexicographically
    out.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.candidate.len().cmp(&b.candidate.len()))
            .then(a.candidate.cmp(&b.candidate))
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eur_ordering_example() {
        let candidates = vec![
            "EUR".to_string(),
            "EURQWE".to_string(),
            "EURASD".to_string(),
            "ASDEUR".to_string(),
            "ZXCEUR".to_string(),
            "QWEEUR".to_string(),
            "EAXU12R".to_string(),
            "XEURY".to_string(),
            "AXBYC".to_string(),
        ];

        let ranked = rank_matches("EUR", &candidates);

        // first should be exact EUR
        assert_eq!(ranked[0].candidate, "EUR");

        // ensure there is at least one prefix before a suffix
        let pos_prefix = ranked
            .iter()
            .position(|r| r.match_type == MatchType::Prefix)
            .expect("expected a prefix match");
        let pos_suffix = ranked
            .iter()
            .position(|r| r.match_type == MatchType::Suffix)
            .expect("expected a suffix match");
        assert!(pos_prefix < pos_suffix, "prefix should rank before suffix");

        // interleaved should appear after suffix/substring
        let pos_inter = ranked
            .iter()
            .position(|r| r.match_type == MatchType::Interleaved)
            .expect("expected interleaved match");
        assert!(
            pos_inter > pos_suffix,
            "interleaved should be lower priority"
        );
    }
}

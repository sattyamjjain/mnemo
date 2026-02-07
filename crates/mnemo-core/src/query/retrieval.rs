use std::collections::HashMap;
use uuid::Uuid;

/// Weighted Reciprocal Rank Fusion: combines multiple ranked lists with per-list weights.
/// Each item's score = sum over all lists of weights[i] / (k + rank_in_list + 1.0).
/// If weights is empty or shorter than ranked_lists, uses 1.0 as default weight.
pub fn weighted_reciprocal_rank_fusion(
    ranked_lists: &[Vec<(Uuid, f32)>],
    k: f32,
    weights: &[f32],
) -> Vec<(Uuid, f32)> {
    let mut scores: HashMap<Uuid, f32> = HashMap::new();
    for (i, list) in ranked_lists.iter().enumerate() {
        let w = weights.get(i).copied().unwrap_or(1.0);
        for (rank, (id, _original_score)) in list.iter().enumerate() {
            *scores.entry(*id).or_insert(0.0) += w / (k + rank as f32 + 1.0);
        }
    }
    let mut fused: Vec<(Uuid, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

/// Reciprocal Rank Fusion: combines multiple ranked lists into a single fused ranking.
/// Each item's score = sum over all lists of 1/(k + rank_in_list).
/// Items not present in a list are not penalized.
pub fn reciprocal_rank_fusion(
    ranked_lists: &[Vec<(Uuid, f32)>],
    k: f32,
) -> Vec<(Uuid, f32)> {
    weighted_reciprocal_rank_fusion(ranked_lists, k, &[])
}

/// Compute a recency score using exponential decay.
/// Returns a value in [0, 1] where 1 = just created, 0 = very old.
/// half_life_hours controls decay rate (e.g., 168 = 1 week half-life).
pub fn recency_score(created_at: &str, half_life_hours: f64) -> f32 {
    let now = chrono::Utc::now();
    let created = match chrono::DateTime::parse_from_rfc3339(created_at) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => return 0.5, // fallback for unparseable dates
    };
    let age_hours = (now - created).num_seconds() as f64 / 3600.0;
    if age_hours < 0.0 {
        return 1.0; // future timestamp
    }
    let decay = (-age_hours * (2.0_f64.ln()) / half_life_hours).exp();
    decay as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_basic() {
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();
        let id3 = Uuid::now_v7();

        let list1 = vec![(id1, 0.9), (id2, 0.8), (id3, 0.7)];
        let list2 = vec![(id2, 0.95), (id1, 0.85), (id3, 0.75)];

        let fused = reciprocal_rank_fusion(&[list1, list2], 60.0);
        assert_eq!(fused.len(), 3);
        // id1 and id2 should be top since they appear in both lists
        // id1: 1/(60+1) + 1/(60+2) = ~0.0164 + ~0.0161 = ~0.0325
        // id2: 1/(60+2) + 1/(60+1) = ~0.0161 + ~0.0164 = ~0.0325
        // They should have equal scores since they swap ranks
        assert!(fused[0].1 > 0.0);
    }

    #[test]
    fn test_rrf_disjoint() {
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();

        let list1 = vec![(id1, 0.9)];
        let list2 = vec![(id2, 0.8)];

        let fused = reciprocal_rank_fusion(&[list1, list2], 60.0);
        assert_eq!(fused.len(), 2);
        // Both get same score: 1/(60+1)
        assert!((fused[0].1 - fused[1].1).abs() < 0.0001);
    }

    #[test]
    fn test_rrf_single_list() {
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();

        let list1 = vec![(id1, 0.9), (id2, 0.8)];
        let fused = reciprocal_rank_fusion(&[list1], 60.0);
        assert_eq!(fused.len(), 2);
        assert!(fused[0].1 > fused[1].1);
    }

    #[test]
    fn test_weighted_rrf() {
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();

        let list1 = vec![(id1, 0.9), (id2, 0.8)];
        let list2 = vec![(id2, 0.95), (id1, 0.85)];

        // With weight [2.0, 1.0], list1 should have more influence
        let fused = weighted_reciprocal_rank_fusion(&[list1.clone(), list2.clone()], 60.0, &[2.0, 1.0]);
        assert_eq!(fused.len(), 2);
        // id1 is rank 0 in list1 (weight 2.0) and rank 1 in list2 (weight 1.0)
        // id1 score = 2.0/(60+1) + 1.0/(60+2) = ~0.0328 + ~0.0161 = ~0.0489
        // id2 is rank 1 in list1 (weight 2.0) and rank 0 in list2 (weight 1.0)
        // id2 score = 2.0/(60+2) + 1.0/(60+1) = ~0.0323 + ~0.0164 = ~0.0487
        // id1 should score slightly higher
        assert_eq!(fused[0].0, id1);
    }

    #[test]
    fn test_recency_score() {
        // Just created
        let now = chrono::Utc::now().to_rfc3339();
        let score = recency_score(&now, 168.0);
        assert!(score > 0.99);

        // Very old
        let old = (chrono::Utc::now() - chrono::Duration::days(365)).to_rfc3339();
        let score = recency_score(&old, 168.0);
        assert!(score < 0.01);

        // One week ago (half-life = 168 hours)
        let week_ago = (chrono::Utc::now() - chrono::Duration::hours(168)).to_rfc3339();
        let score = recency_score(&week_ago, 168.0);
        assert!((score - 0.5).abs() < 0.05);
    }
}

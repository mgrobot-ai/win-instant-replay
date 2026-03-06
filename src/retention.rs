use std::cmp::Ordering;

pub fn keep_segment_count(max_replay_seconds: u32, segment_seconds: u32) -> usize {
    let segment_seconds = segment_seconds.max(1);
    max_replay_seconds.div_ceil(segment_seconds) as usize + 2
}

pub fn files_to_delete<T: Ord + Clone>(mut items: Vec<T>, keep: usize) -> Vec<T> {
    if items.len() <= keep {
        return Vec::new();
    }

    items.sort();
    let delete_count = items.len() - keep;
    items.into_iter().take(delete_count).collect()
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SegmentDescriptor {
    pub sort_key: String,
}

impl Ord for SegmentDescriptor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key.cmp(&other.sort_key)
    }
}

impl PartialOrd for SegmentDescriptor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::{files_to_delete, keep_segment_count};

    #[test]
    fn keep_count_rounds_up_and_adds_small_safety_margin() {
        assert_eq!(keep_segment_count(300, 1), 302);
        assert_eq!(keep_segment_count(300, 2), 152);
        assert_eq!(keep_segment_count(301, 2), 153);
    }

    #[test]
    fn deletes_oldest_entries_when_over_limit() {
        let doomed = files_to_delete(vec![5, 2, 9, 1, 3], 2);
        assert_eq!(doomed, vec![1, 2, 3]);
    }

    #[test]
    fn keeps_everything_when_under_limit() {
        let doomed = files_to_delete(vec!["a", "b"], 5);
        assert!(doomed.is_empty());
    }
}

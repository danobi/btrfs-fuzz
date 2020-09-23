#[derive(Default, Clone, Copy)]
pub struct ChunkTreeKey {
    pub start: u64,
    pub size: u64,
}

#[derive(Default, Clone, Copy)]
pub struct ChunkTreeValue {
    pub offset: u64,
}

#[derive(Default)]
pub struct ChunkTreeCache {
    inner: Vec<(ChunkTreeKey, ChunkTreeValue)>,
}

impl ChunkTreeCache {
    pub fn insert(&mut self, key: ChunkTreeKey, value: ChunkTreeValue) {
        if self.contains_overlapping(&key) {
            panic!("overlapping chunk range detected");
        }

        self.inner.push((key, value));
    }

    pub fn mapping_kv(&self, logical: u64) -> Option<(ChunkTreeKey, ChunkTreeValue)> {
        for (k, v) in &self.inner {
            if logical >= k.start && logical < (k.start + k.size) {
                return Some((*k, *v));
            }
        }

        None
    }

    pub fn offset(&self, logical: u64) -> Option<u64> {
        if let Some((k, v)) = self.mapping_kv(logical) {
            Some(v.offset + (logical - k.start))
        } else {
            None
        }
    }

    fn contains_overlapping(&self, key: &ChunkTreeKey) -> bool {
        for (k, _) in &self.inner {
            if (key.start > k.start && key.start < (k.start + k.size))
                || ((key.start + key.size) > k.start && (key.start + key.size) < (k.start + k.size))
            {
                return true;
            }
        }

        false
    }
}

#[test]
fn test_ctc_basic() {
    let mut tree = ChunkTreeCache::default();
    tree.insert(
        ChunkTreeKey { start: 0, size: 5 },
        ChunkTreeValue { offset: 123 },
    );
    tree.insert(
        ChunkTreeKey { start: 5, size: 5 },
        ChunkTreeValue { offset: 234 },
    );

    assert_eq!(tree.offset(0), Some(123));
    assert_eq!(tree.offset(1), Some(124));
    assert_eq!(tree.offset(5), Some(234));
    assert_eq!(tree.offset(6), Some(235));
    assert_eq!(tree.offset(11), None);
}

#[test]
fn test_ctc_random_order() {
    let mut tree = ChunkTreeCache::default();
    tree.insert(
        ChunkTreeKey { start: 10, size: 3 },
        ChunkTreeValue { offset: 345 },
    );
    tree.insert(
        ChunkTreeKey { start: 25, size: 5 },
        ChunkTreeValue { offset: 456 },
    );
    tree.insert(
        ChunkTreeKey { start: 15, size: 5 },
        ChunkTreeValue { offset: 567 },
    );
    tree.insert(
        ChunkTreeKey { start: 0, size: 5 },
        ChunkTreeValue { offset: 123 },
    );
    tree.insert(
        ChunkTreeKey { start: 5, size: 5 },
        ChunkTreeValue { offset: 234 },
    );

    assert_eq!(tree.offset(0), Some(123));
    assert_eq!(tree.offset(1), Some(124));
    assert_eq!(tree.offset(5), Some(234));
    assert_eq!(tree.offset(6), Some(235));
    assert_eq!(tree.offset(11), Some(346));
    assert_eq!(tree.offset(14), None);
    assert_eq!(tree.offset(18), Some(570));
    assert_eq!(tree.offset(20), None);
    assert_eq!(tree.offset(25), Some(456));
}

#[test]
#[should_panic]
fn test_ctc_edge_overlap() {
    let mut tree = ChunkTreeCache::default();
    tree.insert(
        ChunkTreeKey { start: 0, size: 5 },
        ChunkTreeValue { offset: 123 },
    );
    tree.insert(
        ChunkTreeKey { start: 4, size: 5 },
        ChunkTreeValue { offset: 234 },
    );

    // unreached
    assert!(false);
}

#[test]
#[should_panic]
fn test_ctc_inside_overlap() {
    let mut tree = ChunkTreeCache::default();
    tree.insert(
        ChunkTreeKey { start: 0, size: 5 },
        ChunkTreeValue { offset: 123 },
    );
    tree.insert(
        ChunkTreeKey { start: 1, size: 2 },
        ChunkTreeValue { offset: 234 },
    );

    // unreached
    assert!(false);
}

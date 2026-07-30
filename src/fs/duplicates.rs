use anyhow::{Context, Result};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const DUPLICATE_NODE_VISIT_LIMIT: usize = 5_000_000;
const HASH_CHUNK_SIZE: usize = 1024 * 1024;
const PARTIAL_CHUNK_SIZE: usize = 64 * 1024;
const SCAN_YIELD_NODES: usize = 512;
const SCAN_SLEEP_NODES: usize = 16_384;
const HASH_YIELD_CHUNKS: usize = 16;
const HASH_SLEEP_CHUNKS: usize = 128;
const DUPLICATE_BATCH_GROUP_SIZE: usize = 128;
const DUPLICATE_BATCH_MAX_LATENCY: Duration = Duration::from_millis(120);
const SMALL_FILE_FULL_HASH_LIMIT: u64 = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DuplicateFile {
    pub path: PathBuf,
    pub name: String,
    pub relative: String,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DuplicateGroup {
    pub id: u64,
    pub size: u64,
    pub files: Vec<DuplicateFile>,
}

impl DuplicateGroup {
    pub(crate) fn duplicate_bytes(&self) -> u64 {
        self.size
            .saturating_mul(self.files.len().saturating_sub(1) as u64)
    }
}

pub(crate) fn sort_duplicate_groups(groups: &mut [DuplicateGroup]) {
    groups.sort_by(compare_groups);
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DuplicateScanPhase {
    #[default]
    Walking,
    SizeGrouping,
    ContentChecking,
    Complete,
}

impl DuplicateScanPhase {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Walking => "walking",
            Self::SizeGrouping => "grouping",
            Self::ContentChecking => "checking",
            Self::Complete => "complete",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DuplicateScanStats {
    pub(crate) phase: DuplicateScanPhase,
    pub(crate) visited_nodes: usize,
    pub(crate) scanned_files: usize,
    pub(crate) candidate_files: usize,
    pub(crate) checked_candidates: usize,
    pub(crate) hashed_files: usize,
    pub(crate) cached_hashes: usize,
    pub(crate) processed_bytes: u64,
    pub(crate) verified_files: usize,
    pub(crate) groups: usize,
    pub(crate) duplicate_bytes: u64,
    pub(crate) node_limit_reached: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DuplicateHashCache {
    hashes: HashMap<DuplicateHashCacheKey, blake3::Hash>,
}

impl DuplicateHashCache {
    fn get(&self, key: &DuplicateHashCacheKey) -> Option<blake3::Hash> {
        self.hashes.get(key).copied()
    }

    fn insert(&mut self, key: DuplicateHashCacheKey, hash: blake3::Hash) {
        self.hashes.insert(key, hash);
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DuplicateHashCacheKey {
    identity: DuplicateHashCacheIdentity,
    size: u64,
    modified: Option<std::time::SystemTime>,
    changed: Option<std::time::SystemTime>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DuplicateHashCacheIdentity {
    #[cfg(unix)]
    Unix { dev: u64, ino: u64 },
    #[cfg(not(unix))]
    Path(PathBuf),
}

#[derive(Clone, Debug)]
pub(crate) struct DuplicateScanBatch {
    pub(crate) groups: Vec<DuplicateGroup>,
    pub(crate) stats: DuplicateScanStats,
}

#[derive(Clone, Debug)]
pub(crate) struct DuplicateScanResult {
    pub(crate) groups: Vec<DuplicateGroup>,
    pub(crate) stats: DuplicateScanStats,
}

#[derive(Clone, Debug)]
struct CandidateFile {
    path: PathBuf,
    name: String,
    relative: String,
    size: u64,
    modified: Option<std::time::SystemTime>,
    cache_key: Option<DuplicateHashCacheKey>,
}

#[cfg(test)]
pub(crate) fn scan_duplicates_streaming(
    cwd: &Path,
    show_hidden: bool,
    is_canceled: impl Fn() -> bool,
    emit_batch: impl FnMut(DuplicateScanBatch) -> bool,
) -> Result<DuplicateScanResult> {
    let mut cache = DuplicateHashCache::default();
    scan_duplicates_streaming_with_cache(cwd, show_hidden, &mut cache, is_canceled, emit_batch)
}

pub(crate) fn scan_duplicates_streaming_with_cache(
    cwd: &Path,
    show_hidden: bool,
    cache: &mut DuplicateHashCache,
    is_canceled: impl Fn() -> bool,
    mut emit_batch: impl FnMut(DuplicateScanBatch) -> bool,
) -> Result<DuplicateScanResult> {
    let (size_groups, mut stats) = collect_size_candidates(cwd, show_hidden, &is_canceled)?;
    stats.phase = DuplicateScanPhase::SizeGrouping;
    stats.candidate_files = size_groups.values().map(Vec::len).sum();
    let mut size_buckets = size_groups.into_values().collect::<Vec<_>>();
    size_buckets.sort_by(|left, right| compare_candidate_buckets(left, right));

    let mut groups = Vec::new();
    let mut next_id = 1u64;
    let mut batch_emitter = DuplicateBatchEmitter::new(&mut emit_batch);
    if !batch_emitter.emit_progress(stats) {
        return Ok(DuplicateScanResult { groups, stats });
    }
    stats.phase = DuplicateScanPhase::ContentChecking;
    if !batch_emitter.emit_progress(stats) {
        return Ok(DuplicateScanResult { groups, stats });
    }

    for candidates in size_buckets {
        if is_canceled() {
            break;
        }
        if candidates.len() < 2 {
            continue;
        }
        let verified = verified_groups_for_same_size(
            candidates,
            cache,
            &mut stats,
            &mut batch_emitter,
            &is_canceled,
        )?;
        for files in verified {
            if files.len() < 2 {
                continue;
            }
            let size = files[0].size;
            let group = DuplicateGroup {
                id: next_id,
                size,
                files,
            };
            next_id = next_id.wrapping_add(1);
            stats.groups += 1;
            stats.duplicate_bytes = stats
                .duplicate_bytes
                .saturating_add(group.duplicate_bytes());
            if !batch_emitter.push(group.clone(), stats) {
                groups.push(group);
                groups.sort_by(compare_groups);
                return Ok(DuplicateScanResult { groups, stats });
            }
            groups.push(group);
        }
    }
    stats.phase = DuplicateScanPhase::Complete;
    let _ = batch_emitter.flush(stats);
    groups.sort_by(compare_groups);
    Ok(DuplicateScanResult { groups, stats })
}

struct DuplicateBatchEmitter<'a, F>
where
    F: FnMut(DuplicateScanBatch) -> bool,
{
    pending: Vec<DuplicateGroup>,
    last_emit: Instant,
    last_phase: DuplicateScanPhase,
    emit_batch: &'a mut F,
}

impl<'a, F> DuplicateBatchEmitter<'a, F>
where
    F: FnMut(DuplicateScanBatch) -> bool,
{
    fn new(emit_batch: &'a mut F) -> Self {
        Self {
            pending: Vec::with_capacity(DUPLICATE_BATCH_GROUP_SIZE),
            last_emit: Instant::now() - DUPLICATE_BATCH_MAX_LATENCY,
            last_phase: DuplicateScanPhase::Walking,
            emit_batch,
        }
    }

    fn push(&mut self, group: DuplicateGroup, stats: DuplicateScanStats) -> bool {
        self.pending.push(group);
        if self.pending.len() >= DUPLICATE_BATCH_GROUP_SIZE
            || self.last_emit.elapsed() >= DUPLICATE_BATCH_MAX_LATENCY
        {
            return self.flush(stats);
        }
        true
    }

    fn emit_progress(&mut self, stats: DuplicateScanStats) -> bool {
        if stats.phase == self.last_phase && self.last_emit.elapsed() < DUPLICATE_BATCH_MAX_LATENCY
        {
            return true;
        }
        self.last_emit = Instant::now();
        self.last_phase = stats.phase;
        (self.emit_batch)(DuplicateScanBatch {
            groups: Vec::new(),
            stats,
        })
    }

    fn flush(&mut self, stats: DuplicateScanStats) -> bool {
        if self.pending.is_empty() {
            return true;
        }
        let mut groups = std::mem::replace(
            &mut self.pending,
            Vec::with_capacity(DUPLICATE_BATCH_GROUP_SIZE),
        );
        groups.sort_by(compare_groups);
        self.last_emit = Instant::now();
        self.last_phase = stats.phase;
        (self.emit_batch)(DuplicateScanBatch { groups, stats })
    }
}

fn collect_size_candidates(
    cwd: &Path,
    show_hidden: bool,
    is_canceled: &impl Fn() -> bool,
) -> Result<(HashMap<u64, Vec<CandidateFile>>, DuplicateScanStats)> {
    let mut queue = VecDeque::from([cwd.to_path_buf()]);
    let mut stats = DuplicateScanStats::default();
    let mut size_groups: HashMap<u64, Vec<CandidateFile>> = HashMap::new();

    while let Some(dir) = queue.pop_front() {
        if is_canceled() {
            break;
        }
        if stats.visited_nodes >= DUPLICATE_NODE_VISIT_LIMIT {
            stats.node_limit_reached = true;
            break;
        }
        let read_dir = match fs::read_dir(&dir) {
            Ok(read_dir) => read_dir,
            Err(error) if dir == cwd => {
                return Err(error).with_context(|| format!("failed to read {}", cwd.display()));
            }
            Err(_) => continue,
        };
        let mut nodes = Vec::new();
        for entry in read_dir {
            if is_canceled() || stats.visited_nodes >= DUPLICATE_NODE_VISIT_LIMIT {
                stats.node_limit_reached |= stats.visited_nodes >= DUPLICATE_NODE_VISIT_LIMIT;
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            if !show_hidden && super::is_hidden_entry(&entry) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let name_key = name.to_lowercase();
            stats.visited_nodes += 1;
            breathe_after_node(stats.visited_nodes);
            if file_type.is_dir() {
                if !should_prune_dir(&name_key) {
                    nodes.push((name_key, path, true));
                }
                continue;
            }
            if file_type.is_symlink() || !file_type.is_file() {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let size = metadata.len();
            if size == 0 {
                continue;
            }
            stats.scanned_files += 1;
            let Ok(relative_path) = path.strip_prefix(cwd) else {
                continue;
            };
            let relative = relative_path.to_string_lossy().replace('\\', "/");
            size_groups.entry(size).or_default().push(CandidateFile {
                path: path.clone(),
                name,
                relative,
                size,
                modified: metadata.modified().ok(),
                cache_key: duplicate_hash_cache_key(&path, &metadata),
            });
        }
        nodes.sort_by(|a, b| super::natural_cmp(&a.0, &b.0));
        for (_, path, is_dir) in nodes {
            if is_dir {
                queue.push_back(path);
            }
        }
    }
    size_groups.retain(|_, files| files.len() > 1);
    Ok((size_groups, stats))
}

fn duplicate_hash_cache_key(path: &Path, metadata: &fs::Metadata) -> Option<DuplicateHashCacheKey> {
    let identity = duplicate_hash_cache_identity(path, metadata);
    Some(DuplicateHashCacheKey {
        identity,
        size: metadata.len(),
        modified: metadata.modified().ok(),
        changed: metadata_changed_time(metadata),
    })
}

#[cfg(unix)]
fn duplicate_hash_cache_identity(
    _path: &Path,
    metadata: &fs::Metadata,
) -> DuplicateHashCacheIdentity {
    DuplicateHashCacheIdentity::Unix {
        dev: metadata.dev(),
        ino: metadata.ino(),
    }
}

#[cfg(not(unix))]
fn duplicate_hash_cache_identity(
    path: &Path,
    _metadata: &fs::Metadata,
) -> DuplicateHashCacheIdentity {
    DuplicateHashCacheIdentity::Path(path.to_path_buf())
}

#[cfg(unix)]
fn metadata_changed_time(metadata: &fs::Metadata) -> Option<std::time::SystemTime> {
    let secs = metadata.ctime();
    let nanos = metadata.ctime_nsec();
    if secs < 0 || nanos < 0 {
        return None;
    }
    Some(
        std::time::UNIX_EPOCH
            + Duration::from_secs(secs as u64)
            + Duration::from_nanos(nanos as u64),
    )
}

#[cfg(not(unix))]
fn metadata_changed_time(_metadata: &fs::Metadata) -> Option<std::time::SystemTime> {
    None
}

fn verified_groups_for_same_size<F>(
    candidates: Vec<CandidateFile>,
    cache: &mut DuplicateHashCache,
    stats: &mut DuplicateScanStats,
    batch_emitter: &mut DuplicateBatchEmitter<'_, F>,
    is_canceled: &impl Fn() -> bool,
) -> Result<Vec<Vec<DuplicateFile>>>
where
    F: FnMut(DuplicateScanBatch) -> bool,
{
    let candidates = if candidates
        .first()
        .is_some_and(|candidate| candidate.size <= SMALL_FILE_FULL_HASH_LIMIT)
    {
        candidates
    } else {
        partial_duplicate_candidates(candidates, stats, is_canceled)
    };

    let mut by_hash: HashMap<blake3::Hash, Vec<CandidateFile>> = HashMap::new();
    for candidate in candidates {
        if is_canceled() {
            break;
        }
        match content_hash(&candidate, cache, stats, batch_emitter, is_canceled) {
            Ok(Some(hash)) => {
                stats.checked_candidates += 1;
                stats.hashed_files += 1;
                by_hash.entry(hash).or_default().push(candidate);
            }
            Ok(None) => break,
            Err(_) => {
                stats.checked_candidates += 1;
            }
        }
    }

    let mut groups = Vec::new();
    for same_hash in by_hash.into_values().filter(|files| files.len() > 1) {
        let mut representatives: Vec<Vec<CandidateFile>> = Vec::new();
        'candidate: for candidate in same_hash {
            if is_canceled() {
                break;
            }
            stats.verified_files += 1;
            for group in &mut representatives {
                if files_equal(&candidate.path, &group[0].path).unwrap_or(false) {
                    group.push(candidate);
                    continue 'candidate;
                }
            }
            representatives.push(vec![candidate]);
        }
        for group in representatives.into_iter().filter(|g| g.len() > 1) {
            let mut files = group
                .into_iter()
                .map(|file| DuplicateFile {
                    path: file.path,
                    name: file.name,
                    relative: file.relative,
                    size: file.size,
                    modified: file.modified,
                })
                .collect::<Vec<_>>();
            files.sort_by(|a, b| {
                super::natural_cmp(&a.relative.to_lowercase(), &b.relative.to_lowercase())
                    .then_with(|| a.relative.cmp(&b.relative))
            });
            groups.push(files);
        }
    }
    Ok(groups)
}

fn partial_duplicate_candidates(
    candidates: Vec<CandidateFile>,
    stats: &mut DuplicateScanStats,
    is_canceled: &impl Fn() -> bool,
) -> Vec<CandidateFile> {
    let mut by_partial: HashMap<blake3::Hash, Vec<CandidateFile>> = HashMap::new();
    let mut processed = 0usize;
    for (index, candidate) in candidates.into_iter().enumerate() {
        if is_canceled() {
            break;
        }
        processed += 1;
        breathe_after_node(index + 1);
        if let Ok(fingerprint) = partial_content_fingerprint(&candidate.path, candidate.size) {
            by_partial.entry(fingerprint).or_default().push(candidate);
        }
    }
    let survivors = by_partial
        .into_values()
        .filter(|files| files.len() > 1)
        .flatten()
        .collect::<Vec<_>>();
    stats.checked_candidates = stats
        .checked_candidates
        .saturating_add(processed.saturating_sub(survivors.len()));
    survivors
}

fn partial_content_fingerprint(path: &Path, size: u64) -> Result<blake3::Hash> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());

    let mut first = vec![0u8; PARTIAL_CHUNK_SIZE.min(size as usize)];
    let first_read = file.read(&mut first)?;
    hasher.update(&first[..first_read]);

    if size > PARTIAL_CHUNK_SIZE as u64 {
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(
            size.saturating_sub(PARTIAL_CHUNK_SIZE as u64),
        ))?;
        let mut last = vec![0u8; PARTIAL_CHUNK_SIZE];
        let last_read = file.read(&mut last)?;
        hasher.update(&last[..last_read]);
    }

    Ok(hasher.finalize())
}

fn content_hash<F>(
    candidate: &CandidateFile,
    cache: &mut DuplicateHashCache,
    stats: &mut DuplicateScanStats,
    batch_emitter: &mut DuplicateBatchEmitter<'_, F>,
    is_canceled: &impl Fn() -> bool,
) -> Result<Option<blake3::Hash>>
where
    F: FnMut(DuplicateScanBatch) -> bool,
{
    if let Some(cache_key) = &candidate.cache_key
        && let Some(hash) = cache.get(cache_key)
    {
        stats.cached_hashes += 1;
        return Ok(Some(hash));
    }

    let hash = content_hash_uncached(&candidate.path, stats, batch_emitter, is_canceled)?;
    if let Some(hash) = hash
        && let Some(cache_key) = &candidate.cache_key
    {
        cache.insert(cache_key.clone(), hash);
    }
    Ok(hash)
}

fn content_hash_uncached<F>(
    path: &Path,
    stats: &mut DuplicateScanStats,
    batch_emitter: &mut DuplicateBatchEmitter<'_, F>,
    is_canceled: &impl Fn() -> bool,
) -> Result<Option<blake3::Hash>>
where
    F: FnMut(DuplicateScanBatch) -> bool,
{
    let mut reader = BufReader::new(
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?,
    );
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; HASH_CHUNK_SIZE];
    let mut chunks = 0usize;
    loop {
        if is_canceled() {
            return Ok(None);
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        stats.processed_bytes = stats.processed_bytes.saturating_add(read as u64);
        let _ = batch_emitter.emit_progress(*stats);
        chunks += 1;
        breathe_after_hash_chunk(chunks);
    }
    Ok(Some(hasher.finalize()))
}

fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    let mut left = BufReader::new(File::open(left)?);
    let mut right = BufReader::new(File::open(right)?);
    let mut left_buf = vec![0u8; HASH_CHUNK_SIZE];
    let mut right_buf = vec![0u8; HASH_CHUNK_SIZE];
    let mut chunks = 0usize;
    loop {
        let left_read = left.read(&mut left_buf)?;
        let right_read = right.read(&mut right_buf)?;
        if left_read != right_read {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
        if left_buf[..left_read] != right_buf[..right_read] {
            return Ok(false);
        }
        chunks += 1;
        breathe_after_hash_chunk(chunks);
    }
}

fn breathe_after_node(count: usize) {
    if count.is_multiple_of(SCAN_SLEEP_NODES) {
        std::thread::sleep(std::time::Duration::from_millis(1));
    } else if count.is_multiple_of(SCAN_YIELD_NODES) {
        std::thread::yield_now();
    }
}

fn breathe_after_hash_chunk(chunks: usize) {
    if chunks.is_multiple_of(HASH_SLEEP_CHUNKS) {
        std::thread::sleep(std::time::Duration::from_millis(1));
    } else if chunks.is_multiple_of(HASH_YIELD_CHUNKS) {
        std::thread::yield_now();
    }
}

fn compare_groups(left: &DuplicateGroup, right: &DuplicateGroup) -> std::cmp::Ordering {
    right
        .size
        .cmp(&left.size)
        .then_with(|| right.duplicate_bytes().cmp(&left.duplicate_bytes()))
        .then_with(|| {
            left.files
                .first()
                .map(|f| &f.relative)
                .cmp(&right.files.first().map(|f| &f.relative))
        })
}

fn compare_candidate_buckets(
    left: &[CandidateFile],
    right: &[CandidateFile],
) -> std::cmp::Ordering {
    candidate_bucket_priority(left).cmp(&candidate_bucket_priority(right))
}

fn candidate_bucket_priority(
    files: &[CandidateFile],
) -> (
    u8,
    std::cmp::Reverse<usize>,
    u64,
    std::cmp::Reverse<u64>,
    u64,
) {
    let size = files.first().map_or(0, |file| file.size);
    let class = if size <= SMALL_FILE_FULL_HASH_LIMIT {
        0
    } else if size <= 64 * 1024 * 1024 && files.len() >= 3 {
        1
    } else if size <= 512 * 1024 * 1024 {
        2
    } else {
        3
    };
    (
        class,
        std::cmp::Reverse(files.len()),
        candidate_bucket_read_bytes(files),
        std::cmp::Reverse(duplicate_candidate_bytes(files)),
        size,
    )
}

fn candidate_bucket_read_bytes(files: &[CandidateFile]) -> u64 {
    files
        .first()
        .map_or(0, |file| file.size.saturating_mul(files.len() as u64))
}

fn duplicate_candidate_bytes(files: &[CandidateFile]) -> u64 {
    files.first().map_or(0, |file| {
        file.size
            .saturating_mul(files.len().saturating_sub(1) as u64)
    })
}

fn should_prune_dir(name_key: &str) -> bool {
    matches!(name_key, ".git" | "node_modules" | "target")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "elio-duplicates-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn candidate(name: &str, size: u64) -> CandidateFile {
        CandidateFile {
            path: PathBuf::from(name),
            name: name.to_string(),
            relative: name.to_string(),
            size,
            modified: None,
            cache_key: None,
        }
    }

    fn duplicate_group(id: u64, size: u64, names: &[&str]) -> DuplicateGroup {
        DuplicateGroup {
            id,
            size,
            files: names
                .iter()
                .map(|name| DuplicateFile {
                    path: PathBuf::from(name),
                    name: (*name).to_string(),
                    relative: (*name).to_string(),
                    size,
                    modified: None,
                })
                .collect(),
        }
    }

    #[test]
    fn duplicate_group_order_prefers_larger_files_before_larger_reclaimable_groups() {
        let mut groups = [
            duplicate_group(1, 700 * 1024, &["small-a", "small-b", "small-c", "small-d"]),
            duplicate_group(2, 20 * 1024 * 1024, &["medium-a", "medium-b"]),
            duplicate_group(
                3,
                20 * 1024 * 1024,
                &["medium-more-a", "medium-more-b", "medium-more-c"],
            ),
            duplicate_group(4, 460 * 1024 * 1024, &["large-a", "large-b"]),
        ];

        groups.sort_by(compare_groups);

        assert_eq!(
            groups.iter().map(|group| group.id).collect::<Vec<_>>(),
            vec![4, 3, 2, 1]
        );
    }

    #[test]
    fn candidate_bucket_order_does_not_let_huge_low_count_files_starve_cheap_groups() {
        let huge_size = 16 * 1024 * 1024 * 1024;
        let mut buckets = [
            (0..6)
                .map(|index| candidate(&format!("huge-{index}"), huge_size))
                .collect::<Vec<_>>(),
            (0..20)
                .map(|index| candidate(&format!("small-{index}"), 1024))
                .collect::<Vec<_>>(),
        ];

        buckets.sort_by(|left, right| compare_candidate_buckets(left, right));

        assert_eq!(buckets[0][0].size, 1024);
        assert_eq!(buckets[1][0].size, huge_size);
    }

    #[test]
    fn same_session_cache_reuses_unchanged_content_hashes() {
        let root = temp_path("hash-cache-hit");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"same").unwrap();
        fs::write(root.join("b.txt"), b"same").unwrap();
        let mut cache = DuplicateHashCache::default();

        let first =
            scan_duplicates_streaming_with_cache(&root, true, &mut cache, || false, |_| true)
                .unwrap();
        let second =
            scan_duplicates_streaming_with_cache(&root, true, &mut cache, || false, |_| true)
                .unwrap();

        assert_eq!(first.groups, second.groups);
        assert_eq!(first.stats.cached_hashes, 0);
        assert_eq!(first.stats.checked_candidates, first.stats.candidate_files);
        assert_eq!(second.stats.cached_hashes, second.stats.hashed_files);
        assert_eq!(
            second.stats.checked_candidates,
            second.stats.candidate_files
        );
        assert_eq!(second.stats.processed_bytes, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_session_cache_misses_when_file_metadata_changes() {
        let root = temp_path("hash-cache-stale");
        fs::create_dir_all(&root).unwrap();
        let changed = root.join("a.txt");
        fs::write(&changed, b"same").unwrap();
        fs::write(root.join("b.txt"), b"same").unwrap();
        let mut cache = DuplicateHashCache::default();

        let first =
            scan_duplicates_streaming_with_cache(&root, true, &mut cache, || false, |_| true)
                .unwrap();
        std::thread::sleep(Duration::from_millis(10));
        fs::write(&changed, b"diff").unwrap();
        let second =
            scan_duplicates_streaming_with_cache(&root, true, &mut cache, || false, |_| true)
                .unwrap();

        assert_eq!(first.groups.len(), 1);
        assert!(second.groups.is_empty());
        assert_eq!(
            second.stats.checked_candidates,
            second.stats.candidate_files
        );
        assert!(second.stats.cached_hashes < second.stats.hashed_files);
        assert!(second.stats.processed_bytes > 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_groups_exact_duplicate_files_by_content_not_name() {
        let root = temp_path("exact");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("a.txt"), b"same").unwrap();
        fs::write(root.join("nested/renamed.bin"), b"same").unwrap();
        fs::write(root.join("different.txt"), b"diff").unwrap();

        let result = scan_duplicates_streaming(&root, true, || false, |_| true).unwrap();

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].files.len(), 2);
        assert_eq!(result.groups[0].duplicate_bytes(), 4);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_coalesces_small_duplicate_batches() {
        let root = temp_path("coalesced-batches");
        fs::create_dir_all(&root).unwrap();
        for (left, right, content) in [
            ("a1.txt", "a2.txt", b"aa".as_slice()),
            ("b1.txt", "b2.txt", b"bbb".as_slice()),
            ("c1.txt", "c2.txt", b"cccc".as_slice()),
        ] {
            fs::write(root.join(left), content).unwrap();
            fs::write(root.join(right), content).unwrap();
        }

        let mut batches = Vec::new();
        let result = scan_duplicates_streaming(
            &root,
            true,
            || false,
            |batch| {
                batches.push(batch);
                true
            },
        )
        .unwrap();

        assert_eq!(result.groups.len(), 3);
        let group_batches = batches
            .iter()
            .filter(|batch| !batch.groups.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(group_batches.len(), 1);
        assert_eq!(group_batches[0].groups.len(), 3);
        assert_eq!(result.stats.phase, DuplicateScanPhase::Complete);
        let phases = batches
            .iter()
            .map(|batch| batch.stats.phase)
            .collect::<Vec<_>>();
        assert!(phases.contains(&DuplicateScanPhase::SizeGrouping));
        assert!(phases.contains(&DuplicateScanPhase::ContentChecking));
        assert_eq!(result.stats.candidate_files, 6);
        assert_eq!(result.stats.checked_candidates, 6);
        assert_eq!(result.stats.hashed_files, 6);
        assert!(result.stats.processed_bytes > 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_respects_hidden_file_setting_for_directories() {
        let root = temp_path("hidden-dirs");
        fs::create_dir_all(root.join("visible")).unwrap();
        fs::create_dir_all(root.join(".cache")).unwrap();
        fs::write(root.join("visible/a.txt"), b"visible duplicate").unwrap();
        fs::write(root.join("visible/b.txt"), b"visible duplicate").unwrap();
        fs::write(root.join(".cache/a.txt"), b"hidden duplicate").unwrap();
        fs::write(root.join(".cache/b.txt"), b"hidden duplicate").unwrap();

        let hidden_off = scan_duplicates_streaming(&root, false, || false, |_| true).unwrap();
        assert_eq!(hidden_off.groups.len(), 1);
        assert!(
            hidden_off.groups[0]
                .files
                .iter()
                .all(|file| file.relative.starts_with("visible/"))
        );

        let hidden_on = scan_duplicates_streaming(&root, true, || false, |_| true).unwrap();
        assert_eq!(hidden_on.groups.len(), 2);
        assert!(hidden_on.groups.iter().any(|group| {
            group
                .files
                .iter()
                .all(|file| file.relative.starts_with(".cache/"))
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_ignores_zero_byte_files() {
        let root = temp_path("zero");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a"), b"").unwrap();
        fs::write(root.join("b"), b"").unwrap();

        let result = scan_duplicates_streaming(&root, true, || false, |_| true).unwrap();

        assert!(result.groups.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

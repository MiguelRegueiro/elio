use crate::app::overlays::images::{PreparedStaticImageAsset, SixelDcsKey};
use crate::app::overlays::inline_image::TerminalWindowSize;
use crate::app::overlays::pdf::PdfProbeResult;
use crate::app::{ClipOp, SearchScope};
use crate::core::{Entry, SortMode};
use crate::fs::duplicates::{DuplicateScanBatch, DuplicateScanResult};
use crate::fs::search::{SearchIndex, SearchIndexBatch};
use crate::{preview, preview::PreviewWorkClass};
use std::{path::PathBuf, sync::Arc, time::SystemTime};

/// Parameters needed by the background image-prepare job to pre-encode a
/// Sixel DCS stream alongside the rendered PNG.  Bundled as an `Option` so
/// non-Sixel sessions pay no extra memory cost.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::app) struct SixelPrepareConfig {
    /// Width of the target area in terminal cells.
    pub(in crate::app) area_width: u16,
    /// Height of the target area in terminal cells.
    pub(in crate::app) area_height: u16,
    /// Terminal window dimensions at the time the job was submitted.
    /// Required to reproduce the exact aspect-ratio fitting and pixel-size
    /// computation that will be used at render time.
    pub(in crate::app) window_size: TerminalWindowSize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum PreviewPriority {
    High,
    Low,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum PdfJobPriority {
    Current,
    Prefetch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum ImageJobPriority {
    Current,
    Nearby,
}

#[derive(Debug)]
pub(in crate::app) struct SearchBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) scope: SearchScope,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) fingerprint: crate::fs::DirectoryFingerprint,
    pub(in crate::app) result: Result<SearchIndex, String>,
}

#[derive(Debug)]
pub(in crate::app) struct SearchBatchBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) scope: SearchScope,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) fingerprint: crate::fs::DirectoryFingerprint,
    pub(in crate::app) batch: SearchIndexBatch,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct SearchRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) scope: SearchScope,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) fingerprint: crate::fs::DirectoryFingerprint,
}

#[derive(Debug)]
pub(in crate::app) struct DuplicateScanBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) fingerprint: crate::fs::DirectoryFingerprint,
    pub(in crate::app) result: Result<DuplicateScanResult, String>,
}

#[derive(Debug)]
pub(in crate::app) struct DuplicateScanBatchBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) fingerprint: crate::fs::DirectoryFingerprint,
    pub(in crate::app) batch: DuplicateScanBatch,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct DuplicateScanRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) fingerprint: crate::fs::DirectoryFingerprint,
}

#[derive(Debug)]
pub(in crate::app) struct DirectoryBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) result: Result<crate::fs::DirectorySnapshot, String>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct DirectoryRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) sort_mode: SortMode,
}

#[derive(Debug)]
pub(in crate::app) struct DirectoryItemCountBuild {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) item_count: Option<usize>,
}

#[derive(Debug)]
pub(in crate::app) struct DirectoryStatsBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) result: crate::fs::DirectoryStatsScanResult,
}

#[derive(Debug)]
pub(in crate::app) struct DirectoryFingerprintBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) show_hidden: bool,
    pub(in crate::app) result: Result<crate::fs::DirectoryFingerprint, String>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct DirectoryFingerprintRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) show_hidden: bool,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct DirectoryItemCountRequest {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) show_hidden: bool,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct DirectoryStatsRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) path: PathBuf,
}

#[derive(Debug)]
pub(in crate::app) struct GitStatusBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) branch: Option<String>,
    pub(in crate::app) dirty: bool,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct GitStatusRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
}

#[derive(Debug)]
pub(in crate::app) struct PreviewLineCountBuild {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) size: u64,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) total_lines: Option<usize>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct PreviewLineCountRequest {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) size: u64,
    pub(in crate::app) modified: Option<SystemTime>,
}

#[derive(Debug)]
pub(in crate::app) struct ImagePrepareBuild {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) size: u64,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) target_width_px: u32,
    pub(in crate::app) target_height_px: u32,
    pub(in crate::app) force_render_to_cache: bool,
    pub(in crate::app) prepare_inline_payload: bool,
    pub(in crate::app) canceled: bool,
    pub(in crate::app) result: Option<PreparedStaticImageAsset>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct ImagePrepareRequest {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) size: u64,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) target_width_px: u32,
    pub(in crate::app) target_height_px: u32,
    pub(in crate::app) ffmpeg_available: bool,
    pub(in crate::app) resvg_available: bool,
    pub(in crate::app) magick_available: bool,
    pub(in crate::app) force_render_to_cache: bool,
    pub(in crate::app) prepare_inline_payload: bool,
    /// When `Some`, the prepare job also encodes a Sixel DCS stream for the
    /// rendered image using the area and window dimensions supplied here.
    pub(in crate::app) sixel_prepare: Option<SixelPrepareConfig>,
}

#[derive(Debug)]
pub(in crate::app) struct PdfProbeBuild {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) size: u64,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) page: usize,
    pub(in crate::app) result: Result<PdfProbeResult, String>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct PdfProbeRequest {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) size: u64,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) page: usize,
}

#[derive(Debug)]
pub(in crate::app) struct PdfRenderBuild {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) size: u64,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) page: usize,
    pub(in crate::app) width_px: u32,
    pub(in crate::app) height_px: u32,
    pub(in crate::app) sixel_dcs: Option<Arc<[u8]>>,
    pub(in crate::app) sixel_dcs_key: Option<SixelDcsKey>,
    pub(in crate::app) result: Result<Option<PathBuf>, String>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct PdfRenderRequest {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) size: u64,
    pub(in crate::app) modified: Option<SystemTime>,
    pub(in crate::app) page: usize,
    pub(in crate::app) width_px: u32,
    pub(in crate::app) height_px: u32,
    pub(in crate::app) sixel_prepare: Option<SixelPrepareConfig>,
}

#[derive(Debug)]
pub(in crate::app) struct PreviewBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) entry: Entry,
    pub(in crate::app) variant: preview::PreviewRequestOptions,
    pub(in crate::app) code_line_limit: usize,
    /// The actual line limit used for this render pass. May be less than
    /// `code_line_limit` for initial incremental renders.
    pub(in crate::app) code_render_limit: usize,
    pub(in crate::app) ffmpeg_available: bool,
    pub(in crate::app) result: preview::PreviewContent,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct PreviewRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) entry: Entry,
    pub(in crate::app) variant: preview::PreviewRequestOptions,
    pub(in crate::app) code_line_limit: usize,
    /// The actual render line limit for this pass. For the initial incremental
    /// render this is smaller than `code_line_limit`; for extension/prefetch
    /// renders it equals `code_line_limit`.
    pub(in crate::app) code_render_limit: usize,
    pub(in crate::app) priority: PreviewPriority,
    pub(in crate::app) work_class: PreviewWorkClass,
    pub(in crate::app) ffprobe_available: bool,
    pub(in crate::app) ffmpeg_available: bool,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct ArchiveCreateRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) cwd: PathBuf,
    pub(in crate::app) sources: Vec<PathBuf>,
    pub(in crate::app) output_name: String,
    pub(in crate::app) options: crate::archive::CreateArchiveOptions,
}

#[derive(Debug)]
pub(in crate::app) struct ArchiveCreateBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) completed: usize,
    pub(in crate::app) total: usize,
    /// `true` on the final result; `false` on intermediate progress updates.
    pub(in crate::app) done: bool,
    /// Populated only when `done = true`.
    pub(in crate::app) output_path: Option<PathBuf>,
    /// Populated only when `done = true`.
    pub(in crate::app) status: Option<String>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct ArchiveExtractBatchState {
    pub(in crate::app) total_archives: usize,
    pub(in crate::app) completed_archives: usize,
    pub(in crate::app) failed_archives: usize,
    pub(in crate::app) skipped_archives: usize,
    pub(in crate::app) skipped_non_archives: usize,
    pub(in crate::app) dest_dirs: Vec<PathBuf>,
}

impl ArchiveExtractBatchState {
    pub(in crate::app) fn new(total_archives: usize, skipped_non_archives: usize) -> Self {
        Self {
            total_archives,
            completed_archives: 0,
            failed_archives: 0,
            skipped_archives: 0,
            skipped_non_archives,
            dest_dirs: Vec::new(),
        }
    }

    pub(in crate::app) fn finished_archives(&self) -> usize {
        self.completed_archives + self.failed_archives + self.skipped_archives
    }

    pub(in crate::app) fn is_single_archive(&self) -> bool {
        self.total_archives == 1 && self.skipped_non_archives == 0
    }

    pub(in crate::app) fn reselect_path(&self) -> Option<PathBuf> {
        self.is_single_archive()
            .then(|| self.dest_dirs.first().cloned())
            .flatten()
    }

    pub(in crate::app) fn status(&self) -> String {
        if self.is_single_archive()
            && self.completed_archives == 1
            && let Some(dest_dir) = self.dest_dirs.first()
        {
            let name = dest_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("folder");
            return format!("Extracted 1 archive to \"{name}\"");
        }

        let mut parts = Vec::new();
        parts.push(format!(
            "Extracted {} {}",
            self.completed_archives,
            if self.completed_archives == 1 {
                "archive"
            } else {
                "archives"
            }
        ));
        if self.failed_archives > 0 {
            parts.push(format!("{} failed", self.failed_archives));
        }
        if self.skipped_archives > 0 {
            parts.push(format!("{} skipped", self.skipped_archives));
        }
        if self.skipped_non_archives > 0 {
            parts.push(format!(
                "skipped {} {}",
                self.skipped_non_archives,
                if self.skipped_non_archives == 1 {
                    "non-archive"
                } else {
                    "non-archives"
                }
            ));
        }
        parts.join(", ")
    }
}

#[derive(Clone, Debug)]
pub(in crate::app) struct ArchiveExtractRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) archives: Vec<PathBuf>,
    pub(in crate::app) password: Option<crate::archive::ArchivePassword>,
    pub(in crate::app) batch: ArchiveExtractBatchState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum ArchivePasswordPrompt {
    Required,
    BadPassword,
}

#[derive(Debug)]
pub(in crate::app) struct ArchiveExtractBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) completed: usize,
    pub(in crate::app) total: Option<usize>,
    /// `true` on the final result; `false` on intermediate progress updates.
    pub(in crate::app) done: bool,
    /// Populated only when `done = true`.
    pub(in crate::app) dest_dir: Option<PathBuf>,
    /// Populated only when `done = true`.
    pub(in crate::app) status: Option<String>,
    /// Populated only when `done = true` and extraction needs password input.
    pub(in crate::app) password_prompt: Option<ArchivePasswordPrompt>,
    /// Remaining batch to resume after password input.
    pub(in crate::app) password_request: Option<ArchiveExtractRequest>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct PasteRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) dest_dir: PathBuf,
    pub(in crate::app) paths: Vec<PathBuf>,
    pub(in crate::app) op: ClipOp,
}

#[derive(Debug)]
pub(in crate::app) struct PasteBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) completed: usize,
    /// `true` on the final result; `false` on intermediate progress updates.
    pub(in crate::app) done: bool,
    /// Populated only when `done = true`.
    pub(in crate::app) status: Option<String>,
    /// Destination paths actually written by the completed paste/drop.
    pub(in crate::app) destination_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct TrashRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) targets: Vec<crate::app::state::TrashTarget>,
    pub(in crate::app) permanent: bool,
}

#[derive(Debug)]
pub(in crate::app) struct TrashBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) completed: usize,
    /// `true` on the final result; `false` on intermediate progress updates.
    pub(in crate::app) done: bool,
    /// Populated only when `done = true`.
    pub(in crate::app) status: Option<String>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct RestoreRequest {
    pub(in crate::app) token: u64,
    pub(in crate::app) targets: Vec<crate::app::state::TrashTarget>,
}

#[derive(Debug)]
pub(in crate::app) struct RestoreBuild {
    pub(in crate::app) token: u64,
    pub(in crate::app) completed: usize,
    /// `true` on the final result; `false` on intermediate progress updates.
    pub(in crate::app) done: bool,
    /// Populated only when `done = true`.
    pub(in crate::app) status: Option<String>,
}

#[derive(Debug)]
pub(in crate::app) enum JobResult {
    Directory(DirectoryBuild),
    DirectoryFingerprint(DirectoryFingerprintBuild),
    DirectoryItemCount(DirectoryItemCountBuild),
    DirectoryStats(DirectoryStatsBuild),
    GitStatus(GitStatusBuild),
    PreviewLineCount(PreviewLineCountBuild),
    ImagePrepare(ImagePrepareBuild),
    PdfProbe(PdfProbeBuild),
    PdfRender(PdfRenderBuild),
    SearchBatch(SearchBatchBuild),
    Search(SearchBuild),
    DuplicateScanBatch(DuplicateScanBatchBuild),
    DuplicateScan(DuplicateScanBuild),
    Preview(Box<PreviewBuild>),
    ArchiveCreate(ArchiveCreateBuild),
    ArchiveExtract(ArchiveExtractBuild),
    Paste(PasteBuild),
    Trash(TrashBuild),
    Restore(RestoreBuild),
}

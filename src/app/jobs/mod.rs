mod config;
mod metrics;
mod pool;
mod results;
mod scheduler;
mod sync;
mod tasks;
mod types;

#[cfg(test)]
pub(super) use self::metrics::SchedulerMetricsSnapshot;
pub(super) use self::scheduler::JobScheduler;
use self::sync::{lock_unpoison, wait_unpoison};
pub(super) use self::types::{
    ArchiveCreateBuild, ArchiveCreateRequest, ArchiveExtractBatchState, ArchiveExtractBuild,
    ArchiveExtractRequest, ArchivePasswordPrompt, DirectoryBuild, DirectoryFingerprintBuild,
    DirectoryFingerprintRequest, DirectoryItemCountBuild, DirectoryItemCountRequest,
    DirectoryRequest, DirectoryStatsBuild, DirectoryStatsRequest, GitStatusBuild, GitStatusRequest,
    ImageJobPriority, ImagePrepareBuild, ImagePrepareRequest, JobResult, PasteBuild, PasteRequest,
    PdfJobPriority, PdfProbeBuild, PdfProbeRequest, PdfRenderBuild, PdfRenderRequest, PreviewBuild,
    PreviewLineCountBuild, PreviewLineCountRequest, PreviewPriority, PreviewRequest, RestoreBuild,
    RestoreRequest, SearchBatchBuild, SearchBuild, SearchRequest, SixelPrepareConfig, TrashBuild,
    TrashRequest,
};
use self::{config::SchedulerConfig, metrics::SchedulerMetrics};
#[cfg(test)]
use self::{
    pool::{preview::PreviewJobKey, search::SearchJobKey},
    tasks::{image::ImagePrepareJobKey, pdf_probe::PdfProbeJobKey, pdf_render::PdfRenderJobKey},
};
use super::*;

#[cfg(test)]
mod tests;

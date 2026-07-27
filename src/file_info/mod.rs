mod archives;
mod classify;
mod extensions;
mod license;
mod names;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use self::archives::inspect_compound_archive_name;
pub(crate) use self::classify::{
    inspect_entry_cached, inspect_entry_fast, inspect_path, inspect_path_cached,
};
pub(crate) use self::license::is_canonical_license_file_name;
pub(crate) use self::types::{
    CodeBackend, CompoundArchiveKind, CompressionKind, CustomCodeKind, DiskImageKind,
    DocumentFormat, FileFacts, PreviewKind, PreviewSpec, StructuredFormat,
};

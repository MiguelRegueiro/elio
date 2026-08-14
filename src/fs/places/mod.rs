mod devices;
#[cfg(target_os = "linux")]
mod linux;
mod resolution;
#[cfg(test)]
mod tests;

pub(crate) use self::resolution::{build_sidebar_rows, trash_dir};

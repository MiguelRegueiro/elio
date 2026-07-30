mod actions;
mod input;
mod model;
mod scan;

#[cfg(test)]
mod tests;

use super::*;
use crate::app::jobs::DuplicateScanRequest;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

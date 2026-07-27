use super::{FileFacts, PreviewKind};
use crate::core::FileClass;
use std::{fs, fs::File, io::Read, path::Path};

const FAST_LICENSE_SNIFF_BYTE_LIMIT: usize = 4 * 1024;
const LICENSE_SNIFF_BYTE_LIMIT: usize = 64 * 1024;
const LICENSE_MARKER_LINE_LIMIT: usize = 12;
const LICENSE_PREAMBLE_LINE_LIMIT: usize = 8;

struct HighSignalLicenseSignature {
    detail_label: &'static str,
    top_markers: &'static [&'static str],
    required_phrases: &'static [&'static str],
    forbidden_phrases: &'static [&'static str],
}

const HIGH_SIGNAL_LICENSE_SIGNATURES: &[HighSignalLicenseSignature] = &[
    HighSignalLicenseSignature {
        detail_label: "Creative Commons Attribution-ShareAlike 3.0 Austria",
        top_markers: &[
            "CREATIVE COMMONS IST KEINE RECHTSANWALTSKANZLEI UND LEISTET KEINE RECHTSBERATUNG",
        ],
        required_phrases: &[
            "CREATIVE COMMONS IST KEINE RECHTSANWALTSKANZLEI UND LEISTET KEINE RECHTSBERATUNG",
            "WEITERGABE UNTER GLEICHEN BEDINGUNGEN",
            "RECHT DER REPUBLIK ÖSTERREICH ANWENDUNG",
        ],
        forbidden_phrases: &[],
    },
    HighSignalLicenseSignature {
        detail_label: "Creative Commons Attribution 3.0 Austria",
        top_markers: &[
            "CREATIVE COMMONS IST KEINE RECHTSANWALTSKANZLEI UND LEISTET KEINE RECHTSBERATUNG",
        ],
        required_phrases: &[
            "CREATIVE COMMONS IST KEINE RECHTSANWALTSKANZLEI UND LEISTET KEINE RECHTSBERATUNG",
            "CREATIVE COMMONS PUBLIC LICENSE",
            "RECHT DER REPUBLIK ÖSTERREICH ANWENDUNG",
        ],
        forbidden_phrases: &["WEITERGABE UNTER GLEICHEN BEDINGUNGEN"],
    },
    HighSignalLicenseSignature {
        detail_label: "Creative Commons Attribution-ShareAlike 2.1 Japan",
        top_markers: &["アトリビューション—シェアアライク 2.1", "帰属—同一条件許諾"],
        required_phrases: &["アトリビューション—シェアアライク 2.1", "帰属—同一条件許諾"],
        forbidden_phrases: &[],
    },
    HighSignalLicenseSignature {
        detail_label: "W3C Software Notice and License",
        top_markers: &["W3C SOFTWARE NOTICE AND LICENSE"],
        required_phrases: &[
            "W3C SOFTWARE NOTICE AND LICENSE",
            "By obtaining, using and/or copying this work",
        ],
        forbidden_phrases: &[],
    },
    HighSignalLicenseSignature {
        detail_label: "WTFPL",
        top_markers: &["DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE"],
        required_phrases: &[
            "DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE",
            "Everyone is permitted to copy and distribute verbatim or modified copies",
        ],
        forbidden_phrases: &[],
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LicenseDetection {
    Specific { detail_label: &'static str },
    Generic,
}

impl LicenseDetection {
    const fn detail_label(self) -> &'static str {
        match self {
            Self::Specific { detail_label } => detail_label,
            Self::Generic => "License document",
        }
    }
}

pub(super) fn sniff_license_file_type(
    path: &Path,
    name: &str,
    ext: &str,
    base_facts: FileFacts,
) -> Option<FileFacts> {
    let canonical_candidate = is_canonical_license_candidate_key(name);
    let can_sniff_content = can_sniff_license_content(base_facts);
    let can_sniff_markers = can_sniff_license_markers(ext, base_facts);

    if !canonical_candidate && !can_sniff_markers {
        return None;
    }
    if canonical_candidate && !can_sniff_content {
        return None;
    }

    let text = read_license_text(path)?;
    if !canonical_candidate {
        let has_spdx = detect_spdx_identifier(&text).is_some();
        if !has_spdx && !has_strong_license_markers(&text) {
            return None;
        }
        if !has_spdx && !starts_like_standalone_license(&text) {
            return None;
        }
    }

    let detection = detect_license_document(&text)?;
    Some(license_file_facts(detection, base_facts))
}

pub(super) fn sniff_browser_license_file_type(
    path: &Path,
    name: &str,
    ext: &str,
    base_facts: FileFacts,
) -> Option<FileFacts> {
    let canonical_candidate = is_canonical_license_candidate_key(name);
    let can_sniff_content = can_sniff_license_content(base_facts);
    let can_sniff_markers = can_sniff_license_markers(ext, base_facts);

    if !canonical_candidate && !can_sniff_markers {
        return None;
    }
    if canonical_candidate && !can_sniff_content {
        return None;
    }

    let prefix = read_license_text_prefix(path, FAST_LICENSE_SNIFF_BYTE_LIMIT)?;
    if let Some(detection) = detect_spdx_identifier(&prefix) {
        return Some(license_file_facts(detection, base_facts));
    }

    if !canonical_candidate
        && (!has_strong_license_markers(&prefix) || !starts_like_standalone_license(&prefix))
    {
        return None;
    }

    if let Some(detection) = detect_license_document(&prefix) {
        return Some(license_file_facts(detection, base_facts));
    }

    let text = read_license_text(path)?;
    let detection = detect_license_document(&text)?;
    Some(license_file_facts(detection, base_facts))
}

fn can_sniff_license_content(base_facts: FileFacts) -> bool {
    matches!(
        base_facts.preview.kind,
        PreviewKind::PlainText | PreviewKind::Markdown
    ) && matches!(
        base_facts.builtin_class,
        FileClass::Document | FileClass::File
    )
}

pub(crate) fn is_canonical_license_file_name(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    is_canonical_license_candidate_key(&name)
}

fn is_canonical_license_candidate_key(name: &str) -> bool {
    const EXACT_CANDIDATES: &[&str] = &["license", "licence", "copying", "copyright", "unlicense"];
    const PREFIX_CANDIDATES: &[&str] = &["license", "licence", "copying", "copyright", "unlicense"];

    if EXACT_CANDIDATES.contains(&name) {
        return true;
    }

    PREFIX_CANDIDATES.iter().any(|candidate| {
        name.strip_prefix(candidate)
            .and_then(|suffix| suffix.chars().next())
            .is_some_and(|separator| matches!(separator, '.' | '_' | '-'))
    })
}

fn can_sniff_license_markers(ext: &str, base_facts: FileFacts) -> bool {
    matches!(
        ext,
        "" | "txt" | "md" | "markdown" | "mdown" | "mkd" | "mdx" | "rst"
    ) && can_sniff_license_content(base_facts)
}

fn read_license_text_prefix(path: &Path, byte_limit: usize) -> Option<String> {
    if !is_regular_file(path) {
        return None;
    }

    let mut file = File::open(path).ok()?;
    let mut buffer = vec![0_u8; byte_limit];
    let bytes_read = file.read(&mut buffer).ok()?;
    let bytes = &buffer[..bytes_read];
    if bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn read_license_text(path: &Path) -> Option<String> {
    read_license_text_prefix(path, LICENSE_SNIFF_BYTE_LIMIT)
}

fn is_regular_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

fn license_file_facts(detection: LicenseDetection, base_facts: FileFacts) -> FileFacts {
    FileFacts {
        builtin_class: FileClass::License,
        specific_type_label: Some(detection.detail_label()),
        preview: base_facts.preview,
    }
}

fn has_strong_license_markers(text: &str) -> bool {
    if detect_spdx_identifier(text).is_some() {
        return true;
    }

    let top_lines = text
        .lines()
        .take(LICENSE_MARKER_LINE_LIMIT)
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = normalize_license_text(&top_lines);
    let normalized_signature_text = normalize_high_signal_text(&top_lines);

    [
        "mit license",
        "apache license",
        "mozilla public license",
        "gnu general public license",
        "gnu lesser general public license",
        "gnu affero general public license",
        "bsd 2 clause license",
        "bsd 3 clause license",
        "the unlicense",
        "creative commons zero",
        "permission is hereby granted free of charge",
        "redistribution and use in source and binary forms",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
        || HIGH_SIGNAL_LICENSE_SIGNATURES.iter().any(|signature| {
            signature
                .top_markers
                .iter()
                .any(|marker| contains_phrase(&normalized_signature_text, marker))
        })
}

fn starts_like_standalone_license(text: &str) -> bool {
    let preamble = text
        .lines()
        .take(LICENSE_PREAMBLE_LINE_LIMIT)
        .filter_map(clean_license_preamble_line)
        .collect::<Vec<_>>()
        .join(" ");
    if preamble.is_empty() {
        return false;
    }

    let normalized = normalize_license_text(&preamble);
    if [
        "apache license",
        "mit license",
        "mozilla public license",
        "gnu general public license",
        "gnu lesser general public license",
        "gnu affero general public license",
        "the unlicense",
        "creative commons legal code",
        "cc0 1 0 universal",
        "bsd 2 clause license",
        "bsd 3 clause license",
        "isc license",
    ]
    .iter()
    .any(|title| normalized.starts_with(title))
    {
        return true;
    }

    let normalized_signature_text = normalize_high_signal_text(&preamble);
    HIGH_SIGNAL_LICENSE_SIGNATURES.iter().any(|signature| {
        signature
            .top_markers
            .iter()
            .any(|marker| starts_with_phrase(&normalized_signature_text, marker))
    })
}

fn clean_license_preamble_line(line: &str) -> Option<&str> {
    let cleaned = line
        .trim()
        .trim_start_matches(|ch: char| {
            ch.is_ascii_whitespace() || matches!(ch, '/' | '*' | '#' | ';' | '!' | '<' | '>' | '-')
        })
        .trim();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn detect_license_document(text: &str) -> Option<LicenseDetection> {
    if let Some(detection) = detect_spdx_identifier(text) {
        return Some(detection);
    }

    let normalized = normalize_license_text(text);
    detect_known_license(&normalized)
        .or_else(|| detect_high_signal_license(text))
        .or_else(|| looks_like_license_document(&normalized).then_some(LicenseDetection::Generic))
}

fn detect_spdx_identifier(text: &str) -> Option<LicenseDetection> {
    for line in text.lines().take(LICENSE_MARKER_LINE_LIMIT) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cleaned = trimmed
            .trim_start_matches(|ch: char| {
                ch.is_ascii_whitespace()
                    || matches!(ch, '/' | '*' | '#' | ';' | '!' | '<' | '>' | '-')
            })
            .trim();
        let lower = cleaned.to_ascii_lowercase();
        let Some(index) = lower.find("spdx-license-identifier:") else {
            continue;
        };
        let value = cleaned[index + "spdx-license-identifier:".len()..]
            .trim()
            .trim_end_matches(['*', '/', ';', '-', '>'])
            .trim();
        if value.is_empty() {
            return Some(LicenseDetection::Generic);
        }
        return Some(license_from_spdx(value));
    }
    None
}

fn license_from_spdx(value: &str) -> LicenseDetection {
    match normalize_spdx_expression(value).as_str() {
        "mit" => LicenseDetection::Specific {
            detail_label: "MIT License",
        },
        "apache-2.0" => LicenseDetection::Specific {
            detail_label: "Apache License 2.0",
        },
        "bsd-2-clause" => LicenseDetection::Specific {
            detail_label: "BSD 2-Clause License",
        },
        "bsd-3-clause" => LicenseDetection::Specific {
            detail_label: "BSD 3-Clause License",
        },
        "isc" => LicenseDetection::Specific {
            detail_label: "ISC License",
        },
        "mpl-2.0" => LicenseDetection::Specific {
            detail_label: "Mozilla Public License 2.0",
        },
        "unlicense" => LicenseDetection::Specific {
            detail_label: "The Unlicense",
        },
        "cc0-1.0" => LicenseDetection::Specific {
            detail_label: "CC0 1.0",
        },
        "cc-by-3.0" => LicenseDetection::Specific {
            detail_label: "Creative Commons Attribution 3.0",
        },
        "cc-by-3.0-at" => LicenseDetection::Specific {
            detail_label: "Creative Commons Attribution 3.0 Austria",
        },
        "cc-by-sa-2.1-jp" => LicenseDetection::Specific {
            detail_label: "Creative Commons Attribution-ShareAlike 2.1 Japan",
        },
        "cc-by-sa-3.0-at" => LicenseDetection::Specific {
            detail_label: "Creative Commons Attribution-ShareAlike 3.0 Austria",
        },
        "w3c" => LicenseDetection::Specific {
            detail_label: "W3C Software Notice and License",
        },
        "wtfpl" => LicenseDetection::Specific {
            detail_label: "WTFPL",
        },
        "gpl-2.0-only" => LicenseDetection::Specific {
            detail_label: "GNU GPL 2.0",
        },
        "gpl-2.0-or-later" => LicenseDetection::Specific {
            detail_label: "GNU GPL 2.0 or later",
        },
        "gpl-3.0-only" => LicenseDetection::Specific {
            detail_label: "GNU GPL 3.0",
        },
        "gpl-3.0-or-later" => LicenseDetection::Specific {
            detail_label: "GNU GPL 3.0 or later",
        },
        "lgpl-2.1-only" => LicenseDetection::Specific {
            detail_label: "GNU LGPL 2.1",
        },
        "lgpl-2.1-or-later" => LicenseDetection::Specific {
            detail_label: "GNU LGPL 2.1 or later",
        },
        "lgpl-3.0-only" => LicenseDetection::Specific {
            detail_label: "GNU LGPL 3.0",
        },
        "lgpl-3.0-or-later" => LicenseDetection::Specific {
            detail_label: "GNU LGPL 3.0 or later",
        },
        "agpl-3.0-only" => LicenseDetection::Specific {
            detail_label: "GNU AGPL 3.0",
        },
        "agpl-3.0-or-later" => LicenseDetection::Specific {
            detail_label: "GNU AGPL 3.0 or later",
        },
        "mit or apache-2.0" | "apache-2.0 or mit" => LicenseDetection::Specific {
            detail_label: "Dual MIT / Apache 2.0 license",
        },
        _ => LicenseDetection::Generic,
    }
}

fn normalize_spdx_expression(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_space = false;

    for ch in value
        .trim()
        .trim_matches(|ch| matches!(ch, '(' | ')'))
        .chars()
    {
        if ch.is_ascii_whitespace() {
            if !previous_space {
                normalized.push(' ');
                previous_space = true;
            }
            continue;
        }
        previous_space = false;
        normalized.push(ch.to_ascii_lowercase());
    }

    normalized.trim().to_string()
}

fn detect_known_license(normalized: &str) -> Option<LicenseDetection> {
    if contains_all(
        normalized,
        &["mozilla public license version 2 0", "mozilla org mpl 2 0"],
    ) {
        return Some(LicenseDetection::Specific {
            detail_label: "Mozilla Public License 2.0",
        });
    }
    if contains_all(
        normalized,
        &[
            "apache license version 2 0 january 2004",
            "terms and conditions for use reproduction and distribution",
            "apache org licenses license 2 0",
        ],
    ) {
        return Some(LicenseDetection::Specific {
            detail_label: "Apache License 2.0",
        });
    }
    if contains_all(
        normalized,
        &[
            "permission to use copy modify and or distribute this software for any purpose with or without fee is hereby granted",
            "the software is provided as is",
        ],
    ) {
        return Some(LicenseDetection::Specific {
            detail_label: "ISC License",
        });
    }
    if contains_all(
        normalized,
        &[
            "redistribution and use in source and binary forms with or without modification are permitted provided that the following conditions are met",
            "neither the name of",
        ],
    ) {
        return Some(LicenseDetection::Specific {
            detail_label: "BSD 3-Clause License",
        });
    }
    if contains_all(
        normalized,
        &[
            "redistribution and use in source and binary forms with or without modification are permitted provided that the following conditions are met",
            "this list of conditions and the following disclaimer",
        ],
    ) {
        return Some(LicenseDetection::Specific {
            detail_label: "BSD 2-Clause License",
        });
    }
    if contains_all(
        normalized,
        &[
            "permission is hereby granted free of charge to any person obtaining a copy",
            "the software is furnished to do so",
            "the software is provided as is",
        ],
    ) {
        return Some(LicenseDetection::Specific {
            detail_label: "MIT License",
        });
    }
    if contains_all(
        normalized,
        &[
            "this is free and unencumbered software released into the public domain",
            "for more information please refer to https unlicense org",
        ],
    ) {
        return Some(LicenseDetection::Specific {
            detail_label: "The Unlicense",
        });
    }
    if contains_all(
        normalized,
        &[
            "creative commons legal code",
            "cc0 1 0 universal",
            "no rights reserved",
        ],
    ) {
        return Some(LicenseDetection::Specific {
            detail_label: "CC0 1.0",
        });
    }
    if let Some(detail_label) = detect_gnu_license(normalized) {
        return Some(LicenseDetection::Specific { detail_label });
    }
    None
}

fn detect_high_signal_license(text: &str) -> Option<LicenseDetection> {
    let normalized = normalize_high_signal_text(text);
    for signature in HIGH_SIGNAL_LICENSE_SIGNATURES {
        if matches_signature(&normalized, signature) {
            return Some(LicenseDetection::Specific {
                detail_label: signature.detail_label,
            });
        }
    }
    None
}

fn detect_gnu_license(normalized: &str) -> Option<&'static str> {
    if normalized.contains("gnu affero general public license") {
        return Some(if has_or_later_language(normalized) {
            "GNU AGPL 3.0 or later"
        } else {
            "GNU AGPL 3.0"
        });
    }
    if normalized.contains("gnu lesser general public license") {
        if normalized.contains("version 2 1") {
            return Some(if has_or_later_language(normalized) {
                "GNU LGPL 2.1 or later"
            } else {
                "GNU LGPL 2.1"
            });
        }
        if normalized.contains("version 3") {
            return Some(if has_or_later_language(normalized) {
                "GNU LGPL 3.0 or later"
            } else {
                "GNU LGPL 3.0"
            });
        }
    }
    if normalized.contains("gnu general public license") {
        if normalized.contains("version 2") {
            return Some(if has_or_later_language(normalized) {
                "GNU GPL 2.0 or later"
            } else {
                "GNU GPL 2.0"
            });
        }
        if normalized.contains("version 3") {
            return Some(if has_or_later_language(normalized) {
                "GNU GPL 3.0 or later"
            } else {
                "GNU GPL 3.0"
            });
        }
    }
    None
}

fn has_or_later_language(normalized: &str) -> bool {
    normalized.contains("any later version")
        || normalized.contains("or any later version")
        || normalized.contains("or later")
}

fn looks_like_license_document(normalized: &str) -> bool {
    let markers = [
        "copyright",
        "licensed under",
        "all rights reserved",
        "warranty",
        "liability",
        "permission is hereby granted",
        "redistribution and use in source and binary forms",
        "public domain",
        "terms and conditions for use reproduction and distribution",
        "mozilla public license",
        "gnu general public license",
        "apache license",
        "mit license",
        "the unlicense",
    ];
    markers
        .iter()
        .filter(|marker| normalized.contains(**marker))
        .count()
        >= 2
}

fn contains_all(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().all(|needle| haystack.contains(needle))
}

fn contains_phrase(normalized_text: &str, phrase: &str) -> bool {
    let needle = normalize_high_signal_text(phrase);
    !needle.is_empty() && normalized_text.contains(&needle)
}

fn starts_with_phrase(normalized_text: &str, phrase: &str) -> bool {
    let needle = normalize_high_signal_text(phrase);
    !needle.is_empty() && normalized_text.starts_with(&needle)
}

fn matches_signature(normalized_text: &str, signature: &HighSignalLicenseSignature) -> bool {
    signature
        .required_phrases
        .iter()
        .all(|phrase| contains_phrase(normalized_text, phrase))
        && signature
            .forbidden_phrases
            .iter()
            .all(|phrase| !contains_phrase(normalized_text, phrase))
}

fn normalize_license_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut previous_space = true;

    for ch in text.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            normalized.push(lower);
            previous_space = false;
        } else if !previous_space {
            normalized.push(' ');
            previous_space = true;
        }
    }

    normalized.trim().to_string()
}

fn normalize_high_signal_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut previous_space = true;

    for ch in text.chars() {
        for lower in ch.to_lowercase() {
            if lower.is_alphanumeric() {
                normalized.push(lower);
                previous_space = false;
            } else if !previous_space {
                normalized.push(' ');
                previous_space = true;
            }
        }
    }

    normalized.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{EntryKind, FileClass};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("elio-license-{label}-{unique}"))
    }

    fn write_temp_file(label: &str, file_name: &str, contents: &str) -> (PathBuf, PathBuf) {
        let root = temp_path(label);
        fs::create_dir_all(&root).expect("failed to create temp root");
        let path = root.join(file_name);
        fs::write(&path, contents).expect("failed to write temp file");
        (root, path)
    }

    #[test]
    fn canonical_license_names_do_not_override_source_classification() {
        let (root, path) = write_temp_file(
            "license-rust-source",
            "license.rs",
            "// SPDX-License-Identifier: MPL-2.0\npub fn license() {}\n",
        );

        let facts = super::super::classify::inspect_path(&path, EntryKind::File);

        assert_eq!(facts.builtin_class, FileClass::Code);
        assert_eq!(facts.specific_type_label, Some("Rust source file"));
        assert_eq!(facts.preview.kind, PreviewKind::Source);

        fs::remove_dir_all(root).expect("failed to remove temp root");
    }
}

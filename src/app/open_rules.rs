use std::{ffi::OsString, path::PathBuf};

use crate::{
    config::{self, OpenPlatform, OpenRule, OpenTargetType},
    core::{Entry, EntryKind, FileClass},
    file_info::{self, PreviewKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) enum OpenPlan {
    System { paths: Vec<PathBuf> },
    Detached { program: String, args: Vec<String> },
    Terminal { program: String, args: Vec<String> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommandTemplate {
    program: String,
    args: Vec<String>,
    has_path_placeholder: bool,
}

pub(in crate::app) fn plans_for_entries(entries: &[Entry]) -> Result<Vec<OpenPlan>, String> {
    let mut plans = Vec::new();
    let current_platform = OpenPlatform::current();
    for entry in entries {
        let Some(rule) = matching_rule(entry, current_platform) else {
            push_system(&mut plans, entry.path.clone());
            continue;
        };
        let command = command_template(&rule.command)?;
        if command.has_path_placeholder {
            let args = expand_args(&command.args, std::slice::from_ref(&entry.path));
            push_command_plan(&mut plans, rule.terminal, command.program, args, false);
        } else {
            let mut args = command.args;
            args.push(entry.path.to_string_lossy().into_owned());
            push_command_plan(&mut plans, rule.terminal, command.program, args, true);
        }
    }
    Ok(plans)
}

fn matching_rule(entry: &Entry, platform: OpenPlatform) -> Option<&'static OpenRule> {
    config::open()
        .rules
        .iter()
        .find(|rule| rule_matches(rule, entry, platform))
}

fn rule_matches(rule: &OpenRule, entry: &Entry, platform: OpenPlatform) -> bool {
    (rule.platforms.is_empty() || rule.platforms.contains(&platform))
        && (rule.exts.is_empty() || entry_ext_matches(entry, &rule.exts))
        && (rule.types.is_empty() || entry_type_matches(entry, &rule.types))
}

fn entry_ext_matches(entry: &Entry, exts: &[String]) -> bool {
    let Some(ext) = entry
        .path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
    else {
        return false;
    };
    exts.iter().any(|candidate| candidate == &ext)
}

fn entry_type_matches(entry: &Entry, types: &[OpenTargetType]) -> bool {
    let facts = file_info::inspect_entry_cached(entry);
    types
        .iter()
        .any(|target_type| entry_has_type(entry, facts, *target_type))
}

fn entry_has_type(entry: &Entry, facts: file_info::FileFacts, target_type: OpenTargetType) -> bool {
    match target_type {
        OpenTargetType::Folder => entry.kind == EntryKind::Directory,
        OpenTargetType::Text => entry_is_text_like(entry, facts),
        OpenTargetType::Code => facts.builtin_class == FileClass::Code,
        OpenTargetType::Config => facts.builtin_class == FileClass::Config,
        OpenTargetType::Document => facts.builtin_class == FileClass::Document,
        OpenTargetType::Image => facts.builtin_class == FileClass::Image,
        OpenTargetType::Audio => facts.builtin_class == FileClass::Audio,
        OpenTargetType::Video => facts.builtin_class == FileClass::Video,
        OpenTargetType::Archive => facts.builtin_class == FileClass::Archive,
        OpenTargetType::Font => facts.builtin_class == FileClass::Font,
        OpenTargetType::Data => facts.builtin_class == FileClass::Data,
        OpenTargetType::File => facts.builtin_class == FileClass::File,
    }
}

fn entry_is_text_like(entry: &Entry, facts: file_info::FileFacts) -> bool {
    if entry.kind == EntryKind::Directory {
        return false;
    }
    if matches!(
        facts.builtin_class,
        FileClass::Code | FileClass::Config | FileClass::License
    ) {
        return true;
    }
    match facts.preview.kind {
        PreviewKind::Markdown | PreviewKind::Csv | PreviewKind::Source => true,
        PreviewKind::PlainText => {
            facts.preview.document_format.is_none()
                && !matches!(
                    facts.builtin_class,
                    FileClass::Image
                        | FileClass::Audio
                        | FileClass::Video
                        | FileClass::Archive
                        | FileClass::Font
                )
        }
        PreviewKind::Sqlite
        | PreviewKind::SqliteCandidate
        | PreviewKind::Iso
        | PreviewKind::Torrent => false,
    }
}

fn command_template(command: &str) -> Result<CommandTemplate, String> {
    command_template_with_env(command, config::invoking_user_env_var)
}

fn command_template_with_env(
    command: &str,
    env_var: impl FnMut(&'static str) -> Option<OsString>,
) -> Result<CommandTemplate, String> {
    let command = command.trim();
    let tokens = command_tokens(command, env_var)?;
    let mut tokens = tokens.into_iter();
    let Some(program) = tokens.next() else {
        return Err("Open command is empty".to_string());
    };
    let args: Vec<String> = tokens.collect();
    let has_path_placeholder = args.iter().any(|arg| arg.contains("{path}"));
    Ok(CommandTemplate {
        program,
        args,
        has_path_placeholder,
    })
}

fn command_tokens(
    command: &str,
    mut env_var: impl FnMut(&'static str) -> Option<OsString>,
) -> Result<Vec<String>, String> {
    let mut tokens = tokenize_command(command);
    let Some(first) = tokens.first().map(String::as_str) else {
        return Ok(tokens);
    };
    let Some(env_key) = editor_env_key(first) else {
        return Ok(tokens);
    };

    let mut expanded = editor_env_tokens(env_key, env_var(env_key))?;
    expanded.extend(tokens.drain(1..));
    Ok(expanded)
}

fn editor_env_key(token: &str) -> Option<&'static str> {
    match token {
        "$EDITOR" => Some("EDITOR"),
        "$VISUAL" => Some("VISUAL"),
        _ => None,
    }
}

fn editor_env_tokens(key: &'static str, value: Option<OsString>) -> Result<Vec<String>, String> {
    let Some(value) = value.and_then(|value| value.into_string().ok()) else {
        return Err(format!("${key} is not set"));
    };
    let tokens = tokenize_command(&value);
    if tokens.is_empty() {
        return Err(format!("${key} is not set"));
    }
    Ok(tokens)
}

pub(in crate::app) fn tokenize_command(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quotes = false;
    let mut in_double_quotes = false;
    let mut chars = command.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double_quotes => in_single_quotes = !in_single_quotes,
            '"' if !in_single_quotes => in_double_quotes = !in_double_quotes,
            '\\' => match chars.peek().copied() {
                Some(next) if matches!(next, ' ' | '\t' | '\'' | '"' | '\\') => {
                    chars.next();
                    current.push(next);
                }
                _ => current.push(ch),
            },
            ' ' | '\t' if !in_single_quotes && !in_double_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }
    coalesce_windows_executable_path(tokens)
}

fn coalesce_windows_executable_path(tokens: Vec<String>) -> Vec<String> {
    if tokens.len() < 2 || !looks_like_windows_path_start(&tokens[0]) {
        return tokens;
    }

    let mut program = String::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            program.push(' ');
        }
        program.push_str(token);

        if looks_like_windows_executable_path(&program) {
            let mut merged = Vec::with_capacity(tokens.len() - index);
            merged.push(program);
            merged.extend(tokens[index + 1..].iter().cloned());
            return merged;
        }
    }

    tokens
}

fn looks_like_windows_path_start(token: &str) -> bool {
    let bytes = token.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || token.starts_with(r"\\")
}

fn looks_like_windows_executable_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    [".exe", ".cmd", ".bat", ".com"]
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

fn expand_args(args: &[String], paths: &[PathBuf]) -> Vec<String> {
    let mut expanded = Vec::new();
    for arg in args {
        if arg == "{path}" {
            expanded.extend(paths.iter().map(|path| path.to_string_lossy().into_owned()));
        } else if arg.contains("{path}") {
            for path in paths {
                expanded.push(arg.replace("{path}", &path.to_string_lossy()));
            }
        } else {
            expanded.push(arg.clone());
        }
    }
    expanded
}

fn push_system(plans: &mut Vec<OpenPlan>, path: PathBuf) {
    if let Some(OpenPlan::System { paths }) = plans.last_mut() {
        paths.push(path);
    } else {
        plans.push(OpenPlan::System { paths: vec![path] });
    }
}

fn push_command_plan(
    plans: &mut Vec<OpenPlan>,
    terminal: bool,
    program: String,
    args: Vec<String>,
    merge_append_path: bool,
) {
    if !merge_append_path {
        if terminal {
            plans.push(OpenPlan::Terminal { program, args });
        } else {
            plans.push(OpenPlan::Detached { program, args });
        }
        return;
    }

    if let Some(last) = plans.last_mut() {
        let args_prefix = args[..args.len().saturating_sub(1)].to_vec();
        match last {
            OpenPlan::Terminal {
                program: last_program,
                args: last_args,
            } if terminal && last_program == &program && last_args.starts_with(&args_prefix) => {
                last_args.extend(args.last().cloned());
                return;
            }
            OpenPlan::Detached {
                program: last_program,
                args: last_args,
            } if !terminal && last_program == &program && last_args.starts_with(&args_prefix) => {
                last_args.extend(args.last().cloned());
                return;
            }
            _ => {}
        }
    }

    if terminal {
        plans.push(OpenPlan::Terminal { program, args });
    } else {
        plans.push(OpenPlan::Detached { program, args });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_quoted_command() {
        assert_eq!(
            tokenize_command("open -a \"Preview App\" {path}"),
            vec!["open", "-a", "Preview App", "{path}"]
        );
    }

    #[test]
    fn preserves_backslashes_that_are_not_command_escapes() {
        assert_eq!(
            tokenize_command(r#"echo C:\tmp\notes.txt"#),
            vec!["echo", r#"C:\tmp\notes.txt"#]
        );
    }

    #[test]
    fn still_supports_escaped_unix_whitespace() {
        assert_eq!(
            tokenize_command(r#"my\ command --flag"#),
            vec!["my command", "--flag"]
        );
    }

    #[test]
    fn tokenizes_quoted_windows_command_path() {
        assert_eq!(
            tokenize_command(r#""C:\Program Files\SumatraPDF\SumatraPDF.exe" -reuse-instance"#),
            vec![
                r#"C:\Program Files\SumatraPDF\SumatraPDF.exe"#,
                "-reuse-instance"
            ]
        );
    }

    #[test]
    fn tokenizes_unquoted_windows_command_path_with_spaces() {
        assert_eq!(
            tokenize_command(r#"C:\Program Files\SumatraPDF\SumatraPDF.exe -reuse-instance"#),
            vec![
                r#"C:\Program Files\SumatraPDF\SumatraPDF.exe"#,
                "-reuse-instance"
            ]
        );
    }

    #[test]
    fn tokenizes_windows_forward_slash_command_path_with_spaces() {
        assert_eq!(
            tokenize_command(r#"C:/Program Files/SumatraPDF/SumatraPDF.exe -reuse-instance"#),
            vec![
                "C:/Program Files/SumatraPDF/SumatraPDF.exe",
                "-reuse-instance"
            ]
        );
    }

    #[test]
    fn expands_embedded_path_placeholder_once_per_path() {
        let paths = vec![PathBuf::from("a.md"), PathBuf::from("b.md")];
        assert_eq!(
            expand_args(&["--file={path}".to_string()], &paths),
            vec!["--file=a.md", "--file=b.md"]
        );
    }

    #[test]
    fn editor_command_uses_editor_even_when_visual_is_set() {
        let template = command_template_with_env("$EDITOR --cmd {path}", |key| match key {
            "EDITOR" => Some(OsString::from("nvim --clean")),
            "VISUAL" => Some(OsString::from("hx")),
            _ => None,
        })
        .expect("$EDITOR should expand");

        assert_eq!(template.program, "nvim");
        assert_eq!(template.args, vec!["--clean", "--cmd", "{path}"]);
    }

    #[test]
    fn visual_command_uses_visual_even_when_editor_is_set() {
        let template = command_template_with_env("$VISUAL", |key| match key {
            "EDITOR" => Some(OsString::from("nvim")),
            "VISUAL" => Some(OsString::from("code --wait")),
            _ => None,
        })
        .expect("$VISUAL should expand");

        assert_eq!(template.program, "code");
        assert_eq!(template.args, vec!["--wait"]);
    }

    #[test]
    fn editor_command_does_not_fall_back_to_visual() {
        let error = command_template_with_env("$EDITOR", |key| match key {
            "VISUAL" => Some(OsString::from("hx")),
            _ => None,
        })
        .expect_err("$EDITOR should be required");

        assert_eq!(error, "$EDITOR is not set");
    }

    #[test]
    fn appends_paths_to_matching_command_group() {
        let mut plans = Vec::new();
        push_command_plan(
            &mut plans,
            true,
            "hx".to_string(),
            vec!["--foo".to_string(), "a.md".to_string()],
            true,
        );
        push_command_plan(
            &mut plans,
            true,
            "hx".to_string(),
            vec!["--foo".to_string(), "b.md".to_string()],
            true,
        );
        assert_eq!(
            plans,
            vec![OpenPlan::Terminal {
                program: "hx".to_string(),
                args: vec!["--foo".to_string(), "a.md".to_string(), "b.md".to_string()],
            }]
        );
    }
}

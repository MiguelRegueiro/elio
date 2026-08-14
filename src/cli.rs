use crate::shell_integration::{self, Shell, ShellIntegrationAction};
use anyhow::Result;
use std::{
    env,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

const RUN_USAGE: &str = "Usage: elio [OPTIONS] [PATH]";

pub(crate) fn run() -> Result<ExitCode> {
    match parse_args(env::args().skip(1))? {
        Command::Run {
            options,
            chooser_file,
            start_focus,
            reveal_hidden_start_focus,
        } => elio::run_with_startup_options(
            options,
            start_focus,
            reveal_hidden_start_focus,
            chooser_file,
        )
        .map(run_outcome_exit_code),
        Command::PrintVersion => {
            print_version();
            Ok(ExitCode::SUCCESS)
        }
        Command::PrintHelp => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        Command::UserFsHelper => elio::run_user_fs_helper().map(|()| ExitCode::SUCCESS),
        Command::PrintShellInit(shell) => {
            let executable = env::current_exe()?;
            let invocation = env::args().next();
            let binary =
                shell_integration::binary_command(shell, invocation.as_deref(), &executable);
            print!("{}", shell_integration::init_script(shell, &binary));
            Ok(ExitCode::SUCCESS)
        }
        Command::InstallShellIntegration(shell) => {
            let executable = env::current_exe()?;
            let invocation = env::args().next();
            let shell = match shell {
                Some(shell) => shell,
                None => shell_integration::detect_shell(ShellIntegrationAction::Install)?,
            };
            let binary =
                shell_integration::binary_command(shell, invocation.as_deref(), &executable);
            let report = shell_integration::install(shell, &binary)?;
            println!(
                "Installed elio shell integration for {}.",
                report.shell.name()
            );
            println!();
            println!("Wrote: {}", report.path.display());
            println!();
            println!("Restart your shell, or run:");
            println!("  {}", report.reload_command);
            println!();
            println!("From now on, `elio` will change your shell directory on quit.");
            Ok(ExitCode::SUCCESS)
        }
        Command::UninstallShellIntegration(shell) => {
            let shell = match shell {
                Some(shell) => shell,
                None => shell_integration::detect_shell(ShellIntegrationAction::Uninstall)?,
            };
            let report = shell_integration::uninstall(shell)?;
            println!(
                "Uninstalled elio shell integration for {}.",
                report.shell.name()
            );
            println!();
            if report.changed {
                if report.removed_file {
                    println!("Removed: {}", report.path.display());
                } else {
                    println!("Updated: {}", report.path.display());
                }
            } else {
                println!("No integration found at: {}", report.path.display());
            }
            println!();
            println!("Restart your shell, or run:");
            println!("  {}", report.reload_command);
            println!();
            println!("From now on, `elio` will leave your shell directory unchanged.");
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn run_outcome_exit_code(outcome: elio::RunOutcome) -> ExitCode {
    match outcome {
        elio::RunOutcome::Success => ExitCode::SUCCESS,
        elio::RunOutcome::Cancelled => ExitCode::FAILURE,
    }
}

#[derive(Debug)]
enum Command {
    Run {
        options: elio::RunOptions,
        chooser_file: Option<PathBuf>,
        start_focus: Option<PathBuf>,
        reveal_hidden_start_focus: bool,
    },
    PrintVersion,
    PrintHelp,
    PrintShellInit(Shell),
    InstallShellIntegration(Option<Shell>),
    UninstallShellIntegration(Option<Shell>),
    UserFsHelper,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command> {
    let args = args.into_iter().collect::<Vec<_>>();

    if args.is_empty() {
        return Ok(Command::Run {
            options: elio::RunOptions::default(),
            chooser_file: None,
            start_focus: None,
            reveal_hidden_start_focus: false,
        });
    }

    match args.as_slice() {
        [arg] if arg == "--internal-user-fs-helper" => return Ok(Command::UserFsHelper),
        [arg] if arg == "--version" || arg == "-V" => return Ok(Command::PrintVersion),
        [arg] if arg == "--help" || arg == "-h" => return Ok(Command::PrintHelp),
        [arg, unexpected, ..] if arg == "--version" || arg == "-V" => {
            return Err(anyhow::anyhow!(unknown_argument_message(unexpected)));
        }
        [arg, unexpected, ..] if arg == "--help" || arg == "-h" => {
            return Err(anyhow::anyhow!(unknown_argument_message(unexpected)));
        }
        [command, subcommand, shell] if command == "shell" && subcommand == "init" => {
            return Shell::parse(shell)
                .map(Command::PrintShellInit)
                .map_err(anyhow::Error::msg);
        }
        [command, subcommand] if command == "shell" && subcommand == "install" => {
            return Ok(Command::InstallShellIntegration(None));
        }
        [command, subcommand, shell] if command == "shell" && subcommand == "install" => {
            return Shell::parse(shell)
                .map(|shell| Command::InstallShellIntegration(Some(shell)))
                .map_err(anyhow::Error::msg);
        }
        [command, subcommand] if command == "shell" && subcommand == "uninstall" => {
            return Ok(Command::UninstallShellIntegration(None));
        }
        [command, subcommand, shell] if command == "shell" && subcommand == "uninstall" => {
            return Shell::parse(shell)
                .map(|shell| Command::UninstallShellIntegration(Some(shell)))
                .map_err(anyhow::Error::msg);
        }
        [command, subcommand, _shell, unexpected, ..]
            if command == "shell" && subcommand == "install" =>
        {
            return Err(anyhow::anyhow!(
                unknown_argument_message(unexpected)
                    .replace(RUN_USAGE, "Usage: elio shell install [SHELL]")
            ));
        }
        [command, subcommand, _shell, unexpected, ..]
            if command == "shell" && subcommand == "uninstall" =>
        {
            return Err(anyhow::anyhow!(
                unknown_argument_message(unexpected)
                    .replace(RUN_USAGE, "Usage: elio shell uninstall [SHELL]")
            ));
        }
        [command, subcommand, _shell, unexpected, ..]
            if command == "shell" && subcommand == "init" =>
        {
            return Err(anyhow::anyhow!(
                unknown_argument_message(unexpected)
                    .replace(RUN_USAGE, "Usage: elio shell init <SHELL>")
            ));
        }
        [command, subcommand] if command == "shell" && subcommand == "init" => {
            return Err(anyhow::anyhow!(
                "error: expected a shell after 'elio shell init'\n\nsupported shells: bash, zsh, fish, nu"
            ));
        }
        [command, ..] if command == "shell" => {
            return Err(anyhow::anyhow!(
                "error: expected subcommand 'init', 'install', or 'uninstall' after 'elio shell'\n\nUsage: elio shell init <SHELL>\n       elio shell install [SHELL]\n       elio shell uninstall [SHELL]"
            ));
        }
        _ => {}
    }

    parse_run_args(args)
}

fn parse_run_args(args: Vec<String>) -> Result<Command> {
    let mut start_dir = None;
    let mut start_focus = None;
    let mut reveal_hidden_start_focus = false;
    let mut cwd_file = None;
    let mut chooser_file = None;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if path_option_matches(arg, "--cwd-file") {
            if cwd_file.is_some() {
                return Err(anyhow::anyhow!(
                    "error: '--cwd-file' cannot be used more than once\n\n{RUN_USAGE}"
                ));
            }
            let (file, next_index) = take_path_option_value(&args, index, "--cwd-file")?;
            cwd_file = Some(file);
            index = next_index;
            continue;
        }

        if path_option_matches(arg, "--chooser-file") {
            if chooser_file.is_some() {
                return Err(anyhow::anyhow!(
                    "error: '--chooser-file' cannot be used more than once\n\n{RUN_USAGE}"
                ));
            }
            let (file, next_index) = take_path_option_value(&args, index, "--chooser-file")?;
            chooser_file = Some(file);
            index = next_index;
            continue;
        }

        if arg.starts_with('-') {
            return Err(anyhow::anyhow!(unknown_argument_message(arg)));
        }

        if start_dir.is_some() {
            return Err(anyhow::anyhow!(unknown_argument_message(arg)));
        }
        let startup_path = resolve_startup_path(arg)?;
        start_dir = Some(startup_path.start_dir);
        start_focus = startup_path.start_focus;
        reveal_hidden_start_focus = startup_path.reveal_hidden_start_focus;
        index += 1;
    }

    Ok(Command::Run {
        options: elio::RunOptions {
            start_dir,
            cwd_file,
        },
        chooser_file,
        start_focus,
        reveal_hidden_start_focus,
    })
}

fn path_option_matches(arg: &str, flag: &str) -> bool {
    arg == flag
        || arg
            .strip_prefix(flag)
            .is_some_and(|suffix| suffix.starts_with('='))
}

fn take_path_option_value(
    args: &[String],
    index: usize,
    flag: &'static str,
) -> Result<(PathBuf, usize)> {
    let arg = &args[index];
    let inline_prefix = format!("{flag}=");
    if let Some(file) = arg.strip_prefix(&inline_prefix) {
        if file.is_empty() {
            return Err(anyhow::anyhow!(
                "error: expected a file path after '{flag}'\n\n{RUN_USAGE}"
            ));
        }
        return Ok((PathBuf::from(file), index + 1));
    }

    let Some(file) = args.get(index + 1) else {
        return Err(anyhow::anyhow!(
            "error: expected a file path after '{flag}'\n\n{RUN_USAGE}"
        ));
    };
    Ok((PathBuf::from(file), index + 2))
}

fn print_version() {
    println!("elio {}", env!("CARGO_PKG_VERSION"));
}

fn print_help() {
    println!("elio {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("{RUN_USAGE}");
    println!("       elio shell init <SHELL>");
    println!("       elio shell install [SHELL]");
    println!("       elio shell uninstall [SHELL]");
    println!();
    println!("Arguments:");
    println!(
        "  [PATH]               Start in a directory, or focus a file in its parent directory"
    );
    println!();
    println!("Options:");
    println!("      --chooser-file FILE  Write chosen paths to FILE, or stdout with '-'");
    println!("      --cwd-file FILE  Write the final current directory to FILE on exit");
    println!("  -h, --help           Print help");
    println!("  -V, --version        Print version");
    println!();
    println!("Commands:");
    println!("  shell init <SHELL>        Print shell integration for bash, zsh, fish, or nu");
    println!("  shell install [SHELL]    Install shell integration for bash, zsh, fish, or nu");
    println!("  shell uninstall [SHELL]  Remove shell integration for bash, zsh, fish, or nu");
}

#[derive(Debug, Eq, PartialEq)]
struct StartupPath {
    start_dir: PathBuf,
    start_focus: Option<PathBuf>,
    reveal_hidden_start_focus: bool,
}

fn resolve_startup_path(arg: &str) -> Result<StartupPath> {
    let path = PathBuf::from(arg);
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() => Ok(StartupPath {
            start_dir: path.canonicalize().unwrap_or(path),
            start_focus: None,
            reveal_hidden_start_focus: false,
        }),
        Ok(_) => resolve_startup_file(&path),
        Err(error) => {
            if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
                return resolve_startup_file(&path);
            }
            Err(startup_path_error(&path, &error))
        }
    }
}

fn resolve_startup_file(path: &Path) -> Result<StartupPath> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Cannot open \"{}\": no file name", path.display()))?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let start_dir = parent
        .canonicalize()
        .map_err(|error| startup_path_error(parent, &error))?;
    Ok(StartupPath {
        start_focus: Some(start_dir.join(file_name)),
        start_dir,
        reveal_hidden_start_focus: should_reveal_hidden_start_focus(path, file_name),
    })
}

fn should_reveal_hidden_start_focus(path: &Path, file_name: &OsStr) -> bool {
    file_name.to_string_lossy().starts_with('.') || has_hidden_attribute(path)
}

#[cfg(windows)]
fn has_hidden_attribute(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn has_hidden_attribute(_path: &Path) -> bool {
    false
}

fn startup_path_error(path: &Path, error: &io::Error) -> anyhow::Error {
    let detail = match error.kind() {
        io::ErrorKind::NotFound => "no such file or directory".to_string(),
        io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => error.to_string(),
    };
    anyhow::anyhow!("Cannot open \"{}\": {detail}", path.display())
}

fn unknown_argument_message(arg: &str) -> String {
    let mut message = format!("error: unexpected argument '{arg}' found");

    if arg != "--version" && arg != "-V" && ("--version".starts_with(arg) || "-V".starts_with(arg))
    {
        message.push_str("\n\n  tip: a similar argument exists: '--version'");
    } else if arg != "--help" && arg != "-h" && ("--help".starts_with(arg) || "-h".starts_with(arg))
    {
        message.push_str("\n\n  tip: a similar argument exists: '--help'");
    } else if arg != "--cwd-file" && "--cwd-file".starts_with(arg) {
        message.push_str("\n\n  tip: a similar argument exists: '--cwd-file'");
    } else if arg != "--chooser-file" && "--chooser-file".starts_with(arg) {
        message.push_str("\n\n  tip: a similar argument exists: '--chooser-file'");
    }

    message.push_str("\n\n");
    message.push_str(RUN_USAGE);
    message.push_str("\n\nFor more information, try '--help'.");
    message
}

#[cfg(test)]
mod tests {
    use super::{StartupPath, resolve_startup_path};
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
        std::env::temp_dir().join(format!("elio-cli-{label}-{unique}"))
    }

    #[test]
    fn resolve_startup_path_accepts_existing_directory() {
        let root = temp_path("directory");
        fs::create_dir_all(&root).expect("temp directory should be created");

        let resolved = resolve_startup_path(root.to_str().expect("temp path should be utf-8"))
            .expect("existing directory should resolve");

        assert_eq!(
            resolved,
            StartupPath {
                start_dir: root
                    .canonicalize()
                    .expect("temp directory should canonicalize successfully"),
                start_focus: None,
                reveal_hidden_start_focus: false,
            }
        );

        fs::remove_dir_all(root).expect("temp directory should be removed");
    }

    #[test]
    fn resolve_startup_path_accepts_existing_file() {
        let root = temp_path("file");
        fs::create_dir_all(&root).expect("temp directory should be created");
        let file = root.join("notes.txt");
        fs::write(&file, "hello").expect("temp file should be created");

        let resolved =
            resolve_startup_path(file.to_str().expect("temp path should be valid utf-8"))
                .expect("file path should resolve");
        let canonical_root = root
            .canonicalize()
            .expect("temp directory should canonicalize successfully");

        assert_eq!(
            resolved,
            StartupPath {
                start_dir: canonical_root.clone(),
                start_focus: Some(canonical_root.join("notes.txt")),
                reveal_hidden_start_focus: false,
            }
        );

        fs::remove_dir_all(root).expect("temp directory should be removed");
    }

    #[test]
    fn resolve_startup_path_reveals_hidden_file_targets() {
        let root = temp_path("hidden-file");
        fs::create_dir_all(&root).expect("temp directory should be created");
        let file = root.join(".env");
        fs::write(&file, "secret").expect("temp file should be created");

        let resolved =
            resolve_startup_path(file.to_str().expect("temp path should be valid utf-8"))
                .expect("hidden file path should resolve");
        let canonical_root = root
            .canonicalize()
            .expect("temp directory should canonicalize successfully");

        assert_eq!(
            resolved,
            StartupPath {
                start_dir: canonical_root.clone(),
                start_focus: Some(canonical_root.join(".env")),
                reveal_hidden_start_focus: true,
            }
        );

        fs::remove_dir_all(root).expect("temp directory should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_startup_path_focuses_file_symlink_itself() {
        use std::os::unix::fs::symlink;

        let root = temp_path("file-symlink");
        fs::create_dir_all(&root).expect("temp directory should be created");
        let target = root.join("target.txt");
        let link = root.join("link.txt");
        fs::write(&target, "target").expect("target file should be created");
        symlink(&target, &link).expect("file symlink should be created");

        let resolved =
            resolve_startup_path(link.to_str().expect("temp path should be valid utf-8"))
                .expect("file symlink should resolve");
        let canonical_root = root
            .canonicalize()
            .expect("temp directory should canonicalize successfully");

        assert_eq!(
            resolved,
            StartupPath {
                start_dir: canonical_root.clone(),
                start_focus: Some(canonical_root.join("link.txt")),
                reveal_hidden_start_focus: false,
            }
        );

        fs::remove_dir_all(root).expect("temp directory should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_startup_path_focuses_broken_symlinks() {
        use std::os::unix::fs::symlink;

        let root = temp_path("broken-symlink");
        fs::create_dir_all(&root).expect("temp directory should be created");
        let missing_target = root.join("missing.txt");
        let link = root.join("broken.txt");
        symlink(&missing_target, &link).expect("broken symlink should be created");

        let resolved =
            resolve_startup_path(link.to_str().expect("temp path should be valid utf-8"))
                .expect("broken symlink should resolve");
        let canonical_root = root
            .canonicalize()
            .expect("temp directory should canonicalize successfully");

        assert_eq!(
            resolved,
            StartupPath {
                start_dir: canonical_root.clone(),
                start_focus: Some(canonical_root.join("broken.txt")),
                reveal_hidden_start_focus: false,
            }
        );

        fs::remove_dir_all(root).expect("temp directory should be removed");
    }
}

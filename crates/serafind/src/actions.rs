//! What can be done with a result: open it, show it in its folder, open a terminal there. Done
//! by the resident process, so that the bar can close at once.

use std::path::{Path, PathBuf};
use std::process::Command;

use serafind_ipc::Action;

/// Does `action` on `path`.
///
/// # Errors
/// What went wrong, to show to the user.
pub fn perform(action: Action, path: &Path, terminal: &str, token: Option<&str>) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("{} no longer exists", path.display()));
    }
    match action {
        Action::Open => open(path, token),
        Action::Reveal => reveal(path, token),
        Action::Terminal => open_terminal(&folder_of(path), terminal, token),
    }
}

/// The folder itself, or the folder of a file.
fn folder_of(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf)
    }
}

/// Opens with the default application.
fn open(path: &Path, token: Option<&str>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        match portal::open(path, token) {
            Ok(()) => return Ok(()),
            Err(error) => eprintln!("serafind: the portal cannot open {}: {error}", path.display()),
        }
        spawn(Command::new("xdg-open").arg(path), token)
    }
    #[cfg(windows)]
    {
        let _ = token;
        spawn(Command::new("explorer").arg(path), None)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = token;
        spawn(Command::new("open").arg(path), None)
    }
}

/// Shows in the file manager, selected in its folder.
fn reveal(path: &Path, token: Option<&str>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        match portal::reveal(path, token) {
            Ok(()) => return Ok(()),
            Err(error) => eprintln!("serafind: the portal cannot show {}: {error}", path.display()),
        }
        let parent = path.parent().unwrap_or(path);
        spawn(Command::new("xdg-open").arg(parent), token)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = token;
        spawn(
            Command::new("explorer").raw_arg(format!("/select,\"{}\"", path.display())),
            None,
        )
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = token;
        spawn(Command::new("open").arg("-R").arg(path), None)
    }
}

/// Opens a terminal in `dir`.
fn open_terminal(dir: &Path, setting: &str, token: Option<&str>) -> Result<(), String> {
    let Some(mut command) = terminal_command(setting, dir, &|program| find_program(program).is_some()) else {
        return Err(
            "no terminal found: set one in config.toml ([actions] terminal = \"my-terminal\")".to_owned(),
        );
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The resident process has no console: give the terminal its own.
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        command.creation_flags(CREATE_NEW_CONSOLE);
    }
    spawn(&mut command, token)
}

/// Terminals tried by `terminal = "auto"`, with the arguments that open them in a folder.
#[cfg(not(windows))]
const TERMINALS: &[(&str, &[&str])] = &[
    ("xdg-terminal-exec", &[]),
    ("ptyxis", &["--new-window", "--working-directory", "{dir}"]),
    ("gnome-terminal", &["--working-directory={dir}"]),
    ("konsole", &["--workdir", "{dir}"]),
    ("kitty", &["--directory", "{dir}"]),
    ("alacritty", &["--working-directory", "{dir}"]),
    ("wezterm", &["start", "--cwd", "{dir}"]),
    ("foot", &["--working-directory={dir}"]),
    ("xterm", &[]),
];

#[cfg(windows)]
const TERMINALS: &[(&str, &[&str])] = &[("wt.exe", &["-d", "{dir}"]), ("powershell.exe", &["-NoExit"])];

/// The command that opens a terminal in `dir`: found with `"auto"`, or the one of the setting
/// (`{dir}` replaced; without it, the terminal starts in `dir`). `None` when none is found.
fn terminal_command(setting: &str, dir: &Path, available: &dyn Fn(&str) -> bool) -> Option<Command> {
    let dir_text = dir.to_string_lossy();
    let (program, arguments): (String, Vec<String>) = if setting.trim() == "auto" {
        let (program, arguments) = TERMINALS.iter().find(|(program, _)| available(program))?;
        (
            (*program).to_owned(),
            arguments
                .iter()
                .map(|argument| argument.replace("{dir}", &dir_text))
                .collect(),
        )
    } else {
        let mut words = setting
            .split_whitespace()
            .map(|word| word.replace("{dir}", &dir_text));
        (words.next()?, words.collect())
    };
    let mut command = Command::new(program);
    command.args(arguments).current_dir(dir);
    Some(command)
}

/// Finds a program in `PATH`.
fn find_program(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|folder| folder.join(program))
        .find(|candidate| candidate.is_file())
}

/// Starts a program and collects it when it ends.
fn spawn(command: &mut Command, token: Option<&str>) -> Result<(), String> {
    if let Some(token) = token {
        // Lets the new window take the focus on Wayland.
        command.env("XDG_ACTIVATION_TOKEN", token);
    }
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {program}: {error}"))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// The desktop portal (`org.freedesktop.portal.OpenURI`): the default application, chosen by the
/// desktop, gets the focus.
#[cfg(target_os = "linux")]
mod portal {
    use std::fs::File;
    use std::path::Path;

    use ashpd::desktop::open_uri::{OpenDirectoryRequest, OpenFileRequest};

    pub fn open(path: &Path, token: Option<&str>) -> Result<(), String> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        futures_lite::future::block_on(async {
            OpenFileRequest::default()
                .activation_token(token.map(ashpd::ActivationToken::from))
                .send_file(&file)
                .await?
                .response()
        })
        .map_err(|error| error.to_string())
    }

    /// Opens the folder of `path` with `path` selected.
    pub fn reveal(path: &Path, token: Option<&str>) -> Result<(), String> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        futures_lite::future::block_on(async {
            OpenDirectoryRequest::default()
                .activation_token(token.map(ashpd::ActivationToken::from))
                .send(&file)
                .await?
                .response()
        })
        .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[cfg(not(windows))]
    #[test]
    fn auto_takes_the_first_terminal_found() {
        let dir = Path::new("/home/me/My Projects");
        let command =
            terminal_command("auto", dir, &|program| program == "kitty" || program == "xterm").unwrap();
        assert_eq!(command.get_program(), "kitty");
        assert_eq!(arguments(&command), ["--directory", "/home/me/My Projects"]);
        assert_eq!(command.get_current_dir(), Some(dir));
    }

    #[test]
    fn no_terminal_found() {
        assert!(terminal_command("auto", Path::new("/tmp"), &|_| false).is_none());
    }

    #[test]
    fn custom_terminal_command() {
        let dir = Path::new("/srv/data");
        let command = terminal_command("myterm --cwd {dir} --big", dir, &|_| false).unwrap();
        assert_eq!(command.get_program(), "myterm");
        assert_eq!(arguments(&command), ["--cwd", "/srv/data", "--big"]);
        let command = terminal_command("myterm", dir, &|_| false).unwrap();
        assert!(arguments(&command).is_empty());
        assert_eq!(command.get_current_dir(), Some(dir));
    }

    #[test]
    fn terminal_opens_in_the_folder_of_a_file() {
        let folder = assert_fs::TempDir::new().unwrap();
        let file = folder.path().join("notes.txt");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(folder_of(&file), folder.path());
        assert_eq!(folder_of(folder.path()), folder.path());
    }

    #[test]
    fn missing_paths_are_errors() {
        let error = perform(Action::Open, Path::new("/no/such/file"), "auto", None).unwrap_err();
        assert!(error.contains("no longer exists"), "{error}");
    }
}

//! The real executable: start in the background, status, search through the protocol, quit.
//! Each test uses its own socket and settings (indexing a temporary folder, not the home folder).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use assert_fs::TempDir;
use assert_fs::prelude::*;
use serafind_ipc::{Mode, Request, Response, SOCKET_VARIABLE};

struct Instance {
    _folder: TempDir,
    settings: PathBuf,
    data_home: PathBuf,
    socket: PathBuf,
}

impl Instance {
    /// Settings indexing `data/` of a temporary folder, and a socket of its own (short path:
    /// Unix sockets accept about 100 characters).
    fn new(name: &str) -> Self {
        let folder = TempDir::new().unwrap();
        for file in ["data/Documents/facture-2026.pdf", "data/notes.txt"] {
            folder.child(file).touch().unwrap();
        }
        folder
            .child("data/Projects/main.rs")
            .write_str("fn main() {\n    // TODO: fix the parser\n}\n")
            .unwrap();
        let settings = folder.path().join("settings");
        std::fs::create_dir_all(&settings).unwrap();
        let data = folder.path().join("data");
        std::fs::write(
            settings.join("config.toml"),
            format!(
                "[index]\nroots = [{:?}]\n[shortcut]\nenabled = false\n[actions]\nterminal = \"true\"\n[startup]\nwith_session = false\n",
                data.to_string_lossy()
            ),
        )
        .unwrap();
        // A path on Unix; a pipe name (no folder) on Windows.
        let socket_name = format!("sf-{name}-{}.sock", std::process::id());
        let socket = if cfg!(windows) {
            PathBuf::from(socket_name)
        } else {
            std::env::temp_dir().join(socket_name)
        };
        let data_home = folder.path().join("share");
        Self {
            _folder: folder,
            settings,
            data_home,
            socket,
        }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_serafind"))
            .args(arguments)
            .arg("--settings")
            .arg(&self.settings)
            .env(SOCKET_VARIABLE, &self.socket)
            // The desktop entry goes into the test folder, not the real home folder.
            .env("XDG_DATA_HOME", &self.data_home)
            // And so does the log.
            .env("XDG_STATE_HOME", &self.data_home)
            // And the start with the session.
            .env("XDG_CONFIG_HOME", &self.data_home)
            .output()
            .unwrap()
    }

    fn call(&self, request: &Request) -> std::io::Result<Response> {
        connect(&self.socket)?.call(request)
    }

    fn wait_for_index(&self) {
        let started = Instant::now();
        loop {
            if let Ok(Response::Status(status)) = self.call(&Request::Status)
                && status.index_ready
            {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the index was never ready"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = self.run(&["quit"]);
    }
}

/// A client of the instance listening at `socket`.
fn connect(socket: &Path) -> std::io::Result<serafind_ipc::Client> {
    serafind_ipc::Client::connect_to(&socket.to_string_lossy())
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn status_when_not_running() {
    let instance = Instance::new("absent");
    let output = instance.run(&["status"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(stdout(&output).contains("not running"));
}

#[test]
fn start_search_and_quit() {
    let instance = Instance::new("cycle");
    let output = instance.run(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout(&output).contains("Serafind started"));
    instance.wait_for_index();

    let status = stdout(&instance.run(&["status"]));
    assert!(status.contains("is running"), "{status}");
    assert!(status.contains("Bar: closed."), "{status}");

    let search = Request::Search {
        line: "fact ext:pdf".to_owned(),
        mode: Mode::All,
        max: 10,
    };
    let Ok(Response::Results(results)) = instance.call(&search) else {
        panic!("no results");
    };
    assert_eq!(results.total, 1);
    assert_eq!(results.rows[0].name, "facture-2026.pdf");
    assert_eq!(results.rows[0].highlights, [0, 1, 2, 3]);

    // A second start finds the running one (and would toggle the bar).
    assert!(
        Command::new(env!("CARGO_BIN_EXE_serafind"))
            .args(["daemon", "--settings"])
            .arg(&instance.settings)
            .env(SOCKET_VARIABLE, &instance.socket)
            .output()
            .unwrap()
            .stderr
            .starts_with(b"serafind: Serafind is already running")
    );

    let output = instance.run(&["quit"]);
    assert!(stdout(&output).contains("stopped"));
    let started = Instant::now();
    while instance.call(&Request::Status).is_ok() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "still running after quit"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn bad_requests_are_answered_with_an_error() {
    use std::io::{BufRead, Write};
    let instance = Instance::new("bad");
    assert!(instance.run(&[]).status.success());
    let mut stream = std::os::unix::net::UnixStream::connect(&instance.socket).unwrap();
    stream.write_all(b"{\"type\":\"format_disk\"}\n").unwrap();
    let mut line = String::new();
    std::io::BufReader::new(&stream).read_line(&mut line).unwrap();
    assert!(line.contains("\"type\":\"error\""), "{line}");
}

#[cfg(unix)]
#[test]
fn actions_are_done_by_the_resident_process() {
    let instance = Instance::new("act");
    assert!(instance.run(&[]).status.success());
    let data = instance.settings.parent().unwrap().join("data");
    // The terminal of these settings is `true`: it starts and ends at once.
    let terminal = Request::Act {
        action: serafind_ipc::Action::Terminal,
        path: data.join("notes.txt").to_string_lossy().into_owned(),
        activation_token: None,
    };
    assert_eq!(instance.call(&terminal).unwrap(), Response::Ok);
    let missing = Request::Act {
        action: serafind_ipc::Action::Open,
        path: data.join("gone.txt").to_string_lossy().into_owned(),
        activation_token: None,
    };
    match instance.call(&missing).unwrap() {
        Response::Error { message } => assert!(message.contains("no longer exists"), "{message}"),
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn content_search_reads_the_files() {
    let instance = Instance::new("content");
    assert!(instance.run(&[]).status.success());
    instance.wait_for_index();
    let search = Request::Search {
        line: "todo: fix".to_owned(),
        mode: serafind_ipc::Mode::Content,
        max: 10,
    };
    // Ask again while it is reading, as the bar does.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let results = loop {
        let Ok(Response::Results(results)) = instance.call(&search) else {
            panic!("no results");
        };
        if !results.searching || std::time::Instant::now() > deadline {
            break results;
        }
    };
    assert_eq!(results.total, 1);
    let row = &results.rows[0];
    assert_eq!(row.name, "main.rs");
    let line = row.line.as_ref().unwrap();
    assert_eq!((line.number, line.text.as_str()), (2, "// TODO: fix the parser"));
    let bad = Request::Search {
        line: "re:\"(\"".to_owned(),
        mode: serafind_ipc::Mode::All,
        max: 10,
    };
    let Ok(Response::Results(results)) = instance.call(&bad) else {
        panic!("no results");
    };
    assert!(results.problem.unwrap().contains("not a valid expression"));
}

// On Windows this would write the Run registry value of the machine running the tests.
#[cfg(unix)]
#[test]
fn start_with_the_session_is_applied_live() {
    let instance = Instance::new("session");
    assert!(instance.run(&[]).status.success());
    let file = instance
        .data_home
        .join("autostart/io.github.bluelabl_team.Serafind.desktop");
    assert!(!file.exists());
    let config = instance.settings.join("config.toml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(
        &config,
        text.replace("with_session = false", "with_session = true"),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let entry = std::fs::read_to_string(&file).unwrap();
    assert!(entry.contains(" start\n"), "{entry}");
    assert!(stdout(&instance.run(&["status"])).contains("Starts with the session: yes"));
    // Already running: `start` leaves it alone.
    let output = instance.run(&["start"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("already running"));
}

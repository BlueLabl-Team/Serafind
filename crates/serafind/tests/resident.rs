//! The real executable: start in the background, status, search through the protocol, quit.
//! Each test uses its own socket and settings (indexing a temporary folder, not the home folder).
#![cfg(unix)]

use std::io::{BufRead, Write};
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
        let settings = folder.path().join("settings");
        std::fs::create_dir_all(&settings).unwrap();
        let data = folder.path().join("data");
        std::fs::write(
            settings.join("config.toml"),
            format!(
                "[index]\nroots = [{:?}]\n[shortcut]\nenabled = false\n",
                data.to_string_lossy()
            ),
        )
        .unwrap();
        let socket = std::env::temp_dir().join(format!("sf-{name}-{}.sock", std::process::id()));
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
    let instance = Instance::new("bad");
    assert!(instance.run(&[]).status.success());
    let mut stream = std::os::unix::net::UnixStream::connect(&instance.socket).unwrap();
    stream.write_all(b"{\"type\":\"format_disk\"}\n").unwrap();
    let mut line = String::new();
    std::io::BufReader::new(&stream).read_line(&mut line).unwrap();
    assert!(line.contains("\"type\":\"error\""), "{line}");
}

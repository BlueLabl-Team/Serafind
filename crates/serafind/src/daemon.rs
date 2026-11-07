//! The resident process: keeps the index up to date, answers the local socket, and starts, shows
//! or hides the bar process. It shows no window itself.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serafind_index::{Exclusions, LiveIndex};
use serafind_ipc::{
    Client, Connection, Event, Listener, PROTOCOL_VERSION, Request, Response, Results, Status, Stream,
};
use serafind_settings::{IndexSettings, Settings, SettingsWatcher, SystemMode};

use crate::search::{SharedIndex, from_wire_mode, search_index, understand};

/// How long a bar process may take to connect before another one may be started.
const BAR_START_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs the resident process until it is asked to quit.
///
/// # Errors
/// When another resident process already runs, or the socket cannot be created.
pub fn run(settings_dir: Option<PathBuf>, home: PathBuf) -> io::Result<()> {
    if Client::connect().is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "Serafind is already running",
        ));
    }
    let listener = Listener::bind()?;
    eprintln!("serafind: listening at {}", listener.location());

    let settings = load_settings(settings_dir.as_deref());
    let daemon = Arc::new(Daemon {
        home,
        settings_dir,
        index: Arc::new(RwLock::new(None)),
        live_index: Mutex::new(None),
        index_settings: Mutex::new(settings.config.index.clone()),
        bar: Mutex::new(BarState::default()),
        quitting: AtomicBool::new(false),
    });
    daemon.start_index(&settings.config.index);
    let _watcher = daemon.watch_settings();

    while !daemon.quitting.load(Ordering::Acquire) {
        match listener.accept() {
            Ok(connection) => {
                let daemon = Arc::clone(&daemon);
                std::thread::spawn(move || daemon.serve(connection));
            }
            Err(error) => eprintln!("serafind: connection failed: {error}"),
        }
    }
    daemon.bar.lock().expect("bar lock").send(&Event::Quit);
    eprintln!("serafind: stopped");
    Ok(())
}

struct Daemon {
    home: PathBuf,
    settings_dir: Option<PathBuf>,
    index: SharedIndex,
    /// Keeps the watcher of the index alive.
    live_index: Mutex<Option<LiveIndex>>,
    index_settings: Mutex<IndexSettings>,
    bar: Mutex<BarState>,
    quitting: AtomicBool,
}

/// What the resident process knows of the bar process.
#[derive(Default)]
struct BarState {
    /// The subscribed bar's connection, to push events; `None` when no bar runs.
    link: Option<(u64, Arc<Mutex<Stream>>)>,
    visible: bool,
    /// When a bar process was started and has not connected yet.
    starting: Option<Instant>,
}

impl BarState {
    fn send(&self, event: &Event) -> bool {
        self.link.as_ref().is_some_and(|(_, writer)| {
            writer
                .lock()
                .is_ok_and(|mut writer| serafind_ipc::write_message(&mut *writer, event).is_ok())
        })
    }
}

static NEXT_CONNECTION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Daemon {
    /// Answers one client until it disconnects.
    fn serve(&self, connection: Connection) {
        let id = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
        let (mut reader, writer) = connection.into_parts();
        let writer = Arc::new(Mutex::new(writer));
        loop {
            let request = match serafind_ipc::read_message::<Request>(&mut reader) {
                Ok(Some(request)) => request,
                Ok(None) => break,
                Err(error) => {
                    let response = Response::Error {
                        message: format!("cannot read the request: {error}"),
                    };
                    if reply(&writer, &response).is_err() || error.kind() != io::ErrorKind::InvalidData {
                        break;
                    }
                    continue;
                }
            };
            let response = self.answer(request, id, &writer);
            if reply(&writer, &response).is_err() {
                break;
            }
            if self.quitting.load(Ordering::Acquire) {
                // Wake the accept loop up so that it sees the flag.
                let _ = Client::connect();
                break;
            }
        }
        let mut bar = self.bar.lock().expect("bar lock");
        if bar.link.as_ref().is_some_and(|(bar_id, _)| *bar_id == id) {
            bar.link = None;
            bar.visible = false;
        }
    }

    fn answer(&self, request: Request, id: u64, writer: &Arc<Mutex<Stream>>) -> Response {
        match request {
            Request::Toggle { activation_token } => {
                let visible = self.bar.lock().expect("bar lock").visible;
                if visible {
                    self.hide();
                } else {
                    self.show(activation_token);
                }
                Response::Ok
            }
            Request::Show { activation_token } => {
                self.show(activation_token);
                Response::Ok
            }
            Request::Hide => {
                self.hide();
                Response::Ok
            }
            Request::Quit => {
                self.quitting.store(true, Ordering::Release);
                Response::Ok
            }
            Request::Status => Response::Status(self.status()),
            Request::Search { line, mode, max } => Response::Results(self.search(&line, mode, max)),
            Request::BarVisible { visible } => {
                self.bar.lock().expect("bar lock").visible = visible;
                Response::Ok
            }
            Request::Subscribe => {
                let mut bar = self.bar.lock().expect("bar lock");
                // A newer bar replaces an older one.
                bar.send(&Event::Quit);
                bar.link = Some((id, Arc::clone(writer)));
                bar.starting = None;
                bar.visible = true;
                Response::Ok
            }
        }
    }

    fn show(&self, activation_token: Option<String>) {
        let mut bar = self.bar.lock().expect("bar lock");
        if bar.link.is_some() {
            bar.send(&Event::Show { activation_token });
            bar.visible = true;
            return;
        }
        if bar
            .starting
            .is_some_and(|started| started.elapsed() < BAR_START_TIMEOUT)
        {
            return;
        }
        match self.start_bar(activation_token.as_deref()) {
            Ok(()) => bar.starting = Some(Instant::now()),
            Err(error) => eprintln!("serafind: cannot start the bar: {error}"),
        }
    }

    fn hide(&self) {
        let mut bar = self.bar.lock().expect("bar lock");
        bar.send(&Event::Hide);
        bar.visible = false;
    }

    /// Starts `serafind bar`; it connects back and subscribes.
    fn start_bar(&self, activation_token: Option<&str>) -> io::Result<()> {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("bar");
        if let Some(dir) = &self.settings_dir {
            command.arg("--settings").arg(dir);
        }
        if let Some(token) = activation_token {
            // winit hands it to the compositor so that the new window gets the focus.
            command.env("XDG_ACTIVATION_TOKEN", token);
        }
        command.spawn().map(drop)
    }

    fn status(&self) -> Status {
        let index = self.index.read().ok().and_then(|index| index.clone());
        let entries = index
            .as_ref()
            .and_then(|index| index.read().ok().map(|index| index.len() as u64))
            .unwrap_or(0);
        let bar = self.bar.lock().expect("bar lock");
        Status {
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol: PROTOCOL_VERSION,
            index_ready: index.is_some(),
            entries,
            bar_running: bar.link.is_some(),
            bar_visible: bar.visible,
        }
    }

    fn search(&self, line: &str, mode: serafind_ipc::Mode, max: u32) -> Results {
        let query = understand(line, &self.home);
        let index = self.index.read().ok().and_then(|index| index.clone());
        let Some(index) = index else {
            return Results {
                indexing: true,
                ..Results::default()
            };
        };
        let Ok(index) = index.read() else {
            return Results::default();
        };
        search_index(&index, &query, from_wire_mode(mode), max as usize, &self.home)
    }

    /// Builds the index in the background, then keeps it up to date.
    fn start_index(self: &Arc<Self>, settings: &IndexSettings) {
        let (roots, exclusions) = index_scope(settings, &self.home);
        if let Ok(mut index) = self.index.write() {
            *index = None;
        }
        let daemon = Arc::clone(self);
        std::thread::spawn(move || {
            let started = Instant::now();
            match LiveIndex::start(roots, exclusions, serafind_index::DEFAULT_DELAY) {
                Ok(live) => {
                    let entries = live.index().read().map_or(0, |index| index.len());
                    eprintln!("serafind: {entries} entries indexed in {:.0?}", started.elapsed());
                    if let Ok(mut index) = daemon.index.write() {
                        *index = Some(Arc::clone(live.index()));
                    }
                    if let Ok(mut slot) = daemon.live_index.lock() {
                        *slot = Some(live);
                    }
                }
                Err(error) => eprintln!("serafind: cannot watch the files: {error}"),
            }
        });
    }

    /// Indexes again when the indexed folders or exclusions change in `config.toml`.
    fn watch_settings(self: &Arc<Self>) -> Option<SettingsWatcher> {
        let dir = self.settings_dir.clone()?;
        let watched = dir.clone();
        let daemon = Arc::clone(self);
        SettingsWatcher::start(&watched, Duration::from_millis(300), move || {
            let Ok(settings) = serafind_settings::load(&dir, SystemMode::Dark) else {
                return; // The bar shows the error.
            };
            let changed = {
                let mut current = daemon.index_settings.lock().expect("index settings lock");
                let changed = *current != settings.config.index;
                current.clone_from(&settings.config.index);
                changed
            };
            if changed {
                eprintln!("serafind: indexed folders changed, indexing again");
                daemon.start_index(&settings.config.index);
            }
        })
        .ok()
    }
}

fn reply(writer: &Arc<Mutex<Stream>>, response: &Response) -> io::Result<()> {
    let mut writer = writer
        .lock()
        .map_err(|_| io::Error::other("connection lock poisoned"))?;
    serafind_ipc::write_message(&mut *writer, response)
}

/// The settings, or the defaults when they cannot be read (the bar shows the error).
#[must_use]
pub fn load_settings(dir: Option<&Path>) -> Settings {
    dir.and_then(|dir| serafind_settings::load(dir, SystemMode::Dark).ok())
        .unwrap_or_else(|| Settings::defaults(SystemMode::Dark))
}

/// The folders to index and what to leave out, from the settings.
#[must_use]
pub fn index_scope(settings: &IndexSettings, home: &Path) -> (Vec<PathBuf>, Exclusions) {
    let roots = settings
        .roots
        .iter()
        .map(|root| expand_home(root, home))
        .collect();
    let mut exclusions = Exclusions::defaults(home);
    exclusions.names.clone_from(&settings.exclude_names);
    if !settings.exclude_paths.is_empty() {
        exclusions.paths = settings
            .exclude_paths
            .iter()
            .map(|path| expand_home(path, home))
            .collect();
    }
    exclusions.hidden_folders = !settings.hidden_folders;
    (roots, exclusions)
}

/// `~` and `~/x` from the home folder.
#[must_use]
pub fn expand_home(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        home.to_path_buf()
    } else if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        home.join(rest)
    } else {
        PathBuf::from(path)
    }
}

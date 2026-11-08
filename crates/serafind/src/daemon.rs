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

use crate::content::{ContentQuery, ContentSearch};
use crate::search::{SharedApps, SharedIndex, from_wire_mode, search, understand};
use crate::shortcut::{Shortcut, ShortcutState};
use crate::tray::{Action, Tray};
use crate::usage::Usage;
use serafind_core::SearchMode;
use std::collections::HashMap;

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

    match crate::desktop::install_desktop_file() {
        Ok(written) => {
            for path in written {
                eprintln!("serafind: written: {}", path.display());
            }
        }
        Err(error) => eprintln!("serafind: cannot write the desktop entry: {error}"),
    }
    let settings = load_settings(settings_dir.as_deref());
    let daemon = Arc::new(Daemon {
        home,
        settings_dir,
        index: Arc::new(RwLock::new(None)),
        live_index: Mutex::new(None),
        index_settings: Mutex::new(settings.config.index.clone()),
        terminal: Mutex::new(settings.config.actions.terminal.clone()),
        open_at_line: Mutex::new(settings.config.actions.open_at_line.clone()),
        content: Mutex::new(settings.config.content.clone()),
        content_searches: Mutex::new(HashMap::new()),
        apps: Arc::new(RwLock::new(Arc::new(Vec::new()))),
        apps_watcher: Mutex::new(None),
        usage: Mutex::new(Usage::load(Usage::default_file())),
        learn_from_usage: AtomicBool::new(settings.config.behavior.learn_from_usage),
        with_session: Mutex::new(None),
        bar: Mutex::new(BarState::default()),
        quitting: AtomicBool::new(false),
        shortcut: Mutex::new(None),
        shortcut_keys: Mutex::new(String::new()),
        shortcut_state: Arc::new(Mutex::new(ShortcutState::default())),
        tray: Mutex::new(TrayState::default()),
    });
    daemon.apply_startup(settings.config.startup.with_session);
    daemon.start_index(&settings.config.index);
    daemon.start_apps();
    daemon.start_shortcut(&settings.config.shortcut);
    daemon.start_tray(settings.config.tray.enabled);
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
    drop(daemon.shortcut.lock().expect("shortcut lock").take());
    drop(daemon.tray.lock().expect("tray lock").icon.take());
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
    /// `[actions] terminal`.
    terminal: Mutex<String>,
    /// `[actions] open_at_line`.
    open_at_line: Mutex<String>,
    /// `[content]`.
    content: Mutex<serafind_settings::ContentSettings>,
    /// The search inside files of each connection, with the request it answers.
    content_searches: Mutex<HashMap<u64, (String, Arc<ContentSearch>)>>,
    /// The installed applications.
    apps: SharedApps,
    /// Reads the applications again when they change.
    apps_watcher: Mutex<Option<serafind_apps::AppsWatcher>>,
    /// How often each application was started.
    usage: Mutex<Usage>,
    /// `[behavior] learn_from_usage`.
    learn_from_usage: AtomicBool,
    /// `[startup] with_session`, as applied.
    with_session: Mutex<Option<bool>>,
    bar: Mutex<BarState>,
    quitting: AtomicBool,
    /// The global shortcut; replaced when `[shortcut]` changes.
    shortcut: Mutex<Option<Shortcut>>,
    shortcut_keys: Mutex<String>,
    shortcut_state: Arc<Mutex<ShortcutState>>,
    /// The notification area icon; started or removed when `[tray] enabled` changes.
    tray: Mutex<TrayState>,
}

#[derive(Default)]
struct TrayState {
    enabled: bool,
    icon: Option<Tray>,
    problem: Option<String>,
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
        // Stop its search inside files, if any.
        drop(self.content_searches.lock().expect("content lock").remove(&id));
        let mut bar = self.bar.lock().expect("bar lock");
        if bar.link.as_ref().is_some_and(|(bar_id, _)| *bar_id == id) {
            bar.link = None;
            bar.visible = false;
        }
    }

    fn answer(&self, request: Request, id: u64, writer: &Arc<Mutex<Stream>>) -> Response {
        match request {
            Request::Toggle { activation_token } => {
                self.toggle(activation_token);
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
            Request::Search { line, mode, max } => Response::Results(self.search(&line, mode, max, id)),
            Request::BarVisible { visible } => {
                self.bar.lock().expect("bar lock").visible = visible;
                Response::Ok
            }
            Request::Act {
                action,
                path,
                activation_token,
            } => {
                let terminal = self.terminal.lock().expect("terminal lock").clone();
                let result = if action == serafind_ipc::Action::Launch {
                    self.launch(&path, &terminal, activation_token.as_deref())
                } else if let serafind_ipc::Action::OpenAtLine { line } = action {
                    let apps = self.apps.read().map(|apps| Arc::clone(&apps)).unwrap_or_default();
                    let setting = self.open_at_line.lock().expect("open at line lock").clone();
                    crate::actions::open_at_line(
                        Path::new(&path),
                        line,
                        &setting,
                        &apps,
                        &terminal,
                        activation_token.as_deref(),
                    )
                } else {
                    crate::actions::perform(action, Path::new(&path), &terminal, activation_token.as_deref())
                };
                match result {
                    Ok(()) => Response::Ok,
                    Err(message) => {
                        eprintln!("serafind: {message}");
                        Response::Error { message }
                    }
                }
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

    fn toggle(&self, activation_token: Option<String>) {
        let visible = self.bar.lock().expect("bar lock").visible;
        if visible {
            self.hide();
        } else {
            self.show(activation_token);
        }
    }

    /// Sets up the global shortcut (again when it changed in the settings).
    fn start_shortcut(self: &Arc<Self>, settings: &serafind_settings::Shortcut) {
        let text = if settings.enabled {
            settings.for_this_system()
        } else {
            ""
        };
        let mut keys_in_use = self.shortcut_keys.lock().expect("shortcut lock");
        if *keys_in_use == text {
            return;
        }
        text.clone_into(&mut keys_in_use);
        let mut slot = self.shortcut.lock().expect("shortcut lock");
        // Release the old keys first.
        drop(slot.take());
        if let Ok(mut state) = self.shortcut_state.lock() {
            *state = ShortcutState::default();
            if !settings.enabled {
                state.problem = Some("turned off in config.toml ([shortcut] enabled = false)".to_owned());
                return;
            }
        }
        let keys = match serafind_settings::Keys::parse(text) {
            Ok(keys) => keys,
            Err(problem) => {
                // config.toml is refused in that case: the bar shows why.
                if let Ok(mut state) = self.shortcut_state.lock() {
                    state.problem = Some(format!("shortcut {text:?} {problem}"));
                }
                return;
            }
        };
        let daemon = Arc::downgrade(self);
        let on_press: crate::shortcut::OnPress = Arc::new(move |token| {
            if let Some(daemon) = daemon.upgrade() {
                daemon.toggle(token);
            }
        });
        *slot = Some(crate::shortcut::start(
            keys,
            Arc::clone(&self.shortcut_state),
            on_press,
        ));
    }

    /// Shows or removes the notification area icon.
    fn start_tray(self: &Arc<Self>, enabled: bool) {
        let mut tray = self.tray.lock().expect("tray lock");
        if tray.enabled == enabled {
            return;
        }
        tray.enabled = enabled;
        tray.problem = None;
        if !enabled {
            drop(tray.icon.take());
            return;
        }
        let daemon = Arc::downgrade(self);
        let on_action: crate::tray::OnAction = Arc::new(move |action, token| {
            if let Some(daemon) = daemon.upgrade() {
                daemon.act(action, token);
            }
        });
        match crate::tray::start(on_action) {
            Ok(icon) => tray.icon = Some(icon),
            Err(problem) => {
                eprintln!("serafind: notification area icon: {problem}");
                tray.problem = Some(problem);
            }
        }
    }

    /// Does what was chosen in the notification area icon.
    fn act(self: &Arc<Self>, action: Action, activation_token: Option<String>) {
        match action {
            Action::Toggle => self.toggle(activation_token),
            Action::Show => self.show(activation_token),
            Action::Reindex => {
                let settings = self.index_settings.lock().expect("index settings lock").clone();
                eprintln!("serafind: indexing again");
                self.start_index(&settings);
            }
            Action::OpenSettings => match &self.settings_dir {
                Some(dir) => {
                    if let Err(error) = crate::desktop::open(dir, activation_token.as_deref()) {
                        eprintln!("serafind: cannot open {}: {error}", dir.display());
                    }
                }
                None => eprintln!("serafind: no settings folder"),
            },
            Action::Quit => {
                self.quitting.store(true, Ordering::Release);
                // Wake the accept loop up so that it sees the flag.
                let _ = Client::connect();
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
        let mut child = command.spawn()?;
        // Collect it when it ends, so that it does not linger as a zombie process.
        std::thread::spawn(move || child.wait());
        Ok(())
    }

    fn status(&self) -> Status {
        let index = self.index.read().ok().and_then(|index| index.clone());
        let entries = index
            .as_ref()
            .and_then(|index| index.read().ok().map(|index| index.len() as u64))
            .unwrap_or(0);
        let bar = self.bar.lock().expect("bar lock");
        let shortcut = self
            .shortcut_state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default();
        let tray = self.tray.lock().expect("tray lock");
        Status {
            with_session: self.with_session.lock().expect("startup lock").unwrap_or(false),
            tray_enabled: tray.enabled,
            tray_problem: tray
                .problem
                .clone()
                .or_else(|| tray.icon.as_ref().and_then(Tray::problem)),
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol: PROTOCOL_VERSION,
            index_ready: index.is_some(),
            entries,
            bar_running: bar.link.is_some(),
            bar_visible: bar.visible,
            shortcut: shortcut.active,
            shortcut_problem: shortcut.problem,
        }
    }

    /// The Content mode: starts a search inside the files for this connection (or goes on with
    /// the one already running for the same line), waits a little, and gives what it found so
    /// far; `searching` asks the bar to come back for more.
    fn search_content(
        &self,
        query: &serafind_core::Query,
        line: &str,
        mode: serafind_ipc::Mode,
        max: usize,
        connection: u64,
    ) -> Results {
        let empty = Results {
            mode: Some(serafind_ipc::Mode::Content),
            ..Results::default()
        };
        let key = format!("{line}\u{0}{mode:?}\u{0}{max}");
        let running = {
            let searches = self.content_searches.lock().expect("content lock");
            searches
                .get(&connection)
                .filter(|(running, _)| *running == key)
                .map(|(_, search)| Arc::clone(search))
        };
        let search = if let Some(search) = running {
            search
        } else {
            let content_query = match ContentQuery::new(query) {
                Ok(Some(content_query)) => content_query,
                Ok(None) => return empty,
                Err(problem) => {
                    return Results {
                        problem: Some(problem),
                        ..empty
                    };
                }
            };
            let index = self.index.read().ok().and_then(|index| index.clone());
            let Some(index) = index else {
                return Results {
                    indexing: true,
                    ..empty
                };
            };
            let candidates = index
                .read()
                .ok()
                .and_then(|index| index.content_candidates(query));
            let Some(files) = candidates else {
                return Results {
                    outside_index: true,
                    ..empty
                };
            };
            let settings = self.content.lock().expect("content settings lock").clone();
            let max_size = u64::from(settings.max_file_size) * 1024 * 1024;
            let search = Arc::new(ContentSearch::start(files, content_query, max_size, max));
            self.content_searches
                .lock()
                .expect("content lock")
                .insert(connection, (key, Arc::clone(&search)));
            search
        };
        search.wait(Duration::from_millis(120));
        let progress = search.progress();
        let rows = progress
            .hits
            .iter()
            .map(|(path, found)| serafind_ipc::Row {
                name: path
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                location: crate::search::shorten(path.parent().unwrap_or(path), &self.home),
                path: path.to_string_lossy().into_owned(),
                kind: serafind_ipc::Kind::File,
                highlights: Vec::new(),
                app: None,
                icon: None,
                line: Some(serafind_ipc::Line {
                    number: found.line,
                    text: found.text.clone(),
                    highlights: found.highlights.clone(),
                    count: found.count,
                }),
            })
            .collect();
        let percent = (progress.read * 100).checked_div(progress.files).unwrap_or(100);
        Results {
            rows,
            total: progress.total as u64,
            searching: !progress.done,
            progress: Some(u8::try_from(percent.min(100)).unwrap_or(100)),
            ..empty
        }
    }

    /// Registers Serafind to start with the session, or removes it, when the setting changed.
    fn apply_startup(&self, with_session: bool) {
        let mut applied = self.with_session.lock().expect("startup lock");
        if *applied == Some(with_session) {
            return;
        }
        *applied = Some(with_session);
        match crate::autostart::apply(with_session) {
            Ok(Some(change)) => eprintln!("serafind: start with the session: {change}"),
            Ok(None) => {}
            Err(error) => eprintln!("serafind: cannot change the start with the session: {error}"),
        }
    }

    /// Starts the application `id`, and counts it when learning from usage.
    fn launch(&self, id: &str, terminal: &str, token: Option<&str>) -> Result<(), String> {
        let apps = self.apps.read().map(|apps| Arc::clone(&apps)).unwrap_or_default();
        let app = apps
            .iter()
            .find(|app| app.id == id)
            .ok_or_else(|| format!("{id} is no longer installed"))?;
        crate::actions::launch(app, terminal, token)?;
        if self.learn_from_usage.load(Ordering::Acquire)
            && let Err(error) = self.usage.lock().expect("usage lock").record(id)
        {
            eprintln!("serafind: cannot save the usage counts: {error}");
        }
        Ok(())
    }

    /// Reads the installed applications in the background, and again when they change.
    fn start_apps(self: &Arc<Self>) {
        let read = {
            let apps = Arc::clone(&self.apps);
            move || {
                let started = Instant::now();
                let found = serafind_apps::discover();
                eprintln!(
                    "serafind: {} applications found in {:.0?}",
                    found.len(),
                    started.elapsed()
                );
                if let Ok(mut apps) = apps.write() {
                    *apps = Arc::new(found);
                }
            }
        };
        std::thread::spawn(read.clone());
        match serafind_apps::AppsWatcher::start(
            &serafind_apps::application_folders(),
            Duration::from_secs(1),
            read,
        ) {
            Ok(watcher) => *self.apps_watcher.lock().expect("apps watcher lock") = Some(watcher),
            Err(error) => eprintln!("serafind: cannot watch the applications: {error}"),
        }
    }

    fn search(&self, line: &str, mode: serafind_ipc::Mode, max: u32, connection: u64) -> Results {
        let query = understand(line, &self.home);
        if query.effective_mode(from_wire_mode(mode)) == SearchMode::Content {
            return self.search_content(&query, line, mode, max as usize, connection);
        }
        let apps = self.apps.read().map(|apps| Arc::clone(&apps)).unwrap_or_default();
        let index = self.index.read().ok().and_then(|index| index.clone());
        let index = index.as_ref().and_then(|index| index.read().ok());
        let usage = self.usage.lock().expect("usage lock");
        let launches = self
            .learn_from_usage
            .load(Ordering::Acquire)
            .then(|| usage.launches());
        search(
            index.as_deref(),
            &apps,
            launches,
            &query,
            from_wire_mode(mode),
            max as usize,
            &self.home,
        )
    }

    /// Builds the index in the background, then keeps it up to date.
    fn start_index(self: &Arc<Self>, settings: &IndexSettings) {
        let (roots, exclusions) = index_scope(settings, &self.home);
        if let Ok(mut index) = self.index.write() {
            *index = None;
        }
        let daemon = Arc::clone(self);
        std::thread::spawn(move || {
            // The walk and the watching run in the background of the session.
            crate::priority::lower_this_thread();
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
            settings
                .config
                .actions
                .terminal
                .clone_into(&mut daemon.terminal.lock().expect("terminal lock"));
            settings
                .config
                .actions
                .open_at_line
                .clone_into(&mut daemon.open_at_line.lock().expect("open at line lock"));
            *daemon.content.lock().expect("content settings lock") = settings.config.content.clone();
            daemon
                .learn_from_usage
                .store(settings.config.behavior.learn_from_usage, Ordering::Release);
            daemon.apply_startup(settings.config.startup.with_session);
            daemon.start_shortcut(&settings.config.shortcut);
            daemon.start_tray(settings.config.tray.enabled);
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

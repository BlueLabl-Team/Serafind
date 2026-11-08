//! The icon in the notification area (system tray), shown by the resident process when
//! `[tray] enabled = true`. A click opens or closes the bar; its menu opens the bar, indexes
//! again, opens the settings or quits.
//!
//! Linux: the `StatusNotifierItem` D-Bus protocol (KDE, and GNOME with the `AppIndicator`
//! extension). Windows: the notification area of the taskbar.

use std::sync::Arc;

use crate::strings::Strings;

/// What the user asked from the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Click on the icon: open the bar, or close it.
    Toggle,
    /// Menu: open the bar.
    Show,
    /// Menu: index again.
    Reindex,
    /// Menu: open the settings folder.
    OpenSettings,
    /// Menu: stop Serafind.
    Quit,
}

/// Called on each action, with a Wayland activation token when the desktop gave one.
pub type OnAction = Arc<dyn Fn(Action, Option<String>) + Send + Sync>;

/// The icon; removed when dropped.
pub struct Tray(backend::Handle);

/// Shows the icon.
///
/// # Errors
/// Why the icon cannot be shown, and what to do.
pub fn start(on_action: OnAction) -> Result<Tray, String> {
    backend::start(on_action).map(Tray)
}

impl Tray {
    /// Why the icon is not shown right now (no notification area yet), if so.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        self.0.problem()
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// The menu entries, in order; `None` is a separator.
fn menu_entries() -> [Option<(Action, &'static str)>; 5] {
    let strings = Strings::ENGLISH;
    [
        Some((Action::Show, strings.tray_open)),
        Some((Action::Reindex, strings.tray_reindex)),
        Some((Action::OpenSettings, strings.tray_settings)),
        None,
        Some((Action::Quit, strings.tray_quit)),
    ]
}

#[cfg(target_os = "linux")]
mod backend {
    use std::sync::{Arc, Mutex};

    use ksni::blocking::TrayMethods;

    use super::{Action, OnAction, Strings, menu_entries};
    use crate::logo;

    /// Shown while the desktop has no notification area. At login Serafind may start before it
    /// (GNOME loads its extensions after the session starts): the icon then waits for it.
    const WAITING: &str = "waiting for the notification area of the desktop (on GNOME, it comes \
         with the \"AppIndicator and KStatusNotifierItem Support\" extension)";

    pub struct Handle {
        tray: ksni::blocking::Handle<Icon>,
        waiting: Arc<Mutex<Option<String>>>,
    }

    impl Handle {
        pub fn stop(&self) {
            self.tray.shutdown().wait();
        }

        pub fn problem(&self) -> Option<String> {
            self.waiting.lock().ok().and_then(|waiting| waiting.clone())
        }
    }

    pub fn start(on_action: OnAction) -> Result<Handle, String> {
        let waiting = Arc::new(Mutex::new(None));
        let icon = Icon {
            on_action,
            waiting: Arc::clone(&waiting),
        };
        // A missing notification area is not an error: the icon appears when it comes.
        icon.assume_sni_available(true)
            .spawn()
            .map(|tray| Handle { tray, waiting })
            .map_err(|error| format!("cannot show the icon ({error})"))
    }

    pub struct Icon {
        on_action: OnAction,
        waiting: Arc<Mutex<Option<String>>>,
    }

    impl Icon {
        fn act(&self, action: Action) {
            (self.on_action)(action, activation_token());
        }
    }

    /// The token the desktop gave with the click, so that the bar gets the keyboard focus.
    fn activation_token() -> Option<String> {
        ksni::current_xdg_activation_token().or_else(|| {
            // It may arrive just after the click event (KDE).
            std::thread::sleep(std::time::Duration::from_millis(1));
            ksni::current_xdg_activation_token()
        })
    }

    impl ksni::Tray for Icon {
        fn id(&self) -> String {
            "serafind".to_owned()
        }

        fn title(&self) -> String {
            "Serafind".to_owned()
        }

        fn category(&self) -> ksni::Category {
            ksni::Category::ApplicationStatus
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            [logo::PNG_32, logo::PNG_64]
                .into_iter()
                .map(|png| {
                    let image = logo::decode(png);
                    let mut data = image.rgba;
                    for pixel in data.chunks_exact_mut(4) {
                        pixel.rotate_right(1); // RGBA to ARGB
                    }
                    ksni::Icon {
                        width: i32::try_from(image.width).unwrap_or(0),
                        height: i32::try_from(image.height).unwrap_or(0),
                        data,
                    }
                })
                .collect()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: Strings::ENGLISH.tray_tooltip.to_owned(),
                ..ksni::ToolTip::default()
            }
        }

        fn watcher_online(&self) {
            if let Ok(mut waiting) = self.waiting.lock() {
                *waiting = None;
            }
            eprintln!("serafind: notification area icon shown");
        }

        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            if let Ok(mut waiting) = self.waiting.lock() {
                *waiting = Some(WAITING.to_owned());
            }
            eprintln!("serafind: no notification area yet ({reason:?}): the icon waits for it");
            // Keep running: the icon appears when the notification area comes.
            true
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.act(Action::Toggle);
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            menu_entries()
                .into_iter()
                .map(|entry| match entry {
                    None => ksni::MenuItem::Separator,
                    Some((action, label)) => ksni::menu::StandardItem {
                        label: label.to_owned(),
                        activate: Box::new(move |icon: &mut Self| icon.act(action)),
                        ..ksni::menu::StandardItem::default()
                    }
                    .into(),
                })
                .collect()
        }
    }
}

#[cfg(windows)]
mod backend {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::JoinHandle;

    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    use super::{Action, OnAction, Strings, menu_entries};
    use crate::logo;

    pub struct Handle {
        stop: Arc<AtomicBool>,
        thread: std::sync::Mutex<Option<JoinHandle<()>>>,
    }

    impl Handle {
        #[expect(clippy::unused_self, reason = "same interface on every system")]
        pub fn problem(&self) -> Option<String> {
            None
        }

        pub fn stop(&self) {
            self.stop.store(true, Ordering::Release);
            if let Some(thread) = self.thread.lock().ok().and_then(|mut thread| thread.take()) {
                let _ = thread.join();
            }
        }
    }

    /// The icon and its menu must live on the thread that dispatches their window messages.
    pub fn start(on_action: OnAction) -> Result<Handle, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (started, result) = std::sync::mpsc::channel();
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || run(&on_action, &stop, &started))
        };
        match result.recv() {
            Ok(Ok(())) => Ok(Handle {
                stop,
                thread: std::sync::Mutex::new(Some(thread)),
            }),
            Ok(Err(problem)) => Err(problem),
            Err(_) => Err("the icon thread stopped".to_owned()),
        }
    }

    fn run(on_action: &OnAction, stop: &AtomicBool, started: &std::sync::mpsc::Sender<Result<(), String>>) {
        let menu = Menu::new();
        let mut actions = Vec::new();
        for entry in menu_entries() {
            let added = match entry {
                None => menu.append(&PredefinedMenuItem::separator()),
                Some((action, label)) => {
                    let item = MenuItem::new(label, true, None);
                    actions.push((item.id().clone(), action));
                    menu.append(&item)
                }
            };
            if let Err(error) = added {
                let _ = started.send(Err(format!("cannot build the icon menu ({error})")));
                return;
            }
        }
        let logo = logo::decode(logo::PNG_32);
        let icon = match tray_icon::Icon::from_rgba(logo.rgba, logo.width, logo.height) {
            Ok(icon) => icon,
            Err(error) => {
                let _ = started.send(Err(format!("cannot load the icon ({error})")));
                return;
            }
        };
        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip(Strings::ENGLISH.tray_tooltip)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build();
        let _tray = match tray {
            Ok(tray) => tray,
            Err(error) => {
                let _ = started.send(Err(format!("cannot show the icon ({error})")));
                return;
            }
        };
        let _ = started.send(Ok(()));
        while !stop.load(Ordering::Acquire) {
            pump_messages();
            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    on_action(Action::Toggle, None);
                }
            }
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if let Some((_, action)) = actions.iter().find(|(id, _)| *id == event.id) {
                    on_action(*action, None);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }

    /// Dispatches this thread's window messages: that is how clicks reach the icon.
    fn pump_messages() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
        };
        // SAFETY: plain Win32 message pumping on this thread with a zeroed MSG buffer.
        #[allow(unsafe_code)]
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&raw mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod backend {
    use super::OnAction;

    pub struct Handle;

    impl Handle {
        pub fn stop(&self) {}

        pub fn problem(&self) -> Option<String> {
            None
        }
    }

    pub fn start(_on_action: OnAction) -> Result<Handle, String> {
        Err("no notification area icon on this system".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_has_the_four_actions() {
        let actions: Vec<Action> = menu_entries()
            .into_iter()
            .flatten()
            .map(|(action, _)| action)
            .collect();
        assert_eq!(
            actions,
            [Action::Show, Action::Reindex, Action::OpenSettings, Action::Quit]
        );
        assert!(
            menu_entries()
                .iter()
                .flatten()
                .all(|(_, label)| !label.is_empty())
        );
    }
}

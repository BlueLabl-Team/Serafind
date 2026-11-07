//! The global shortcut that opens and closes the bar, held by the resident process.
//!
//! - Wayland (GNOME, KDE...): the `GlobalShortcuts` desktop portal. The shortcut of
//!   `config.toml` is only *proposed*: the desktop asks the user once, may let them change it,
//!   and remembers it. Each press comes with an activation token for the bar's focus.
//! - X11 and Windows: `global-hotkey` takes the keys directly.
//!
//! When the shortcut cannot be set up, the reason is kept for `serafind status` and the bar;
//! `serafind toggle` keeps working.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serafind_settings::Keys;

/// What happened to the shortcut, for `serafind status` and the bar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcutState {
    /// The shortcut in use, as the desktop describes it.
    pub active: Option<String>,
    /// Why there is no shortcut, and what to do.
    pub problem: Option<String>,
}

/// Called on each press, with the activation token the desktop gave, if any.
pub type OnPress = Arc<dyn Fn(Option<String>) + Send + Sync>;

/// A running shortcut; dropping it releases the keys.
pub struct Shortcut {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Shortcut {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Sets up `keys` in the background; `state` tells how it went.
#[must_use]
pub fn start(keys: Keys, state: Arc<Mutex<ShortcutState>>, on_press: OnPress) -> Shortcut {
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("serafind-shortcut".to_owned())
        .spawn(move || {
            let result = backend::run(&keys, &state, &on_press, &stopping);
            if let Err(problem) = result {
                eprintln!("serafind: global shortcut {keys}: {problem}");
                crate::desktop::notify(&format!("Serafind: {keys} is not available"), &problem);
                if let Ok(mut state) = state.lock() {
                    state.active = None;
                    state.problem = Some(problem);
                }
            }
        })
        .ok();
    Shortcut { stop, thread }
}

/// The advice given when no global shortcut can be set up.
const BIND_IT_YOURSELF: &str = "bind `serafind toggle` to a key in your desktop's keyboard settings";

/// How often a waiting backend checks whether it must stop.
#[cfg_attr(
    not(any(target_os = "linux", windows)),
    expect(dead_code, reason = "no backend")
)]
const STOP_CHECK: std::time::Duration = std::time::Duration::from_millis(200);

#[cfg(target_os = "linux")]
mod backend {
    use super::{AtomicBool, BIND_IT_YOURSELF, Keys, Mutex, OnPress, Ordering, STOP_CHECK, ShortcutState};

    pub fn run(
        keys: &Keys,
        state: &Mutex<ShortcutState>,
        on_press: &OnPress,
        stop: &AtomicBool,
    ) -> Result<(), String> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            portal::run(keys, state, on_press, stop)
        } else if std::env::var_os("DISPLAY").is_some() {
            super::hotkey::run(keys, state, on_press, stop)
        } else {
            Err(format!("no graphical session found: {BIND_IT_YOURSELF}"))
        }
    }

    mod portal {
        use ashpd::desktop::CreateSessionOptions;
        use ashpd::desktop::global_shortcuts::{
            BindShortcutsOptions, GlobalShortcuts, ListShortcutsOptions, NewShortcut, Shortcut,
        };
        use futures_lite::{FutureExt, StreamExt};

        use super::{
            AtomicBool, BIND_IT_YOURSELF, Keys, Mutex, OnPress, Ordering, STOP_CHECK, ShortcutState,
        };
        use crate::desktop::APP_ID;

        const SHORTCUT_ID: &str = "toggle";

        pub fn run(
            keys: &Keys,
            state: &Mutex<ShortcutState>,
            on_press: &OnPress,
            stop: &AtomicBool,
        ) -> Result<(), String> {
            futures_lite::future::block_on(async {
                // Tells the portal which application this process is (it is not sandboxed).
                if let Ok(app_id) = ashpd::AppID::try_from(APP_ID)
                    && let Err(error) = ashpd::register_host_app(app_id).await
                {
                    eprintln!("serafind: cannot register with the desktop portal: {error}");
                }
                let portal = GlobalShortcuts::new().await.map_err(|error| {
                    format!("your desktop has no global shortcut portal ({error}): {BIND_IT_YOURSELF}")
                })?;
                let session = portal
                    .create_session(CreateSessionOptions::default())
                    .await
                    .map_err(|error| format!("the desktop portal refused a session ({error})"))?;
                let wanted = keys.to_xdg();
                let shortcut = NewShortcut::new(SHORTCUT_ID, "Open or close the Serafind search bar")
                    .preferred_trigger(wanted.as_str());
                let find = |shortcuts: &[Shortcut]| {
                    shortcuts
                        .iter()
                        .find(|shortcut| shortcut.id() == SHORTCUT_ID)
                        .map(|shortcut| shortcut.trigger_description().to_owned())
                };
                let bound = portal
                    .bind_shortcuts(&session, &[shortcut], None, BindShortcutsOptions::default())
                    .await
                    .and_then(|request| request.response());
                let active = match bound {
                    Ok(bound) => find(bound.shortcuts()),
                    // GNOME 48 answers "failed" when the shortcut was already accepted earlier,
                    // yet binds it: ask what the session holds before giving up.
                    Err(error) => {
                        let listed = portal
                            .list_shortcuts(&session, ListShortcutsOptions::default())
                            .await
                            .and_then(|request| request.response())
                            .ok()
                            .and_then(|listed| find(listed.shortcuts()));
                        if listed.is_none() {
                            let _ = session.close().await;
                            return Err(format!(
                                "the shortcut was not accepted ({error}): choose another one, or {BIND_IT_YOURSELF}"
                            ));
                        }
                        listed
                    }
                }
                .filter(|description| !description.is_empty());
                if let Ok(mut state) = state.lock() {
                    state.problem = active
                        .is_none()
                        .then(|| format!("the desktop gave no key to the shortcut: set one in its settings, or {BIND_IT_YOURSELF}"));
                    state.active.clone_from(&active);
                }
                eprintln!(
                    "serafind: global shortcut: {}",
                    active.as_deref().unwrap_or("none")
                );

                let mut presses = portal
                    .receive_activated()
                    .await
                    .map_err(|error| format!("cannot follow the shortcut ({error})"))?;
                let listen = async {
                    while let Some(press) = presses.next().await {
                        if press.shortcut_id() == SHORTCUT_ID {
                            let token = press
                                .options()
                                .get("activation_token")
                                .and_then(|value| String::try_from(value.try_clone().ok()?).ok());
                            on_press(token);
                        }
                    }
                };
                let stopped = async {
                    while !stop.load(Ordering::Acquire) {
                        async_io::Timer::after(STOP_CHECK).await;
                    }
                };
                listen.or(stopped).await;
                let _ = session.close().await;
                Ok(())
            })
        }
    }
}

#[cfg(windows)]
mod backend {
    use super::{AtomicBool, Keys, Mutex, OnPress, ShortcutState};

    pub fn run(
        keys: &Keys,
        state: &Mutex<ShortcutState>,
        on_press: &OnPress,
        stop: &AtomicBool,
    ) -> Result<(), String> {
        super::hotkey::run(keys, state, on_press, stop)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod backend {
    use super::{AtomicBool, BIND_IT_YOURSELF, Keys, Mutex, OnPress, ShortcutState};

    pub fn run(
        _keys: &Keys,
        _state: &Mutex<ShortcutState>,
        _on_press: &OnPress,
        _stop: &AtomicBool,
    ) -> Result<(), String> {
        Err(format!(
            "global shortcuts are not supported on this system yet: {BIND_IT_YOURSELF}"
        ))
    }
}

/// X11 and Windows: the keys are grabbed directly.
#[cfg(any(target_os = "linux", windows))]
mod hotkey {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use serafind_settings::Key;

    use super::{AtomicBool, BIND_IT_YOURSELF, Keys, Mutex, OnPress, Ordering, STOP_CHECK, ShortcutState};

    pub fn run(
        keys: &Keys,
        state: &Mutex<ShortcutState>,
        on_press: &OnPress,
        stop: &AtomicBool,
    ) -> Result<(), String> {
        let hotkey = to_hotkey(keys);
        // On Windows the manager must live on the thread that runs the message loop below.
        let manager = GlobalHotKeyManager::new()
            .map_err(|error| format!("cannot grab keys ({error}): {BIND_IT_YOURSELF}"))?;
        manager.register(hotkey).map_err(|error| {
            format!(
                "{keys} is already used by another application ({error}): choose another one in config.toml"
            )
        })?;
        if let Ok(mut state) = state.lock() {
            state.active = Some(keys.to_string());
            state.problem = None;
        }
        eprintln!("serafind: global shortcut: {keys}");
        let wanted = hotkey.id();
        let on_press = std::sync::Arc::clone(on_press);
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.id() == wanted && event.state() == HotKeyState::Pressed {
                on_press(None);
            }
        }));
        wait(stop);
        GlobalHotKeyEvent::set_event_handler(None::<fn(GlobalHotKeyEvent)>);
        let _ = manager.unregister(hotkey);
        Ok(())
    }

    /// Waits until asked to stop. On Windows the thread's messages are dispatched meanwhile:
    /// that is how the hotkey events arrive.
    #[cfg(windows)]
    fn wait(stop: &AtomicBool) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
        };
        while !stop.load(Ordering::Acquire) {
            // SAFETY: plain Win32 message pumping on this thread with a zeroed MSG buffer.
            #[allow(unsafe_code)]
            unsafe {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        let _ = STOP_CHECK;
    }

    #[cfg(not(windows))]
    fn wait(stop: &AtomicBool) {
        while !stop.load(Ordering::Acquire) {
            std::thread::sleep(STOP_CHECK);
        }
    }

    fn to_hotkey(keys: &Keys) -> HotKey {
        let mut modifiers = Modifiers::empty();
        for (on, modifier) in [
            (keys.ctrl, Modifiers::CONTROL),
            (keys.alt, Modifiers::ALT),
            (keys.shift, Modifiers::SHIFT),
            (keys.logo, Modifiers::META),
        ] {
            if on {
                modifiers |= modifier;
            }
        }
        HotKey::new((!modifiers.is_empty()).then_some(modifiers), code(keys.key))
    }

    fn code(key: Key) -> Code {
        match key {
            Key::Letter(letter) => letter_code(letter),
            Key::Digit(digit) => digit_code(digit),
            Key::Function(number) => function_code(number),
            Key::Space => Code::Space,
            Key::Enter => Code::Enter,
            Key::Tab => Code::Tab,
            Key::Insert => Code::Insert,
            Key::Delete => Code::Delete,
            Key::Home => Code::Home,
            Key::End => Code::End,
            Key::PageUp => Code::PageUp,
            Key::PageDown => Code::PageDown,
            Key::Grave => Code::Backquote,
        }
    }

    fn letter_code(letter: char) -> Code {
        const LETTERS: [Code; 26] = [
            Code::KeyA,
            Code::KeyB,
            Code::KeyC,
            Code::KeyD,
            Code::KeyE,
            Code::KeyF,
            Code::KeyG,
            Code::KeyH,
            Code::KeyI,
            Code::KeyJ,
            Code::KeyK,
            Code::KeyL,
            Code::KeyM,
            Code::KeyN,
            Code::KeyO,
            Code::KeyP,
            Code::KeyQ,
            Code::KeyR,
            Code::KeyS,
            Code::KeyT,
            Code::KeyU,
            Code::KeyV,
            Code::KeyW,
            Code::KeyX,
            Code::KeyY,
            Code::KeyZ,
        ];
        let index = (letter.to_ascii_uppercase() as usize).saturating_sub('A' as usize);
        LETTERS.get(index).copied().unwrap_or(Code::KeyF)
    }

    fn digit_code(digit: char) -> Code {
        const DIGITS: [Code; 10] = [
            Code::Digit0,
            Code::Digit1,
            Code::Digit2,
            Code::Digit3,
            Code::Digit4,
            Code::Digit5,
            Code::Digit6,
            Code::Digit7,
            Code::Digit8,
            Code::Digit9,
        ];
        DIGITS[digit.to_digit(10).unwrap_or(0) as usize]
    }

    fn function_code(number: u8) -> Code {
        const FUNCTIONS: [Code; 24] = [
            Code::F1,
            Code::F2,
            Code::F3,
            Code::F4,
            Code::F5,
            Code::F6,
            Code::F7,
            Code::F8,
            Code::F9,
            Code::F10,
            Code::F11,
            Code::F12,
            Code::F13,
            Code::F14,
            Code::F15,
            Code::F16,
            Code::F17,
            Code::F18,
            Code::F19,
            Code::F20,
            Code::F21,
            Code::F22,
            Code::F23,
            Code::F24,
        ];
        FUNCTIONS[usize::from(number.clamp(1, 24) - 1)]
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn keys_become_hotkeys() {
            let hotkey = to_hotkey(&Keys::parse("Super+F").unwrap());
            assert!(hotkey.matches(Modifiers::META, Code::KeyF));
            let hotkey = to_hotkey(&Keys::parse("Ctrl+Alt+Space").unwrap());
            assert!(hotkey.matches(Modifiers::CONTROL | Modifiers::ALT, Code::Space));
            let hotkey = to_hotkey(&Keys::parse("F12").unwrap());
            assert!(hotkey.matches(Modifiers::empty(), Code::F12));
            assert_eq!(letter_code('z'), Code::KeyZ);
            assert_eq!(digit_code('7'), Code::Digit7);
        }
    }
}

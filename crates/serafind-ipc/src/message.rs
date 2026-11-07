//! The messages. Their JSON form is the protocol: field names and tags must stay stable.

use serde::{Deserialize, Serialize};

/// What a client asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Open the bar, or close it when it is open.
    Toggle {
        /// A Wayland activation token, so that the bar gets the keyboard focus.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activation_token: Option<String>,
    },
    /// Open the bar.
    Show {
        /// A Wayland activation token, so that the bar gets the keyboard focus.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activation_token: Option<String>,
    },
    /// Close the bar.
    Hide,
    /// Stop the resident process (and the bar).
    Quit,
    /// How the resident process is doing.
    Status,
    /// Search the index.
    Search {
        /// The search line, as typed.
        line: String,
        /// The mode chosen in the bar (a mode filter of the line wins).
        mode: Mode,
        /// Results wanted at most.
        max: u32,
    },
    /// From the bar: it is shown or hidden now (it closed itself with Escape, lost the focus...).
    BarVisible {
        /// True when shown.
        visible: bool,
    },
    /// From the bar: receive [`Event`]s on this connection from now on.
    Subscribe,
    /// Do something with a file or folder.
    Act {
        /// What.
        action: Action,
        /// The file or folder.
        path: String,
        /// A Wayland activation token, so that the opened window gets the focus.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activation_token: Option<String>,
    },
}

/// What [`Request::Act`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Open with the default application.
    Open,
    /// Show in the file manager, selected in its folder.
    Reveal,
    /// Open a terminal in the folder (the folder of a file).
    Terminal,
    /// Start an application; the path is the id of [`Row::app`].
    Launch,
}

/// The answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Done.
    Ok,
    /// Not done.
    Error {
        /// Why.
        message: String,
    },
    /// Answer to [`Request::Status`].
    Status(Status),
    /// Answer to [`Request::Search`].
    Results(Results),
}

/// Sent by the resident process to a subscribed bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Show the bar (it was kept hidden).
    Show {
        /// A Wayland activation token, when the shortcut gave one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activation_token: Option<String>,
    },
    /// Hide (or close) the bar.
    Hide,
    /// Close the bar for good.
    Quit,
}

/// The state of the resident process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent yes/no facts of a report"
)]
pub struct Status {
    /// Its process id.
    pub pid: u32,
    /// Its version.
    pub version: String,
    /// [`crate::PROTOCOL_VERSION`].
    pub protocol: u32,
    /// True once the first walk is done.
    pub index_ready: bool,
    /// Entries in the index.
    pub entries: u64,
    /// A bar process is connected.
    pub bar_running: bool,
    /// The bar is shown.
    pub bar_visible: bool,
    /// The global shortcut in use, as the desktop describes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// Why there is no global shortcut, and what to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut_problem: Option<String>,
    /// The notification area icon is turned on (`[tray] enabled`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tray_enabled: bool,
    /// Why the turned-on icon is not shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_problem: Option<String>,
}

/// The modes of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Apps, folders and files.
    All,
    /// Applications.
    Apps,
    /// Folders.
    Folders,
    /// Files.
    Files,
    /// Text inside files.
    Content,
}

/// What a result is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// An application.
    App,
    /// A folder.
    Folder,
    /// A file.
    File,
}

/// One result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Name shown.
    pub name: String,
    /// Full path.
    pub path: String,
    /// Parent folder, shortened (`~/Documents`).
    pub location: String,
    /// What it is.
    pub kind: Kind,
    /// Characters of the name to highlight.
    pub highlights: Vec<u32>,
    /// For an application: its id, to start it with [`Action::Launch`]. `path` is then its
    /// program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// For an application: its icon file (PNG or SVG).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Answer to a search.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Results {
    /// The mode the search ran in (a mode filter of the line wins over the asked mode).
    pub mode: Option<Mode>,
    /// The best results, best first.
    pub rows: Vec<Row>,
    /// How many matched in all.
    pub total: u64,
    /// The rows come from the fuzzy fallback.
    pub fuzzy: bool,
    /// `in:` names a folder outside the index.
    pub outside_index: bool,
    /// The index is still being built.
    pub indexing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(Request::Toggle { activation_token: None }, r#"{"type":"toggle"}"#)]
    #[case(Request::Show { activation_token: Some("x".into()) }, r#"{"type":"show","activation_token":"x"}"#)]
    #[case(Request::Quit, r#"{"type":"quit"}"#)]
    #[case(
        Request::Search { line: "invoice ext:pdf".into(), mode: Mode::All, max: 50 },
        r#"{"type":"search","line":"invoice ext:pdf","mode":"all","max":50}"#
    )]
    #[case(Request::BarVisible { visible: false }, r#"{"type":"bar_visible","visible":false}"#)]
    #[case(
        Request::Act { action: Action::Reveal, path: "/home/me/a.pdf".into(), activation_token: None },
        r#"{"type":"act","action":"reveal","path":"/home/me/a.pdf"}"#
    )]
    fn requests_are_stable_json(#[case] request: Request, #[case] json: &str) {
        assert_eq!(serde_json::to_string(&request).unwrap(), json);
        assert_eq!(serde_json::from_str::<Request>(json).unwrap(), request);
    }

    #[test]
    fn responses_are_stable_json() {
        let results = Response::Results(Results {
            mode: Some(Mode::Files),
            rows: vec![Row {
                name: "facture.pdf".into(),
                path: "/home/me/facture.pdf".into(),
                location: "~".into(),
                kind: Kind::File,
                highlights: vec![0, 1],
                app: None,
                icon: None,
            }],
            total: 1,
            ..Results::default()
        });
        insta::assert_snapshot!(serde_json::to_string_pretty(&results).unwrap());
        assert_eq!(serde_json::to_string(&Response::Ok).unwrap(), r#"{"type":"ok"}"#);
        assert_eq!(serde_json::to_string(&Event::Hide).unwrap(), r#"{"type":"hide"}"#);
    }

    #[test]
    fn unknown_messages_are_errors() {
        assert!(serde_json::from_str::<Request>(r#"{"type":"format_disk"}"#).is_err());
    }
}

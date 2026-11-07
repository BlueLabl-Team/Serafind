//! The local protocol of Serafind.
//!
//! The resident process listens on a local socket (a Unix socket in the user's private runtime
//! folder on Linux, a named pipe on Windows). Clients send [`Request`]s and read [`Response`]s:
//! JSON objects, one per line, tagged by `"type"`. A client that sends [`Request::Subscribe`]
//! then also receives [`Event`]s on that connection (the bar uses it to be told to show or hide).
//!
//! The protocol is plain JSON on purpose: tools and third-party integrations can use it from any
//! language (`socat`, Python, ...).
//!
//! ```text
//! → {"type":"search","line":"invoice ext:pdf","mode":"all","max":50}
//! ← {"type":"results","total":3,"fuzzy":false,"outside_index":false,"indexing":false,"rows":[...]}
//! → {"type":"toggle"}
//! ← {"type":"ok"}
//! ```

mod message;
mod transport;

pub use message::{Event, Kind, Mode, Request, Response, Results, Row, Status};
pub use transport::{
    Client, Connection, Listener, SOCKET_VARIABLE, Stream, read_message, socket_location, write_message,
};

/// Version of the protocol, sent in [`Status`]. Raised when a message changes incompatibly.
pub const PROTOCOL_VERSION: u32 = 1;

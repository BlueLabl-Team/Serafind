//! The local socket and its framing: one JSON message per line.

use std::io::{self, BufRead, BufReader, Read, Write};

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ListenerOptions, Name};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;

use crate::{Request, Response};

/// Longest message read, in bytes: far more than any request or answer.
const MAX_MESSAGE: u64 = 4 * 1024 * 1024;

/// Environment variable that replaces the socket location (tests, several instances).
pub const SOCKET_VARIABLE: &str = "SERAFIND_SOCKET";

/// Where the resident process listens: a socket file on Unix (in the user's private runtime
/// folder), a pipe name on Windows.
#[must_use]
pub fn socket_location() -> String {
    #[cfg(test)]
    if let Some(location) = tests::LOCATION_OVERRIDE.with(|cell| cell.borrow().clone()) {
        return location;
    }
    if let Some(location) = std::env::var_os(SOCKET_VARIABLE) {
        return location.to_string_lossy().into_owned();
    }
    default_location()
}

#[cfg(unix)]
fn default_location() -> String {
    // $XDG_RUNTIME_DIR is private to the user (mode 0700); otherwise a private folder in /tmp.
    let folder = dirs::runtime_dir().unwrap_or_else(|| {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".to_owned());
        std::env::temp_dir().join(format!("serafind-{user}"))
    });
    folder.join("serafind.sock").to_string_lossy().into_owned()
}

#[cfg(windows)]
fn default_location() -> String {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".to_owned());
    format!("serafind-{user}")
}

#[cfg(unix)]
fn socket_name(location: &str) -> io::Result<Name<'static>> {
    use interprocess::local_socket::GenericFilePath;
    let path = std::path::PathBuf::from(location);
    if let Some(folder) = path.parent() {
        create_private_folder(folder)?;
    }
    path.to_fs_name::<GenericFilePath>().map(Name::into_owned)
}

#[cfg(unix)]
fn create_private_folder(folder: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    if let Ok(metadata) = std::fs::symlink_metadata(folder) {
        // The fallback folder in the shared temp folder (no private runtime folder): another
        // user could have created it first.
        let fallback = folder.parent() == Some(std::env::temp_dir().as_path())
            && folder
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("serafind-"));
        if fallback
            && (!metadata.is_dir()
                || metadata.uid() != crate::peer::this_user()
                || metadata.mode() & 0o077 != 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{} is not a private folder of this user: remove it, or set XDG_RUNTIME_DIR",
                    folder.display()
                ),
            ));
        }
        return Ok(());
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
}

#[cfg(windows)]
fn socket_name(location: &str) -> io::Result<Name<'static>> {
    use interprocess::local_socket::GenericNamespaced;
    location
        .to_owned()
        .to_ns_name::<GenericNamespaced>()
        .map(Name::into_owned)
}

/// Writes one message and its line end, then flushes.
///
/// # Errors
/// When the connection is broken.
pub fn write_message(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let mut line = serde_json::to_vec(message).map_err(io::Error::other)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()
}

/// Reads one message; `None` when the other side closed the connection.
///
/// # Errors
/// When the connection is broken or the line is not a valid message.
pub fn read_message<T: DeserializeOwned>(reader: &mut impl BufRead) -> io::Result<Option<T>> {
    let mut line = String::new();
    let length = reader.take(MAX_MESSAGE).read_line(&mut line)?;
    if length == 0 {
        return Ok(None);
    }
    if !line.ends_with('\n') && u64::try_from(length).unwrap_or(u64::MAX) >= MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a message is longer than {MAX_MESSAGE} bytes"),
        ));
    }
    serde_json::from_str(line.trim_end())
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// The resident process's side: accepts connections.
pub struct Listener {
    inner: LocalSocketListener,
    location: String,
}

impl Listener {
    /// Listens at [`socket_location`]. A socket left behind by a process that crashed is
    /// replaced; check first with [`Client::connect`] that no process is listening.
    ///
    /// # Errors
    /// When the socket cannot be created.
    pub fn bind() -> io::Result<Self> {
        let location = socket_location();
        let inner = ListenerOptions::new()
            .name(socket_name(&location)?)
            .try_overwrite(true)
            .create_sync()?;
        Ok(Self { inner, location })
    }

    /// Where it listens.
    #[must_use]
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Waits for the next client.
    ///
    /// # Errors
    /// When accepting fails.
    /// Clients of another user are refused (`PermissionDenied`).
    pub fn accept(&self) -> io::Result<Connection> {
        let stream = self.inner.accept()?;
        crate::peer::check_same_user(&stream)?;
        Ok(Connection::new(stream))
    }
}

/// A shared handle on a connection's stream: one can read with it while another writes.
#[derive(Clone)]
pub struct Stream(Arc<LocalSocketStream>);

impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        (&*self.0).read(buffer)
    }
}

impl Write for Stream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        (&*self.0).write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self.0).flush()
    }
}

/// One connection, either side: read and write messages.
pub struct Connection {
    reader: BufReader<Stream>,
    writer: Stream,
}

impl Connection {
    fn new(stream: LocalSocketStream) -> Self {
        let stream = Stream(Arc::new(stream));
        Self {
            reader: BufReader::new(stream.clone()),
            writer: stream,
        }
    }

    /// Reads the next message; `None` when the other side closed.
    ///
    /// # Errors
    /// When the connection is broken or the message is not understood.
    pub fn read<T: DeserializeOwned>(&mut self) -> io::Result<Option<T>> {
        read_message(&mut self.reader)
    }

    /// Writes a message.
    ///
    /// # Errors
    /// When the connection is broken.
    pub fn write(&mut self, message: &impl Serialize) -> io::Result<()> {
        write_message(&mut self.writer, message)
    }

    /// Splits into a reader and a writer, to read in one thread and write from another.
    #[must_use]
    pub fn into_parts(self) -> (BufReader<Stream>, Stream) {
        (self.reader, self.writer)
    }
}

/// A client of the resident process.
pub struct Client {
    connection: Connection,
}

impl Client {
    /// Connects to the resident process.
    ///
    /// # Errors
    /// When no resident process listens (`NotFound` or `ConnectionRefused`).
    pub fn connect() -> io::Result<Self> {
        Self::connect_to(&socket_location())
    }

    /// Connects to a resident process listening at `location` (a socket path on Unix, a pipe
    /// name on Windows), e.g. another instance.
    ///
    /// # Errors
    /// When nothing listens there.
    pub fn connect_to(location: &str) -> io::Result<Self> {
        let stream = LocalSocketStream::connect(socket_name(location)?)?;
        // Another user may have taken the place first: never talk to it.
        crate::peer::check_same_user(&stream)?;
        Ok(Self {
            connection: Connection::new(stream),
        })
    }

    /// Sends a request and waits for its response.
    ///
    /// # Errors
    /// When the connection is broken or closed before the response.
    pub fn call(&mut self, request: &Request) -> io::Result<Response> {
        self.connection.write(request)?;
        self.connection.read()?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the resident process closed the connection",
            )
        })
    }

    /// The connection, for [`Request::Subscribe`] (events then arrive on it).
    #[must_use]
    pub fn into_connection(self) -> Connection {
        self.connection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Event, Mode};

    /// A socket of its own for each test.
    fn private_location(name: &str) -> String {
        if cfg!(windows) {
            format!("serafind-test-{name}-{}", std::process::id())
        } else {
            std::env::temp_dir()
                .join(format!("serafind-test-{}", std::process::id()))
                .join(format!("{name}.sock"))
                .to_string_lossy()
                .into_owned()
        }
    }

    #[test]
    fn requests_and_responses_go_through() {
        let location = private_location("roundtrip");
        let listener = with_location(&location, Listener::bind).unwrap();
        let server = std::thread::spawn(move || {
            let mut connection = listener.accept().unwrap();
            while let Some(request) = connection.read::<Request>().unwrap() {
                let response = match request {
                    Request::Search { line, .. } => Response::Error { message: line },
                    Request::Subscribe => {
                        connection.write(&Response::Ok).unwrap();
                        connection.write(&Event::Hide).unwrap();
                        continue;
                    }
                    _ => Response::Ok,
                };
                connection.write(&response).unwrap();
            }
        });

        let mut client = with_location(&location, Client::connect).unwrap();
        assert_eq!(client.call(&Request::Quit).unwrap(), Response::Ok);
        let search = Request::Search {
            line: "été".into(),
            mode: Mode::All,
            max: 5,
        };
        assert_eq!(
            client.call(&search).unwrap(),
            Response::Error {
                message: "été".into()
            }
        );
        assert_eq!(client.call(&Request::Subscribe).unwrap(), Response::Ok);
        let mut connection = client.into_connection();
        assert_eq!(connection.read::<Event>().unwrap(), Some(Event::Hide));
        drop(connection);
        server.join().unwrap();
    }

    #[test]
    fn nobody_listening() {
        let location = private_location("nobody");
        assert!(with_location(&location, Client::connect).is_err());
    }

    /// Runs `action` with the socket location given directly (no environment variable: tests run
    /// in parallel).
    fn with_location<T>(location: &str, action: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
        LOCATION_OVERRIDE.with(|cell| *cell.borrow_mut() = Some(location.to_owned()));
        let result = action();
        LOCATION_OVERRIDE.with(|cell| *cell.borrow_mut() = None);
        result
    }

    thread_local! {
        pub(super) static LOCATION_OVERRIDE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    }

    #[cfg(unix)]
    #[test]
    fn a_fallback_folder_that_is_not_private_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!("serafind-shared-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o777)).unwrap();
        let location = folder.join("serafind.sock").to_string_lossy().into_owned();
        let error = socket_name(&location).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(socket_name(&location).is_ok(), "private: accepted");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn too_long_messages_are_refused() {
        let mut line = vec![b'x'; usize::try_from(MAX_MESSAGE).unwrap() + 10];
        line.push(b'\n');
        let error = read_message::<Request>(&mut line.as_slice()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("longer than"));
    }
}

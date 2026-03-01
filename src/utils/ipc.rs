use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{bail, Context};
use serde_json::Value;
use shellexpand::tilde;

static SOCKFILE: OnceLock<PathBuf> = OnceLock::new();

/// Returns the cached or newly discovered path to the Qtile IPC socket.
fn get_sockfile() -> &'static Path {
    SOCKFILE.get_or_init(|| find_sockfile(None))
}

/// Discovers the Qtile socket file path by checking the following in order:
/// 1.  An explicitly provided display name.
/// 2.  `WAYLAND_DISPLAY` environment variable.
/// 3.  `DISPLAY` environment variable.
/// 4.  Default locations for Wayland (`wayland-0`) and X11 (`:0`).
fn find_sockfile(display: Option<String>) -> PathBuf {
    let xdg_cache_home = std::env::var("XDG_CACHE_HOME").unwrap_or(tilde("~/.cache").to_string());
    let cache_dir = Path::new(&xdg_cache_home);
    let sockfile: PathBuf;
    match display {
        Some(s) => {
            sockfile = cache_dir.join("qtile").join(format!("qtilesocket.{s}"));
            sockfile
        }
        None => match std::env::var("WAYLAND_DISPLAY") {
            Ok(s) => {
                sockfile = cache_dir.join("qtile").join(format!("qtilesocket.{s}"));
                sockfile
            }
            Err(_) => match std::env::var("DISPLAY") {
                Ok(s) => {
                    sockfile = cache_dir.join("qtile").join(format!("qtilesocket.{s}"));
                    sockfile
                }
                Err(_) => {
                    let mut sockfile = cache_dir.join("qtile").join("qtilesocket.wayland-0");
                    if std::path::Path::exists(&sockfile) {
                        return sockfile;
                    }

                    sockfile = cache_dir.join("qtile").join("qtilesocket.:0");
                    if std::path::Path::exists(&sockfile) {
                        return sockfile;
                    }

                    sockfile
                }
            },
        },
    }
}

/// Client for communicating with the Qtile IPC socket.
pub struct Client {}
impl Client {
    /// Connects to the socket, sends the payload, and returns the raw response.
    pub fn send(data: String) -> anyhow::Result<String> {
        let sockfile = get_sockfile();
        let mut stream = UnixStream::connect(sockfile)
            .with_context(|| format!("could not connect to socket: {sockfile:?}"))?;
        stream.write_all(data.as_bytes())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .context("Could not shutdown writing on the stream")?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }

    /// Validates and parses the Qtile IPC response.
    ///
    /// The response is expected to be a JSON array where the first element
    /// is a status code (0 for success) and the second is the result or error message.
    pub fn match_response(response: anyhow::Result<String>) -> anyhow::Result<Value> {
        match response {
            Ok(response) => match serde_json::from_str(&response) {
                Ok(s) => match s {
                    Value::Array(array) => {
                        let status = &array[0];
                        let result = &array[1];
                        match status {
                            Value::Number(n) => {
                                let n = n.as_u64().context("ipc.Client: invalid status code")?;
                                match n {
                                    0 => Ok(result.clone()),
                                    n => match result {
                                        Value::String(s) => {
                                            bail!("ipc.Client: response_code = {n}:\n{s:#}")
                                        }
                                        Value::Null
                                        | Value::Bool(_)
                                        | Value::Number(_)
                                        | Value::Array(_)
                                        | Value::Object(_) => {
                                            bail!("ipc.Client: bad response by qtile!?")
                                        }
                                    },
                                }
                            }
                            Value::Null
                            | Value::Bool(_)
                            | Value::String(_)
                            | Value::Array(_)
                            | Value::Object(_) => {
                                bail!("ipc.Client: bad response by qtile!?")
                            }
                        }
                    }
                    Value::Null
                    | Value::Bool(_)
                    | Value::String(_)
                    | Value::Object(_)
                    | Value::Number(_) => {
                        bail!("ipc.Client: bad response by qtile!?")
                    }
                },
                Err(err) => bail!("ipc.Client: {err}"),
            },
            Err(err) => bail!("{err}"),
        }
    }
}

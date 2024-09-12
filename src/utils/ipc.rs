use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use serde_json::Value;
use shellexpand::tilde;

/// Finds the appropriate socket file for the given display.
///
/// If unspecified, the socket file is determined as follows:
///
///    - If WAYLAND_DISPLAY is set, use it.
///    - else if DISPLAY is set, use that.
///    - else check for the existence of a socket file for WAYLAND_DISPLAY=wayland-0
///      and if it exists, use it.
///    - else check for the existence of a socket file for DISPLAY=:0
///      and if it exists, use it.
fn find_sockfile(display: Option<String>) -> PathBuf {
    let xdg_cache_home = std::env::var("XDG_CACHE_HOME").unwrap_or(tilde("~/.cache").to_string());
    let cache_dir = Path::new(&xdg_cache_home);
    let sockfile: PathBuf;
    match display {
        Some(s) => {
            sockfile = cache_dir.join("qtile").join(format!("qtilesocket.{}", s));
            // eprintln!("sockfile: {sockfile:#?}");
            sockfile
        }
        None => match std::env::var("WAYLAND_DISPLAY") {
            Ok(s) => {
                sockfile = cache_dir.join("qtile").join(format!("qtilesocket.{}", s));
                // eprintln!("sockfile: {sockfile:#?}");
                sockfile
            }
            Err(_) => match std::env::var("DISPLAY") {
                Ok(s) => {
                    sockfile = cache_dir.join("qtile").join(format!("qtilesocket.{}", s));
                    // eprintln!("sockfile: {sockfile:#?}");
                    sockfile
                }
                Err(_) => {
                    let mut sockfile = cache_dir
                        .join("qtile")
                        .join(format!("qtilesocket.{}", "wayland-0"));
                    if std::path::Path::exists(&sockfile) {
                        // eprintln!("sockfile: {sockfile:#?}");
                        return sockfile;
                    }

                    sockfile = cache_dir
                        .join("qtile")
                        .join(format!("qtilesocket.{}", ":0"));
                    if std::path::Path::exists(&sockfile) {
                        // eprintln!("sockfile: {sockfile:#?}");
                        return sockfile;
                    }

                    // eprintln!("sockfile: {sockfile:#?}");
                    sockfile
                }
            },
        },
    }
}

/// IPC client which is used for sending the "requests" to `qtile`'s socket
pub struct Client {}
impl Client {
    /// Send the message to the server.
    ///
    /// Connect to the server, then pack and send the message to the server,
    /// then wait for and return the response from the server.
    pub fn send(data: String) -> anyhow::Result<String> {
        let mut stream =
            UnixStream::connect(find_sockfile(None)).expect("could not connect to socket");
        stream.write_all(data.as_bytes())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .context("Could not shutdown writing on the stream")?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }
    /// Match a response from a [`String`] to a [`serde_json::Value`] based on how qtile should or
    /// shouldn't respond
    pub fn match_response(response: anyhow::Result<String>) -> anyhow::Result<Value> {
        match response {
            Ok(response) => match serde_json::from_str(&response) {
                Ok(s) => match s {
                    Value::Array(array) => {
                        let status = &array[0];
                        let result = &array[1];
                        match status {
                            Value::Number(n) => {
                                let n = n.as_u64().unwrap();
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

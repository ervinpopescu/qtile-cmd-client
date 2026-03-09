use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use serde_json::{json, Value};
use shellexpand::tilde;

/// Discovers the Qtile socket file path by checking the following in order:
/// 1.  An explicitly provided display name.
/// 2.  `WAYLAND_DISPLAY` environment variable.
/// 3.  `DISPLAY` environment variable.
/// 4.  Default locations for Wayland (`wayland-0`) and X11 (`:0`).
pub(crate) fn find_sockfile(display: Option<String>) -> PathBuf {
    let xdg_cache_home = std::env::var("XDG_CACHE_HOME").unwrap_or(tilde("~/.cache").to_string());
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let x11_display = std::env::var("DISPLAY").ok();

    find_sockfile_with_env(display, Some(xdg_cache_home), wayland_display, x11_display)
}

pub(crate) fn find_sockfile_with_env(
    display: Option<String>,
    xdg_cache_home: Option<String>,
    wayland_display: Option<String>,
    x11_display: Option<String>,
) -> PathBuf {
    let xdg_cache_home = xdg_cache_home.unwrap_or_else(|| tilde("~/.cache").to_string());
    let cache_dir = Path::new(&xdg_cache_home);

    match display {
        Some(s) => cache_dir.join("qtile").join(format!("qtilesocket.{s}")),
        None => {
            if let Some(s) = wayland_display {
                return cache_dir.join("qtile").join(format!("qtilesocket.{s}"));
            }
            if let Some(s) = x11_display {
                return cache_dir.join("qtile").join(format!("qtilesocket.{s}"));
            }

            for default_display in ["wayland-0", ":0", ":99"] {
                let sockfile = cache_dir
                    .join("qtile")
                    .join(format!("qtilesocket.{default_display}"));
                if std::path::Path::exists(&sockfile) {
                    return sockfile;
                }
            }
            // Fallback to wayland-0 if nothing found
            cache_dir.join("qtile").join("qtilesocket.wayland-0")
        }
    }
}

/// Client for communicating with the Qtile IPC socket.
pub struct Client {
    stream: UnixStream,
    #[allow(dead_code)]
    framed: bool,
}

impl Client {
    #[allow(dead_code)]
    pub fn new_from_stream(stream: UnixStream, framed: bool) -> Self {
        Self { stream, framed }
    }

    #[allow(dead_code)]
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_path(None)
    }

    pub fn new_with_path(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let sockfile = path.unwrap_or_else(|| find_sockfile(None));
        let stream = UnixStream::connect(&sockfile)
            .with_context(|| format!("could not connect to socket: {sockfile:?}"))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .context("Could not set read timeout on the stream")?;
        Ok(Self {
            stream,
            framed: false,
        })
    }

    pub fn connect(framed: bool) -> anyhow::Result<Self> {
        Self::connect_with_path(None, framed)
    }

    pub fn connect_with_path(path: Option<PathBuf>, framed: bool) -> anyhow::Result<Self> {
        let sockfile = path.unwrap_or_else(|| find_sockfile(None));
        let stream = UnixStream::connect(&sockfile)
            .with_context(|| format!("could not connect to socket: {sockfile:?}"))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .context("Could not set read timeout on the stream")?;
        Ok(Self { stream, framed })
    }

    /// Send a message and get a response using a one-off connection.
    pub fn send_request(data: String, framed: bool) -> anyhow::Result<String> {
        let res = Self::try_send_request(data.clone(), framed);

        // If we tried unframed and got EOF/empty response, retry with framing.
        // This handles servers that have dropped unframed legacy support entirely.
        if !framed {
            match res {
                Ok(ref s) if s.is_empty() => {
                    return Self::try_send_request(data, true);
                }
                Err(_) => {
                    // On connection/read error, we could also try to retry with framing
                    // but usually EOF manifests as Ok("") or specific IO error.
                    return Self::try_send_request(data, true);
                }
                _ => {}
            }
        }
        res
    }

    fn try_send_request(data: String, framed: bool) -> anyhow::Result<String> {
        let mut client = Self::connect(framed)?;
        if framed {
            // NEW JSON IPC: Wrap our structured object in the protocol envelope
            // We expect 'data' to already be the JSON-serialized CommandParser object
            let payload = json!({
                "message_type": "command",
                "content": serde_json::from_str::<Value>(&data).context("Invalid JSON payload")?
            });
            let framed_data = serde_json::to_string(&payload)?;
            client.write_frame(framed_data.as_bytes())?;
            let response = client.read_frame()?;
            Ok(String::from_utf8(response)?)
        } else {
            // UNFRAMED IPC (Legacy or JSON): Send raw data and read until EOF
            let mut stream = client.stream;
            stream.write_all(data.as_bytes())?;
            stream
                .shutdown(std::net::Shutdown::Write)
                .context("Could not shutdown writing on the stream")?;
            let mut response = String::new();
            stream.read_to_string(&mut response)?;
            Ok(response)
        }
    }

    /// Write a framed message (4-byte BE length prefix + payload).
    pub fn write_frame(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let len = data.len() as u32;
        self.stream.write_all(&len.to_be_bytes())?;
        self.stream.write_all(data)?;
        Ok(())
    }

    /// Read a framed message.
    pub fn read_frame(&mut self) -> anyhow::Result<Vec<u8>> {
        let mut header = [0u8; 4];
        self.stream
            .read_exact(&mut header)
            .context("Failed to read frame header")?;
        let len = u32::from_be_bytes(header) as usize;
        let mut buffer = vec![0u8; len];
        self.stream
            .read_exact(&mut buffer)
            .context("Failed to read frame body")?;
        Ok(buffer)
    }

    /// Legacy wrapper for backward compatibility.
    /// Uses the unframed, EOF-terminated protocol.
    #[allow(dead_code)]
    pub fn send(data: String) -> anyhow::Result<String> {
        // This is tricky: legacy 'send' expects tuple serialization.
        // We'll let the caller handle the serialization format for now.
        Self::send_request(data, false)
    }

    /// Validates and parses the Qtile IPC response.
    pub fn match_response(
        response: anyhow::Result<String>,
        _framed: bool,
    ) -> anyhow::Result<Value> {
        let response = response?;
        let mut s: Value = serde_json::from_str(&response).context("ipc.Client: invalid JSON")?;

        // Handle enveloped 'reply' message if present (new JSON IPC)
        if let Value::Object(ref mut map) = s {
            if map.get("message_type") == Some(&json!("reply")) {
                if let Some(content) = map.remove("content") {
                    s = content;
                }
            }
        }

        let s = Self::decode_qtile_tuples(s);
        match s {
            // NEW JSON IPC or Modern JSON response format
            Value::Object(map) => {
                let status = map
                    .get("status")
                    .context("ipc.Client: missing 'status' in reply content")?;
                // The 'json_ipc' branch uses 'data', others might use 'result'
                let result = map
                    .get("result")
                    .or_else(|| map.get("data"))
                    .unwrap_or(&Value::Null);

                match status {
                    Value::Number(n) => {
                        let n = n.as_u64().context("ipc.Client: invalid status code")?;
                        match n {
                            0 => Ok(result.clone()),
                            n => match result {
                                Value::String(s) => {
                                    bail!("ipc.Client: response_code = {n}:\n{s:#}")
                                }
                                Value::Array(arr) => {
                                    let msg = arr
                                        .iter()
                                        .map(|v| v.as_str().unwrap_or(""))
                                        .collect::<Vec<_>>()
                                        .join("");
                                    bail!("ipc.Client: response_code = {n}:\n{msg}")
                                }
                                _ => {
                                    bail!("ipc.Client: bad response by qtile!? (Object branch, result type unexpected: {result:?})")
                                }
                            },
                        }
                    }
                    _ => bail!("ipc.Client: status is not a number: {status:?}"),
                }
            }
            // LEGACY IPC or old-style JSON response format
            Value::Array(array) => {
                if array.is_empty() {
                    bail!("ipc.Client: received empty response array");
                }
                let status = &array[0];
                let result = if array.len() > 1 {
                    &array[1]
                } else {
                    &Value::Null
                };

                match status {
                    Value::Number(n) => {
                        let n = n.as_u64().context("ipc.Client: invalid status code")?;
                        match n {
                            0 => Ok(result.clone()),
                            n => match result {
                                Value::String(s) => {
                                    bail!("ipc.Client: response_code = {n}:\n{s:#}")
                                }
                                Value::Array(arr) => {
                                    let msg = arr
                                        .iter()
                                        .map(|v| v.as_str().unwrap_or(""))
                                        .collect::<Vec<_>>()
                                        .join("");
                                    bail!("ipc.Client: response_code = {n}:\n{msg}")
                                }
                                _ => {
                                    bail!("ipc.Client: bad response by qtile!? (Array branch, result type unexpected: {result:?})")
                                }
                            },
                        }
                    }
                    _ => bail!("ipc.Client: status is not a number: {status:?}"),
                }
            }
            _ => bail!("ipc.Client: bad response by qtile!? (Unknown top-level: {s:?})"),
        }
    }

    /// Recursively decode {"$tuple": [...]} into simple arrays.
    fn decode_qtile_tuples(val: Value) -> Value {
        match val {
            Value::Object(mut map) => {
                if map.len() == 1 {
                    if let Some(Value::Array(arr)) = map.remove("$tuple") {
                        return Value::Array(
                            arr.into_iter().map(Self::decode_qtile_tuples).collect(),
                        );
                    }
                }
                Value::Object(
                    map.into_iter()
                        .map(|(k, v)| (k, Self::decode_qtile_tuples(v)))
                        .collect(),
                )
            }
            Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Self::decode_qtile_tuples).collect())
            }
            _ => val,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_find_sockfile() {
        let cache = Some("/tmp/qtile-test-cache".to_string());

        let sock = find_sockfile_with_env(Some(":99".to_string()), cache.clone(), None, None);
        assert!(sock.to_str().unwrap().contains("qtilesocket.:99"));

        let sock_wl =
            find_sockfile_with_env(None, cache.clone(), Some("wayland-test".into()), None);
        assert!(sock_wl
            .to_str()
            .unwrap()
            .contains("qtilesocket.wayland-test"));

        let sock_x11 = find_sockfile_with_env(None, cache, None, Some(":1.0".into()));
        assert!(sock_x11.to_str().unwrap().contains("qtilesocket.:1.0"));
    }

    #[test]
    fn test_match_response_new_json_ipc() {
        // Enveloped 'reply' message from new JSON IPC (using 'result')
        let payload = json!({
            "message_type": "reply",
            "content": {
                "status": 0,
                "result": {"version": "0.1.dev"}
            }
        });
        let response = serde_json::to_string(&payload).unwrap();

        let result = Client::match_response(Ok(response), true)
            .expect("Should parse new JSON IPC reply with 'result'");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_json_ipc_data() {
        // Enveloped 'reply' message from new JSON IPC (using 'data')
        let payload = json!({
            "message_type": "reply",
            "content": {
                "status": 0,
                "data": {"version": "0.1.dev"}
            }
        });
        let response = serde_json::to_string(&payload).unwrap();

        let result = Client::match_response(Ok(response), true)
            .expect("Should parse new JSON IPC reply with 'data'");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_legacy() {
        // Legacy tuple-based response: [status, result]
        let response = "[0, {\"version\": \"0.1.dev\"}]".to_string();
        let result =
            Client::match_response(Ok(response), false).expect("Should parse legacy IPC reply");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_flexible() {
        let legacy_response = "[0, \"ok\"]".to_string();
        let modern_modern = json!({"status": 0, "result": "ok"}).to_string();

        assert_eq!(
            Client::match_response(Ok(legacy_response), true).unwrap(),
            "ok"
        );
        assert_eq!(
            Client::match_response(Ok(modern_modern), false).unwrap(),
            "ok"
        );
    }
}

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
}

impl Client {
    pub fn connect() -> anyhow::Result<Self> {
        Self::connect_with_path(None)
    }

    pub fn connect_with_path(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let sockfile = path.unwrap_or_else(|| find_sockfile(None));
        let stream = UnixStream::connect(&sockfile)
            .with_context(|| format!("could not connect to socket: {sockfile:?}"))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .context("Could not set read timeout on the stream")?;
        Ok(Self { stream })
    }

    /// Send a message and get a response using a one-off connection.
    pub fn send_request(data: String) -> anyhow::Result<String> {
        let mut client = Self::connect()?;
        client.stream.write_all(data.as_bytes())?;
        client
            .stream
            .shutdown(std::net::Shutdown::Write)
            .context("Could not shutdown writing on the stream")?;
        let mut response = String::new();
        client.stream.read_to_string(&mut response)?;
        Ok(response)
    }

    /// Legacy wrapper for backward compatibility.
    #[allow(dead_code)]
    pub fn send(data: String) -> anyhow::Result<String> {
        Self::send_request(data)
    }

    /// Validates and parses the Qtile IPC response.
    pub fn match_response(response: anyhow::Result<String>) -> anyhow::Result<Value> {
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
            // Modern JSON response format
            Value::Object(map) => {
                // Qtile may return {"error": "..."} directly (e.g. "Session locked.")
                if let Some(Value::String(e)) = map.get("error") {
                    bail!("{e}");
                }

                let status = map
                    .get("status")
                    .context("ipc.Client: missing 'status' in reply content")?;
                let result = map
                    .get("result")
                    .or_else(|| map.get("data"))
                    .unwrap_or(&Value::Null);

                match status {
                    Value::Number(n) => {
                        let n = n.as_u64().context("ipc.Client: invalid status code")?;
                        match n {
                            0 => Ok(result.clone()),
                            n => bail!("{}", Self::format_error_result(result, n)),
                        }
                    }
                    _ => bail!("ipc.Client: status is not a number: {status:?}"),
                }
            }
            // Legacy tuple-based response format
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
                            n => bail!("{}", Self::format_error_result(result, n)),
                        }
                    }
                    _ => bail!("ipc.Client: status is not a number: {status:?}"),
                }
            }
            _ => bail!("ipc.Client: bad response by qtile!? (Unknown top-level: {s:?})"),
        }
    }

    fn format_error_result(result: &Value, code: u64) -> String {
        match result {
            Value::String(s) => format!("ipc.Client: response_code = {code}:\n{s}"),
            Value::Array(arr) => {
                let msg = arr
                    .iter()
                    .map(|v| v.as_str().unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join("");
                format!("ipc.Client: response_code = {code}:\n{msg}")
            }
            Value::Object(map) => {
                if let Some(Value::String(e)) = map.get("error") {
                    e.clone()
                } else {
                    format!("ipc.Client: response_code = {code}:\n{result:#}")
                }
            }
            _ => format!("ipc.Client: response_code = {code}: {result:?}"),
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
        let payload = json!({
            "message_type": "reply",
            "content": {
                "status": 0,
                "result": {"version": "0.1.dev"}
            }
        });
        let response = serde_json::to_string(&payload).unwrap();
        let result = Client::match_response(Ok(response))
            .expect("Should parse new JSON IPC reply with 'result'");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_json_ipc_data() {
        let payload = json!({
            "message_type": "reply",
            "content": {
                "status": 0,
                "data": {"version": "0.1.dev"}
            }
        });
        let response = serde_json::to_string(&payload).unwrap();
        let result = Client::match_response(Ok(response))
            .expect("Should parse new JSON IPC reply with 'data'");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_legacy() {
        let response = "[0, {\"version\": \"0.1.dev\"}]".to_string();
        let result = Client::match_response(Ok(response)).expect("Should parse legacy IPC reply");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_flexible() {
        let legacy_response = "[0, \"ok\"]".to_string();
        let modern_response = json!({"status": 0, "result": "ok"}).to_string();
        assert_eq!(Client::match_response(Ok(legacy_response)).unwrap(), "ok");
        assert_eq!(Client::match_response(Ok(modern_response)).unwrap(), "ok");
    }

    #[test]
    fn test_match_response_errors() {
        assert!(Client::match_response(Ok("{invalid".into())).is_err());
        assert!(Client::match_response(Ok("[]".into())).is_err());

        let missing_status = json!({"result": "ok"}).to_string();
        assert!(Client::match_response(Ok(missing_status)).is_err());

        let bad_status = json!({"status": "error", "result": "ok"}).to_string();
        assert!(Client::match_response(Ok(bad_status)).is_err());

        let err_resp = json!({"status": 1, "result": "some error"}).to_string();
        let res = Client::match_response(Ok(err_resp));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("some error"));

        let weird_err = json!({"status": 1, "result": 123}).to_string();
        assert!(Client::match_response(Ok(weird_err)).is_err());

        // Qtile "Session locked." style: plain {"error": "..."} object
        let locked = json!({"error": "Session locked."}).to_string();
        let res = Client::match_response(Ok(locked));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Session locked."));

        // Legacy array where result is {"error": "..."}
        let locked_legacy = json!([1, {"error": "Session locked."}]).to_string();
        let res = Client::match_response(Ok(locked_legacy));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Session locked."));
    }

    #[test]
    fn test_decode_qtile_tuples() {
        let input = json!({
            "a": {"$tuple": [1, 2]},
            "b": [3, {"$tuple": [4, 5]}]
        });
        let expected = json!({
            "a": [1, 2],
            "b": [3, [4, 5]]
        });
        assert_eq!(Client::decode_qtile_tuples(input), expected);
    }

    #[test]
    fn test_decode_qtile_tuples_complex() {
        let input = json!({
            "a": {"$tuple": [{"$tuple": [1, 2]}, 3]},
            "b": [{"$tuple": [4, 5]}, {"c": {"$tuple": [6]}}]
        });
        let expected = json!({
            "a": [[1, 2], 3],
            "b": [[4, 5], {"c": [6]}]
        });
        assert_eq!(Client::decode_qtile_tuples(input), expected);
    }

    #[test]
    fn test_send_request_fail() {
        let path = Some(PathBuf::from("/nonexistent/path/qtile/qtilesocket.:0"));
        assert!(Client::connect_with_path(path).is_err());
    }

    #[test]
    #[ignore = "requires live Qtile socket"]
    fn test_client_connect_success() {
        assert!(Client::connect().is_ok());
    }

    #[test]
    fn test_client_connect_fail() {
        let path = Some(PathBuf::from("/nonexistent/path/qtile/qtilesocket.:0"));
        assert!(Client::connect_with_path(path).is_err());
    }
}

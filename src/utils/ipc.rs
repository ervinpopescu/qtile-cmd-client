use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
};

use anyhow::{bail, Context};
use serde_json::Value;

pub struct Client {}
impl Client {
    pub fn send(data: String) -> anyhow::Result<String> {
        let xdg_cache_home = std::env::var("XDG_CACHE_HOME").unwrap_or("~/.cache".to_string());
        let cache_dir = Path::new(&xdg_cache_home);
        let mut stream = UnixStream::connect(cache_dir.join("qtile/qtilesocket.:0"))?;
        stream.write_all(data.as_bytes())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .context("Could not shutdown writing on the stream")?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }
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
                                    1 => bail!("ipc.Client: response_code = 0: {result}"),
                                    _ => bail!("ipc.Client: qtile should return 0/1"),
                                }
                            }
                            Value::Null
                            | Value::Bool(_)
                            | Value::String(_)
                            | Value::Array(_)
                            | Value::Object(_) => bail!("ipc.Client: bad response by qtile!?"),
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

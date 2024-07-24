use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
};

use anyhow::{Context, Ok, Result};

pub struct Client {}
impl Client {
    pub fn send(data: String) -> Result<String> {
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
}

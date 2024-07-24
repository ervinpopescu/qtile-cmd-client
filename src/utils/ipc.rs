use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
};

use anyhow::{Context, Ok, Result};

pub struct Client {}
impl Client {
    pub fn send(data: String) -> Result<String> {
        let mut stream = UnixStream::connect("/home/ervin/.cache/qtile/qtilesocket.:0")?;
        stream.write_all(data.as_bytes())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .context("Could not shutdown writing on the stream")?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }
}

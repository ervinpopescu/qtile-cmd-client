use super::{args::Args, ipc::Client, parser::CommandParser};
use anyhow::bail;
use serde_json::Value;

/// Client used for CLI by `qticc` binary
pub struct ShellClient {}

impl ShellClient {
    /// construct a [`CommandParser`] from CLI args and [`serialize`](serde_tuple::Serialize_tuple) it into a tuple string for sending over the qtile socket
    pub fn call(args: Args) -> anyhow::Result<()> {
        let c = CommandParser::from_args(args.clone())?;
        let data = serde_json::to_string(&c).unwrap();
        let response = Client::send(data.clone());
        let result = Client::match_response(response);
        match result {
            Ok(result) => println!("{result:#}"),
            Err(err) => bail!("{err}"),
        }
        Ok(())
    }
    // pub fn
}

/// Client used by library for executing qtile commands
#[allow(dead_code)]
pub struct InteractiveCommandClient {}
#[allow(dead_code)]
impl InteractiveCommandClient {
    /// construct a [`CommandParser`] from parameters and [`serialize`](serde_tuple::Serialize_tuple) it into a tuple string for sending over the qtile socket
    pub fn call(
        object: Option<Vec<String>>,
        function: Option<String>,
        args: Option<Vec<String>>,
        info: bool,
    ) -> anyhow::Result<Value> {
        let c = CommandParser::from_params(object, function, args, info)?;
        let data = serde_json::to_string(&c).unwrap();
        let response = Client::send(data.clone());
        Client::match_response(response)
    }
}

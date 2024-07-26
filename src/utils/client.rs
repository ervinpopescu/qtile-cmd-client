use super::{args::Args, ipc::Client, parser::CommandParser};
use anyhow::bail;
use serde_json::Value;

pub(crate) struct ShellClient {}
impl ShellClient {
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
pub struct InteractiveCommandClient {}
impl InteractiveCommandClient {
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

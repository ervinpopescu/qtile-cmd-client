use super::{
    ipc::Client,
    parser::{CommandAction, CommandParser},
};
use anyhow::Context;
use serde_json::Value;

pub enum CallResult {
    Value(Value),
    Text(String),
}

/// Client used by library for executing qtile commands
pub struct InteractiveCommandClient {}
impl InteractiveCommandClient {
    /// construct a [`CommandParser`] from parameters and [`serialize`](serde_tuple::Serialize_tuple) it into a tuple string for sending over the qtile socket
    pub fn call(
        object: Option<Vec<String>>,
        function: Option<String>,
        args: Option<Vec<String>>,
        info: bool,
    ) -> anyhow::Result<CallResult> {
        let action = CommandParser::from_params(object, function, args, info)?;
        match action {
            CommandAction::Execute(c) => {
                let data = serde_json::to_string(&c)
                    .context("Failed to serialize CommandParser to JSON")?;
                let response = Client::send(data.clone());
                Client::match_response(response).map(CallResult::Value)
            }
            CommandAction::Help(text) => Ok(CallResult::Text(text)),
        }
    }
}

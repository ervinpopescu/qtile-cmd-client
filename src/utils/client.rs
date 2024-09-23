use super::{ipc::Client, parser::CommandParser};
use serde_json::Value;

/// Client used by library for executing qtile commands
pub struct InteractiveCommandClient {}
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

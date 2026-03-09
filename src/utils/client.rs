use super::{
    ipc::Client,
    parser::{CommandAction, CommandParser},
};
use anyhow::Context;
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum CallResult {
    Value(Value),
    Text(String),
}

impl fmt::Display for CallResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallResult::Value(v) => write!(f, "{v:#}"),
            CallResult::Text(t) => write!(f, "{t}"),
        }
    }
}

impl CallResult {
    /// Returns the result as a JSON Value reference if it's a [`CallResult::Value`].
    #[allow(dead_code)]
    pub fn as_value(&self) -> Option<&Value> {
        match self {
            CallResult::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the result as a string reference if it's a [`CallResult::Text`].
    #[allow(dead_code)]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            CallResult::Text(t) => Some(t),
            _ => None,
        }
    }

    /// Returns the result as a JSON Value.
    /// For [`CallResult::Text`], it returns a JSON string.
    pub fn to_json(&self) -> Value {
        match self {
            CallResult::Value(v) => v.clone(),
            CallResult::Text(t) => Value::String(t.clone()),
        }
    }
}

/// Encapsulates a request to the Qtile command graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandQuery {
    pub object: Option<Vec<String>>,
    pub function: Option<String>,
    pub args: Option<Vec<String>>,
    pub info: bool,
}

impl CommandQuery {
    /// Creates a new, empty query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the target object path.
    pub fn object(mut self, path: Vec<String>) -> Self {
        self.object = Some(path);
        self
    }

    /// Sets the function to call.
    pub fn function(mut self, name: String) -> Self {
        self.function = Some(name);
        self
    }

    /// Sets the arguments for the function call.
    pub fn args(mut self, args: Vec<String>) -> Self {
        self.args = Some(args);
        self
    }

    /// Sets whether to fetch documentation for the function.
    pub fn info(mut self, info: bool) -> Self {
        self.info = info;
        self
    }
}

/// Client for executing commands against a running Qtile instance.
pub struct QtileClient {
    framed: bool,
}

impl QtileClient {
    /// Creates a new client with the specified framing protocol setting.
    pub fn new(framed: bool) -> Self {
        Self { framed }
    }

    /// Returns the framing protocol setting for this client.
    #[allow(dead_code)]
    pub fn framed(&self) -> bool {
        self.framed
    }

    /// Executes a command or fetches help text based on the provided [`CommandQuery`].
    pub fn call(&self, query: CommandQuery) -> anyhow::Result<CallResult> {
        let action = CommandParser::from_params(
            query.object,
            query.function,
            query.args,
            query.info,
            self.framed,
        )?;
        match action {
            CommandAction::Execute(c) => {
                // We ALWAYS use the new JSON object representation because modern Qtile
                // (including the json_ipc branch) expects it even for unframed requests.
                let data = serde_json::to_string(&c)
                    .context("Failed to serialize CommandParser to JSON object")?;

                let response = Client::send_request(data, self.framed);
                Client::match_response(response, self.framed).map(CallResult::Value)
            }
            CommandAction::Help(text) => Ok(CallResult::Text(text)),
        }
    }

    /// Convenience method to call a function on the root object.
    #[allow(dead_code)]
    pub fn call_root<S: Into<String>>(&self, function: S) -> anyhow::Result<CallResult> {
        self.call(CommandQuery::new().function(function.into()))
    }
}

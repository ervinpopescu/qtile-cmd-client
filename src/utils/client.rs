use super::{
    ipc::Client,
    parser::{CommandAction, CommandParser},
};
use serde_json::Value;
use std::{collections::HashMap, fmt};

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
    pub kwargs: Option<HashMap<String, Value>>,
    pub lifted: Option<bool>,
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

    /// Sets the keyword arguments for the function call.
    pub fn kwargs(mut self, kwargs: HashMap<String, Value>) -> Self {
        self.kwargs = Some(kwargs);
        self
    }

    /// Sets the `lifted` flag for the IPC payload.
    ///
    /// When `true` (the default), Qtile will "lift" the response out of its envelope.
    pub fn lifted(mut self, lifted: bool) -> Self {
        self.lifted = Some(lifted);
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
    #[cfg(feature = "framing")]
    pub(crate) framed: bool,
}

impl Default for QtileClient {
    fn default() -> Self {
        Self::new(false)
    }
}

impl QtileClient {
    /// Creates a new client with the specified framing protocol setting.
    pub fn new(_framed: bool) -> Self {
        Self {
            #[cfg(feature = "framing")]
            framed: _framed,
        }
    }

    /// Returns the framing protocol setting for this client.
    #[cfg(feature = "framing")]
    pub fn framed(&self) -> bool {
        self.framed
    }

    /// Executes a command or fetches help text based on the provided [`CommandQuery`].
    pub fn call(&self, query: CommandQuery) -> anyhow::Result<CallResult> {
        #[cfg(feature = "framing")]
        let framed = self.framed;
        #[cfg(not(feature = "framing"))]
        let framed = false;
        let action = CommandParser::from_params(
            query.object,
            query.function,
            query.args,
            query.kwargs,
            query.lifted,
            query.info,
            framed,
        )?;
        match action {
            CommandAction::Execute(c) => {
                let data = c.to_payload()?;
                let response = Client::send_request(data, framed);
                Client::match_response(response, framed).map(CallResult::Value)
            }
            CommandAction::Help(text) => Ok(CallResult::Text(text)),
        }
    }

    /// Convenience method to call a function on the root object.
    #[allow(dead_code)]
    pub fn call_root<S: Into<String>>(&self, function: S) -> anyhow::Result<CallResult> {
        self.call(CommandQuery::new().function(function.into()))
    }

    /// Convenience method to call a function on the root object with positional arguments.
    #[allow(dead_code)]
    pub fn call_root_with_args<S: Into<String>>(
        &self,
        function: S,
        args: Vec<String>,
    ) -> anyhow::Result<CallResult> {
        self.call(CommandQuery::new().function(function.into()).args(args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_command_query_builder() {
        let query = CommandQuery::new()
            .object(vec!["group".to_string(), "1".to_string()])
            .function("info".to_string())
            .args(vec!["short".to_string()])
            .info(true);

        assert_eq!(
            query.object,
            Some(vec!["group".to_string(), "1".to_string()])
        );
        assert_eq!(query.function, Some("info".to_string()));
        assert_eq!(query.args, Some(vec!["short".to_string()]));
        assert!(query.info);
        assert_eq!(query.kwargs, None);
        assert_eq!(query.lifted, None);
    }

    #[test]
    fn test_command_query_kwargs_and_lifted() {
        let mut kw = HashMap::new();
        kw.insert("x".to_string(), json!(100));
        kw.insert("y".to_string(), json!(200));

        let query = CommandQuery::new().kwargs(kw.clone()).lifted(false);

        assert_eq!(query.kwargs, Some(kw));
        assert_eq!(query.lifted, Some(false));
    }

    #[test]
    fn test_call_result_helpers() {
        let val_res = CallResult::Value(json!({"status": "ok"}));
        assert_eq!(val_res.as_value(), Some(&json!({"status": "ok"})));
        assert_eq!(val_res.as_str(), None);
        assert_eq!(val_res.to_json(), json!({"status": "ok"}));

        let text_res = CallResult::Text("help text".to_string());
        assert_eq!(text_res.as_value(), None);
        assert_eq!(text_res.as_str(), Some("help text"));
        assert_eq!(text_res.to_json(), Value::String("help text".to_string()));
    }

    #[test]
    fn test_call_result_display() {
        let val_res = CallResult::Value(json!({"a": 1}));
        // Pretty print adds newlines and indentation
        assert!(format!("{}", val_res).contains("\"a\": 1"));

        let text_res = CallResult::Text("plain text".to_string());
        assert_eq!(format!("{}", text_res), "plain text");
    }
}

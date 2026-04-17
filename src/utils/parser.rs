use crate::utils::{
    graph::{ObjectType, OBJECTS},
    ipc::Client,
};
use anyhow::{bail, Context};
use itertools::{EitherOrBoth::*, Itertools};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use serde_json::Value;
use std::{collections::HashMap, str::FromStr};
use strum::Display;

#[derive(PartialEq, Debug, Display)]
pub(crate) enum NumberOrString {
    Uint(u32),
    String(String),
}

impl FromStr for NumberOrString {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<u32>() {
            Ok(n) => Ok(Self::Uint(n)),
            Err(_) => Ok(Self::String(s.to_string())),
        }
    }
}

/// Represents a parsed Qtile command ready for serialization as a JSON object (new IPC).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommandParser {
    pub selectors: Vec<Vec<Value>>,
    #[serde(rename = "name")]
    pub command: String,
    pub args: Vec<String>,
    pub kwargs: HashMap<String, Value>,
    pub lifted: bool,
}

/// The result of parsing parameters, either an executable command or help text.
pub enum CommandAction {
    Execute(CommandParser),
    Help(String),
}

impl CommandParser {
    /// Serializes the command as the JSON array format expected by Qtile's IPC handler:
    /// `[selectors, name, args, kwargs, lifted]`.
    ///
    /// Qtile's `interface.py:call()` positionally unpacks the incoming request as a tuple,
    /// so the payload must be a JSON array — not a JSON object.
    pub fn to_payload(&self) -> anyhow::Result<String> {
        let arr = serde_json::json!([
            self.selectors,
            self.command,
            self.args,
            self.kwargs,
            self.lifted
        ]);
        serde_json::to_string(&arr).context("Failed to serialize command payload")
    }

    /// Creates a [`CommandAction`] from raw CLI or library parameters.
    pub fn from_params(
        object: Option<Vec<String>>,
        function: Option<String>,
        args: Option<Vec<String>>,
        info: bool,
    ) -> anyhow::Result<CommandAction> {
        let command: String;
        let mut args_to_be_sent: Vec<String> = vec![];
        let kwargs: HashMap<String, Value> = HashMap::new();
        let lifted = true;

        let selectors = if let Some(v) = object {
            Self::get_object(v)?
        } else {
            vec![]
        };

        match function {
            Some(ref s) => {
                if "help" == s.as_str() {
                    let help = Self::get_help(&selectors, None)?;
                    return Ok(CommandAction::Help(help));
                } else if info {
                    let info_cmd = s.to_owned();
                    let info_text = format!(
                        "{} {}",
                        s,
                        Self::get_formatted_info(selectors.clone(), &info_cmd, true, false,)?
                    );
                    return Ok(CommandAction::Help(info_text));
                } else {
                    command = s.to_owned();
                }
            }
            None => {
                let help = Self::get_help(&selectors, None)?;
                return Ok(CommandAction::Help(help));
            }
        }

        if let Some(args) = args {
            args_to_be_sent = args;
        };

        Ok(CommandAction::Execute(Self {
            selectors,
            command,
            args: args_to_be_sent,
            kwargs,
            lifted,
        }))
    }

    /// Fetches the list of available commands for the current selectors and returns a help string.
    pub fn get_help(
        selectors: &[Vec<Value>],
        object_names: Option<Vec<String>>,
    ) -> anyhow::Result<String> {
        let commands = CommandParser {
            selectors: selectors.to_owned(),
            command: "commands".to_string(),
            args: vec![],
            kwargs: HashMap::new(),
            lifted: true,
        };
        let data = commands.to_payload()?;
        let response = Client::send_request(data);
        let result = Client::match_response(response);

        match result {
            Ok(Value::Array(arr)) => {
                let obj_string = object_names
                    .map(|v| v.iter().join(" "))
                    .unwrap_or_else(|| "root".to_owned());
                let prefix = format!("-o {obj_string} -f ");
                Self::get_commands_help(selectors.to_owned(), prefix, arr)
            }
            Ok(_) => bail!("'commands' result should be an array"),
            Err(err) => bail!("qtile error: {err}"),
        }
    }

    /// Splits a raw Qtile docstring into `(signature, description)`.
    fn split_docstring(doc: &str) -> anyhow::Result<(String, String)> {
        let mut lines = doc.splitn(3, '\n');
        let first = lines.next().unwrap_or("");
        let desc = lines.next().unwrap_or("").trim().to_string();

        let start = first.find('(').context("missing '(' in docstring")?;
        let end = first.find(')').context("missing ')' in docstring")?;
        let sig = first[start..end + 1].to_string();
        Ok((sig, desc))
    }

    /// Formats a raw Qtile docstring into a readable help line.
    fn parse_docstring(doc: &str, include_args: bool, short: bool) -> anyhow::Result<String> {
        let (sig, desc) = Self::split_docstring(doc)?;
        let doc_args: &str = if !include_args {
            ""
        } else if short {
            if sig == "()" {
                " "
            } else {
                "*"
            }
        } else {
            &sig
        };
        Ok(format!("{doc_args} {desc}"))
    }

    /// Batches documentation requests for multiple commands into a single 'eval' call for performance.
    fn get_commands_help(
        selectors: Vec<Vec<Value>>,
        prefix: String,
        arr: Vec<Value>,
    ) -> anyhow::Result<String> {
        let commands = arr
            .iter()
            .map(|s| {
                s.as_str()
                    .map(|s| s.to_owned())
                    .context("command name is not a string")
            })
            .collect::<anyhow::Result<Vec<String>>>()?;

        // Use a unique separator to batch docstrings into a single string.
        // This avoids issues with Python's 'eval()' returning stringified lists.
        let sep = "\u{0001}";
        let eval_cmd = format!("'{sep}'.join([self.doc(cmd) for cmd in {commands:?}])");
        let eval_parser = CommandParser {
            selectors: selectors.clone(),
            command: "eval".to_string(),
            args: vec![eval_cmd],
            kwargs: HashMap::new(),
            lifted: true,
        };

        let data = eval_parser.to_payload()?;
        let result = Client::match_response(Client::send_request(data))?;

        let docs_str = result.as_str().context("eval result should be a string")?;
        let docs: Vec<&str> = docs_str.split(sep).collect();

        if docs.len() != commands.len() {
            bail!(
                "eval result length mismatch: expected {}, got {}",
                commands.len(),
                docs.len()
            );
        }

        let mut output: Vec<[String; 3]> = vec![];
        for (cmd, doc_str) in commands.iter().zip(docs.iter()) {
            let (sig, desc) = Self::split_docstring(doc_str)?;
            output.push([format!("{prefix}{cmd}"), sig, desc]);
        }

        let max_cmd = output.iter().map(|r| r[0].len()).max().unwrap_or(0);
        let indent = " ".repeat(max_cmd + 2);
        let mut help_str = String::new();
        for [cmd, sig, desc] in output {
            help_str.push_str(&format!("{cmd:<max_cmd$}  {sig}\n"));
            if !desc.is_empty() {
                help_str.push_str(&format!("{indent}{desc}\n"));
            }
        }
        Ok(help_str)
    }

    fn get_formatted_info(
        selectors: Vec<Vec<Value>>,
        cmd: &str,
        args: bool,
        short: bool,
    ) -> anyhow::Result<String> {
        let commands = CommandParser {
            selectors: selectors.clone(),
            command: "doc".to_string(),
            args: vec![cmd.to_owned()],
            kwargs: HashMap::new(),
            lifted: true,
        };
        let data = commands.to_payload()?;
        let response = Client::send_request(data);
        match Client::match_response(response) {
            Ok(res) => {
                let doc = res.as_str().context("doc result not a string")?;
                Self::parse_docstring(doc, args, short)
            }
            Err(err) => bail!("{err}"),
        }
    }

    /// Recursively parses a list of object identifiers into Qtile selectors.
    fn get_object(mut object: Vec<String>) -> anyhow::Result<Vec<Vec<Value>>> {
        if object.first() == Some(&"root".to_owned()) {
            object.remove(0);
        };
        if object.len() == 1 && !OBJECTS.iter().any(|o| *o == object[0]) {
            bail!("No such object \"{object_0}\"", object_0 = object[0]);
        }

        let mut selectors: Vec<Vec<Value>> = Vec::new();
        let mut parsed_next = false;

        if !object.is_empty() {
            for pair in object.iter().zip_longest(object[1..].iter()) {
                match pair {
                    Both(arg0, arg1) => {
                        if parsed_next {
                            parsed_next = false;
                            continue;
                        }
                        let arg0_parsed = arg0
                            .parse::<NumberOrString>()
                            .map_err(|e| anyhow::anyhow!(e))?;
                        let arg1_parsed = arg1
                            .parse::<NumberOrString>()
                            .map_err(|e| anyhow::anyhow!(e))?;

                        match arg0_parsed {
                            NumberOrString::Uint(n) => {
                                if object.iter().position(|s| *s == n.to_string()) == Some(0) {
                                    bail!("Number {n} is not an object")
                                }
                            }
                            NumberOrString::String(name) => {
                                if !OBJECTS.contains(&name.as_str()) {
                                    bail!("No such object {name}");
                                }

                                match arg1_parsed {
                                    NumberOrString::Uint(index) => {
                                        // Attempt numeric index resolution first
                                        let obj_type = ObjectType::with_number(&name, index);
                                        match obj_type {
                                            Ok(o) => {
                                                let idx = match o {
                                                    ObjectType::Layout(i)
                                                    | ObjectType::Screen(i)
                                                    | ObjectType::Window(i) => i,
                                                    _ => None,
                                                };
                                                parsed_next = true;
                                                selectors.push(vec![
                                                    Value::String(name),
                                                    idx.map(|i| Value::Number(Number::from(i)))
                                                        .unwrap_or(Value::Null),
                                                ]);
                                            }
                                            Err(_) => {
                                                // Fallback to string if number is not supported (e.g. group "1")
                                                if ObjectType::with_string(&name, index.to_string())
                                                    .is_ok()
                                                {
                                                    parsed_next = true;
                                                    selectors.push(vec![
                                                        Value::String(name),
                                                        Value::String(index.to_string()),
                                                    ]);
                                                } else {
                                                    bail!("Object {name} does not take a numeric index");
                                                }
                                            }
                                        }
                                    }
                                    NumberOrString::String(selector) => {
                                        let obj_type =
                                            ObjectType::with_string(&name, selector.clone());
                                        match obj_type {
                                            Ok(_) => {
                                                parsed_next = true;
                                                selectors.push(vec![
                                                    Value::String(name),
                                                    Value::String(selector),
                                                ]);
                                            }
                                            Err(_) => {
                                                selectors.push(vec![
                                                    Value::String(name.clone()),
                                                    Value::Null,
                                                ]);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Left(left) => {
                        if OBJECTS.contains(&left.as_str()) {
                            selectors.push(vec![Value::String(left.clone()), Value::Null]);
                        }
                    }
                    Right(_) => {}
                }
            }
        }
        Ok(selectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_number_or_string_from_str() {
        assert_eq!(
            NumberOrString::from_str("123").unwrap(),
            NumberOrString::Uint(123)
        );
        assert_eq!(
            NumberOrString::from_str("abc").unwrap(),
            NumberOrString::String("abc".to_string())
        );
    }

    #[test]
    fn test_get_object_simple() {
        let obj = vec!["group".to_string()];
        let selectors = CommandParser::get_object(obj).unwrap();
        assert_eq!(
            selectors,
            vec![vec![Value::String("group".into()), Value::Null]]
        );
    }

    #[test]
    fn test_get_object_with_selector() {
        let obj = vec!["group".to_string(), "1".to_string()];
        let selectors = CommandParser::get_object(obj).unwrap();
        assert_eq!(
            selectors,
            vec![vec![
                Value::String("group".into()),
                Value::String("1".into())
            ]]
        );
    }

    #[test]
    fn test_get_object_with_index() {
        let obj = vec!["screen".to_string(), "0".to_string()];
        let selectors = CommandParser::get_object(obj).unwrap();
        assert_eq!(
            selectors,
            vec![vec![
                Value::String("screen".into()),
                Value::Number(0.into())
            ]]
        );
    }

    #[test]
    fn test_get_object_root() {
        let obj = vec!["root".to_string(), "layout".to_string()];
        let selectors = CommandParser::get_object(obj).unwrap();
        assert_eq!(
            selectors,
            vec![vec![Value::String("layout".into()), Value::Null]]
        );
    }

    #[test]
    fn test_get_object_invalid() {
        let obj = vec!["nonexistent".to_string()];
        assert!(CommandParser::get_object(obj).is_err());
    }

    #[test]
    fn test_get_object_complex_selector() {
        // Test bar with a string name that looks like a number
        let obj = vec!["bar".to_string(), "1".to_string()];
        let selectors = CommandParser::get_object(obj).unwrap();
        assert_eq!(
            selectors,
            vec![vec![Value::String("bar".into()), Value::String("1".into())]]
        );
    }

    #[test]
    fn test_get_object_multi_level() {
        let obj = vec![
            "group".to_string(),
            "1".to_string(),
            "window".to_string(),
            "123".to_string(),
        ];
        let selectors = CommandParser::get_object(obj).unwrap();
        assert_eq!(
            selectors,
            vec![
                vec![Value::String("group".into()), Value::String("1".into())],
                vec![Value::String("window".into()), Value::Number(123.into())],
            ]
        );
    }

    #[test]
    fn test_parse_docstring() {
        let doc = "cmd(arg1, arg2)\nThis is a test command.";
        let parsed = CommandParser::parse_docstring(doc, true, false).unwrap();
        assert_eq!(parsed, "(arg1, arg2) This is a test command.");

        let parsed_no_args = CommandParser::parse_docstring(doc, false, false).unwrap();
        assert_eq!(parsed_no_args, " This is a test command.");

        let parsed_short = CommandParser::parse_docstring(doc, true, true).unwrap();
        assert_eq!(parsed_short, "* This is a test command.");
    }

    #[test]
    fn test_parse_docstring_errors() {
        assert!(CommandParser::parse_docstring("no parens", true, false).is_err());
        assert!(CommandParser::parse_docstring("missing end (", true, false).is_err());
    }

    #[test]
    fn test_to_payload_is_array() {
        // Qtile's interface.py positionally unpacks the request, so we must send an array
        let parser = CommandParser {
            selectors: vec![vec![Value::String("group".into()), Value::Null]],
            command: "info".to_string(),
            args: vec!["arg1".to_string()],
            kwargs: HashMap::new(),
            lifted: true,
        };
        let payload = parser.to_payload().unwrap();
        let parsed: Value = serde_json::from_str(&payload).unwrap();
        // Must be [selectors, name, args, kwargs, lifted]
        let arr = parsed.as_array().expect("payload must be a JSON array");
        assert_eq!(arr.len(), 5);
        assert_eq!(arr[0], serde_json::json!([["group", null]])); // selectors
        assert_eq!(arr[1], Value::String("info".into())); // name
        assert_eq!(arr[2], serde_json::json!(["arg1"])); // args
        assert_eq!(arr[3], serde_json::json!({})); // kwargs
        assert_eq!(arr[4], Value::Bool(true)); // lifted
    }

    #[test]
    fn test_get_object_errors() {
        // Number at start
        assert!(CommandParser::get_object(vec!["1".into()]).is_err());
        // Unknown object name
        assert!(CommandParser::get_object(vec!["unknown".into()]).is_err());
        // Object that doesn't take index
        assert!(CommandParser::get_object(vec!["core".into(), "1".into()]).is_err());
    }

    #[test]
    #[ignore = "requires live Qtile socket"]
    fn test_command_parser_from_params() {
        // Execute action
        let action = CommandParser::from_params(
            Some(vec!["group".into()]),
            Some("info".into()),
            None,
            false,
        )
        .unwrap();
        if let CommandAction::Execute(p) = action {
            assert_eq!(p.command, "info");
            assert_eq!(p.selectors.len(), 1);
        } else {
            panic!("Expected Execute action");
        }

        // Help action
        let action_help =
            CommandParser::from_params(None, Some("help".into()), None, false).unwrap();
        assert!(matches!(action_help, CommandAction::Help(_)));

        // Implicit help (no function)
        let action_no_func = CommandParser::from_params(None, None, None, false).unwrap();
        assert!(matches!(action_no_func, CommandAction::Help(_)));

        // Info action
        let action_info =
            CommandParser::from_params(None, Some("status".into()), None, true).unwrap();
        assert!(matches!(action_info, CommandAction::Help(_)));
    }

    #[test]
    #[ignore = "requires live Qtile socket"]
    fn test_command_parser_get_help() {
        // In the coverage env, qtile is running, so this should succeed.
        let res = CommandParser::get_help(&[], None);
        assert!(res.is_ok());
    }
}

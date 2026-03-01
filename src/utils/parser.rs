use crate::utils::{
    graph::{ObjectType, OBJECTS},
    ipc::Client,
};
use anyhow::{bail, Context};
use itertools::{EitherOrBoth::*, Itertools};
use serde::Deserialize;
use serde_json::Value;
use serde_tuple::Serialize_tuple;
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

/// Represents a parsed Qtile command ready for serialization.
///
/// Example JSON structure for a complex command:
/// ```json
/// [
///   [ ["screen", null], ["bar", "bottom"] ],
///   "eval",
///   ["[w.info() for w in self.widgets]"],
///   {},
///   true
/// ]
/// ```
#[derive(Serialize_tuple, Deserialize, Debug)]
pub struct CommandParser {
    selectors: Vec<Vec<Value>>,
    command: String,
    args: Vec<String>,
    kwargs: HashMap<String, Value>,
    lifted: bool,
}

/// The result of parsing parameters, either an executable command or help text.
pub enum CommandAction {
    Execute(CommandParser),
    Help(String),
}

impl CommandParser {
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
                        Self::get_formatted_info(selectors.clone(), &info_cmd, true, false)?
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
        let data = serde_json::to_string(&commands).context("Failed to serialize help command")?;
        let response = Client::send(data);
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

    /// Formats a raw Qtile docstring into a readable help line.
    fn parse_docstring(doc: &str, include_args: bool, short: bool) -> anyhow::Result<String> {
        let doc_lines = doc.split('\n').collect_vec();
        let tdoc = doc_lines[0].to_owned();
        let start = tdoc.find('(').context("missing '(' in docstring")?;
        let end = tdoc.find(')').context("missing ')' in docstring")?;

        let mut doc_args = &tdoc[start..end + 1];
        let short_desc: &str = if doc_lines.len() > 1 {
            doc_lines[1]
        } else {
            ""
        };

        if !include_args {
            doc_args = "";
        } else if short {
            doc_args = if doc_args == "()" { " " } else { "*" }
        }
        Ok(format!("{doc_args} {short_desc}"))
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

        let eval_cmd = format!("[self.cmd_doc(cmd) for cmd in {commands:?}]");
        let eval_parser = CommandParser {
            selectors: selectors.clone(),
            command: "eval".to_string(),
            args: vec![eval_cmd],
            kwargs: HashMap::new(),
            lifted: true,
        };

        let data = serde_json::to_string(&eval_parser).context("Failed to serialize eval")?;
        let result = Client::match_response(Client::send(data))?;

        let docs = match result {
            Value::Array(arr) => {
                let success = arr[0].as_bool().context("eval success status invalid")?;
                if !success {
                    let err = &arr[1];
                    bail!("eval failed: {err}");
                }
                arr[1]
                    .as_array()
                    .context("eval result not an array")?
                    .to_owned()
            }
            _ => bail!("eval result should be an array"),
        };

        let mut output: Vec<[String; 2]> = vec![];
        for (cmd, doc_val) in commands.iter().zip(docs.iter()) {
            let doc_str = doc_val.as_str().context("doc result invalid")?;
            let formatted = Self::parse_docstring(doc_str, true, true)?;
            output.push([format!("{prefix}{cmd}"), formatted]);
        }

        let max_cmd = output.iter().map(|[p, _]| p.len()).max().unwrap_or(0);
        let mut help_str = String::new();
        for line in output {
            help_str.push_str(&format!(
                "{:<width$}\t{}\n",
                line[0],
                line[1],
                width = max_cmd
            ));
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
        let data = serde_json::to_string(&commands).context("Failed to serialize doc")?;
        let response = Client::send(data);
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
                                        let obj_type = ObjectType::with_number(&name, index);
                                        match obj_type {
                                            Ok(o) => {
                                                let idx = match o {
                                                    ObjectType::Layout(i)
                                                    | ObjectType::Screen(i)
                                                    | ObjectType::Window(i) => i,
                                                    _ => bail!(
                                                        "Object {name} does not accept numeric index"
                                                    ),
                                                };
                                                parsed_next = true;
                                                selectors.push(vec![
                                                    Value::String(name),
                                                    idx.map(Value::from).unwrap_or(Value::Null),
                                                ]);
                                            }
                                            Err(_) => {
                                                selectors
                                                    .push(vec![Value::String(name), Value::Null]);
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
                                                selectors
                                                    .push(vec![Value::String(name), Value::Null]);
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

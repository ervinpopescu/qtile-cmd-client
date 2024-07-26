// use crate::utils::graph::OBJ_HASHMAP;
use crate::utils::{
    graph::{ObjectType, OBJECTS},
    ipc::Client,
};
use anyhow::bail;
use clap::command;
use itertools::{EitherOrBoth::*, Itertools};
use serde::Deserialize;
use serde_json::value::Number;
use serde_json::Value;
use serde_tuple::Serialize_tuple;
use std::{collections::HashMap, process::exit, str::FromStr};
use strum::Display;

use super::args::{Args, Commands};

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

// [
//   [
//      ["screen", null],
//      ["bar", "bottom"]
//   ],
//   "eval",
//   ["[w.info() for w in self.widgets]"],
//   {},
//   true
// ]
#[derive(Serialize_tuple, Deserialize, Debug)]
pub struct CommandParser {
    selectors: Vec<Vec<Value>>,
    command: String,
    args: Vec<String>,
    kwargs: HashMap<String, Value>,
    lifted: bool,
}

impl CommandParser {
    pub fn from_args(cli_args: Args) -> anyhow::Result<Self> {
        // let graph_node: CommandGraphNode = CommandGraphNode::new(
        //     Selector::String("screen".to_string()),
        //     None,
        //     NodeType::Object(ObjectType::Screen("screen".to_string(), None)),
        // );
        // println!("{:#?}", graph_node);
        match cli_args.command {
            Commands::CmdObj {
                object,
                function,
                args,
                info,
            } => Self::from_params(object, function, args, info),
        }
    }
    pub fn from_params(
        object: Option<Vec<String>>,
        function: Option<String>,
        args: Option<Vec<String>>,
        info: bool,
    ) -> anyhow::Result<Self> {
        let mut command: String = String::new();
        let mut args_to_be_sent: Vec<String> = vec![];
        let kwargs: HashMap<String, Value> = HashMap::new();
        let lifted = true;
        let selectors: Vec<Vec<Value>>;
        if let Some(ref v) = object {
            selectors = Self::get_object(v.clone())?;
        } else {
            selectors = vec![];
        }
        match function {
            Some(ref s) => {
                if "help" == s.as_str() {
                    let commands = CommandParser {
                        selectors: selectors.clone(),
                        command: "commands".to_string(),
                        args: vec![],
                        kwargs: kwargs.clone(),
                        lifted,
                    };
                    let data = serde_json::to_string(&commands).unwrap();
                    let response = Client::send(data);
                    let result = Client::match_response(response);
                    match result {
                        Ok(result) => match result {
                            Value::Array(arr) => {
                                if let Some(v) = object {
                                    let obj_string = v.iter().join(" ").to_string();
                                    let prefix = "-o ".to_owned() + &obj_string + " -f ";
                                    let printed_commands =
                                        Self::print_commands(selectors.clone(), prefix, arr);
                                    match printed_commands {
                                        Ok(_) => {
                                            exit(0);
                                        }
                                        Err(err) => bail!("{err}"),
                                    }
                                }
                            }
                            Value::Null
                            | Value::Bool(_)
                            | Value::Number(_)
                            | Value::String(_)
                            | Value::Object(_) => bail!("{commands:?} result should be an array"),
                        },
                        Err(err) => bail!("{err}"),
                    }
                } else {
                    command.clone_from(s);
                };
            }
            None => match info {
                true => bail!("function is never None"),
                false => {
                    command = function.unwrap();
                }
            },
        }
        if let Some(args) = args {
            args_to_be_sent = args;
        };
        Ok(Self {
            selectors,
            command,
            args: args_to_be_sent,
            kwargs,
            lifted,
        })
    }
    fn print_commands(
        selectors: Vec<Vec<Value>>,
        prefix: String,
        arr: Vec<Value>,
    ) -> anyhow::Result<()> {
        let commands = arr
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect_vec();
        let mut output: Vec<[String; 2]> = vec![];
        for cmd in commands {
            let doc_args = Self::get_formatted_info(selectors.clone(), &cmd, true, true);
            match doc_args {
                Ok(doc_args) => {
                    let pcmd = prefix.clone() + cmd.as_str();
                    output.push([pcmd, doc_args]);
                }
                Err(err) => bail!("{err}"),
            }
        }
        let max_cmd = output
            .iter()
            .map(|[pcmd, doc_args]| pcmd.len())
            .max()
            .unwrap();
        for line in output {
            println!("{:<width$}\t{}", line[0], line[1], width = max_cmd);
        }
        Ok(())
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
        let data = serde_json::to_string(&commands).unwrap();
        let response = Client::send(data);
        let result = Client::match_response(response);
        match result {
            Ok(result) => {
                let doc = result.as_str().unwrap().to_owned();
                let doc = doc.split('\n').collect_vec();
                let tdoc = doc[0].to_owned();
                let mut doc_args = &tdoc[tdoc.find('(').unwrap()..tdoc.find(')').unwrap() + 1];
                let short_desc: &str = if doc.len() > 1 { doc[1] } else { "" };
                if !args {
                    doc_args = "";
                } else if short {
                    doc_args = if doc_args == "()" { " " } else { "*" }
                }
                Ok(doc_args.to_owned() + " " + short_desc)
            }
            Err(err) => bail!("{err}"),
        }
    }
    pub fn get_object(mut object: Vec<String>) -> anyhow::Result<Vec<Vec<Value>>> {
        if object.first() == Some(&"root".to_string()) {
            object.remove(0);
        };
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
                        let arg0 = arg0.parse::<NumberOrString>().unwrap();
                        let arg1 = arg1.parse::<NumberOrString>().unwrap();
                        // println!(
                        //     "arg0={:?}\narg1={:?}\nparsed_next={}",
                        //     arg0, arg1, parsed_next
                        // );
                        match arg0 {
                            NumberOrString::Uint(arg0) => {
                                if object.iter().position(|s| *s == arg0.to_string()) == Some(0) {
                                    bail!("Number {} is not an object", arg0)
                                }
                            }
                            NumberOrString::String(arg0) => {
                                let obj_string = OBJECTS.iter().find(|&o| *o == arg0);
                                // let hashmap = OBJ_HASHMAP.lock().unwrap();
                                let mut obj_vec: Vec<Value> = vec![];
                                match obj_string {
                                    Some(_i) => {
                                        // println!("Found arg0: {} in OBJECTS", i);
                                        match arg1 {
                                            NumberOrString::Uint(arg1) => {
                                                // println!("Number from arg1: {:?}", arg1);
                                                let mut obj_type =
                                                    ObjectType::with_number(&arg0, arg1);
                                                let index: Option<u32>;
                                                match obj_type {
                                                    Ok(ref o) => {
                                                        match o {
                                                            ObjectType::Layout(layout_index) => {
                                                                index = *layout_index;
                                                                parsed_next = true;
                                                            }
                                                            ObjectType::Screen(screen_index) => {
                                                                index = *screen_index;
                                                                parsed_next = true;
                                                            }
                                                            ObjectType::Window(wid) => {
                                                                index = *wid;
                                                                parsed_next = true;
                                                            }
                                                            _ => bail!(
                                                            "Not possible to have object {:?} here",
                                                            obj_type
                                                        ),
                                                        };
                                                        match index {
                                                            Some(index) => {
                                                                obj_vec = vec![
                                                                    Value::String(arg0),
                                                                    Value::Number(
                                                                        Number::from_f64(
                                                                            index as f64,
                                                                        )
                                                                        .unwrap(),
                                                                    ),
                                                                ];
                                                            }
                                                            None => {
                                                                obj_vec = vec![
                                                                    Value::String(arg0),
                                                                    Value::Null,
                                                                ];
                                                            }
                                                        };
                                                    }
                                                    Err(_err) => {
                                                        // println!(
                                                        //     "Error: {}\n  arg0={}\n  arg1={}",
                                                        //     err, arg0, arg1
                                                        // );
                                                        obj_type = ObjectType::with_none(&arg0);
                                                        match obj_type {
                                                            Ok(ref _o) => {
                                                                // println!(
                                                                //     "Using:\n  arg0={:?}, arg1=None",
                                                                //     obj_type
                                                                // );
                                                            }
                                                            Err(err) => {
                                                                eprintln!("Failed to parse {}", err)
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            NumberOrString::String(arg1) => {
                                                // println!("String from arg1: {:?}", arg1);
                                                let mut obj_type =
                                                    ObjectType::with_string(&arg0, arg1.clone());
                                                let mut item_selector: Option<String> = None;
                                                match obj_type {
                                                    Ok(ref o) => {
                                                        match o {
                                                            ObjectType::Bar(pos) => match pos {
                                                                Some(pos) => {
                                                                    match pos.as_str() {
                                                                    "top" => {
                                                                        // println!("Found {:?} with {} identifier", arg0, pos);
                                                                        item_selector = Some(pos.to_string());
                                                                        parsed_next = true;
                                                                    }
                                                                    "right" => {
                                                                        // println!("Found {:?} with {} identifier", arg0, pos);
                                                                        item_selector = Some(pos.to_string());
                                                                        parsed_next = true;
                                                                    }
                                                                    "bottom"=> {
                                                                        // println!("Found {:?} with {} identifier", arg0, pos);
                                                                        item_selector = Some(pos.to_string());
                                                                        parsed_next = true;
                                                                    }
                                                                    "left" => {
                                                                        // println!("Found {:?} with {} identifier", arg0, pos);
                                                                        item_selector = Some(pos.to_string());
                                                                        parsed_next = true;
                                                                    }
                                                                    _ => bail!("bar needs to have position: top|right|bottom|left")
                                                                }
                                                                }
                                                                None => {
                                                                    item_selector = None;
                                                                }
                                                            },
                                                            ObjectType::Group(name) => {
                                                                match name {
                                                                    Some(ref name) => {
                                                                        // println!("Found {:?} with {} identifier", arg0, name);
                                                                        item_selector =
                                                                            Some(name.clone());
                                                                        parsed_next = true;
                                                                    }
                                                                    None => {
                                                                        item_selector = None;
                                                                    }
                                                                }
                                                            }
                                                            ObjectType::Widget(name) => {
                                                                match name {
                                                                    Some(name) => {
                                                                        // println!("Found {:?} with {} identifier", arg0, name);
                                                                        item_selector =
                                                                            Some(name.clone());
                                                                        parsed_next = true;
                                                                    }
                                                                    None => {
                                                                        item_selector = None;
                                                                    }
                                                                }
                                                            }
                                                            ObjectType::Core => {
                                                                println!(
                                                                "Found core {:?} with arg0: {:?}",
                                                                o, arg0
                                                            );
                                                                item_selector = None;
                                                            }
                                                            _ => bail!(
                                                            "Not possible to have object {:?} here",
                                                            obj_type
                                                        ),
                                                        }
                                                    }
                                                    Err(_err) => {
                                                        // println!(
                                                        //     "Error: {}\n  arg0={}\n  arg1={}",
                                                        //     err, arg0, arg1
                                                        // );
                                                        obj_type = ObjectType::with_none(&arg0);
                                                        match obj_type {
                                                            Ok(ref _o) => {
                                                                // println!(
                                                                //     "Using:\n  arg0={:?}, arg1=None",
                                                                //     obj_type
                                                                // );
                                                            }
                                                            Err(err) => {
                                                                eprintln!("Failed to parse {}", err)
                                                            }
                                                        }
                                                    }
                                                }
                                                match item_selector {
                                                    Some(item_selector) => {
                                                        obj_vec = vec![
                                                            Value::String(arg0),
                                                            Value::String(item_selector),
                                                        ];
                                                    }
                                                    None => {
                                                        obj_vec =
                                                            vec![Value::String(arg0), Value::Null];
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    None => {
                                        bail!("No such object {}", arg0);
                                    }
                                };
                                selectors.push(obj_vec);
                            }
                        }
                    }
                    Left(_left) => {
                        if _left.as_str() == "bar" {
                            bail!("bar needs to have position: top|right|bottom|left")
                        }
                    }
                    Right(_right) => {
                        println!("right: {_right:?}")
                    }
                };
                // println!();
            }
        }
        Ok(selectors)
    }
}

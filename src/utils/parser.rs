// use crate::utils::graph::OBJ_HASHMAP;
use crate::utils::graph::{ObjectType, OBJECTS};
use anyhow::bail;
use clap::command;
use itertools::{EitherOrBoth::*, Itertools};
use serde::Deserialize;
use serde_json::value::Number;
use serde_json::Value;
use serde_tuple::Serialize_tuple;
use std::{collections::HashMap, str::FromStr};
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
        if let Some(v) = object {
            selectors = Self::get_object(v)?;
        } else {
            selectors = vec![];
        }
        match function {
            Some(s) => {
                if let "help" = s.as_str() {
                    let navigate = CommandParser::from_params(
                        Some(vec![]),
                        Some("commands".to_string()),
                        Some(vec![]),
                        false,
                    );
                } else {
                    command = s;
                }
            }
            None => match info {
                true => todo!(),
                false => {
                    command = function.unwrap();
                }
            },
        }
        if let Some(v) = args {
            args_to_be_sent = v;
        };
        Ok(Self {
            selectors,
            command,
            args: args_to_be_sent,
            kwargs,
            lifted,
        })
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
                        // println!("left: {left:?}")
                    }
                    Right(_right) => {
                        // println!("right: {right:?}")
                    }
                };
                // println!();
            }
        }
        Ok(selectors)
    }
}

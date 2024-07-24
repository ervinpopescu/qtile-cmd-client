use std::{collections::HashMap, sync::Mutex};

use super::parser::NumberOrString;
use anyhow::Error;
use itertools::Itertools;
use once_cell::sync::Lazy;

pub(crate) static OBJECTS: &[&str] = &[
    "core", "screen", "bar", "widget", "group", "layout", "window", "root",
];

#[derive(Debug)]
pub(crate) enum Selector {
    String(String),
    Int(u32),
    Null,
}

impl Clone for Selector {
    fn clone(&self) -> Self {
        match self {
            Selector::String(s) => Selector::String(s.clone()),
            Selector::Int(i) => Selector::Int(*i),
            Selector::Null => Selector::Null,
        }
    }
}

impl Default for Selector {
    fn default() -> Self {
        Self::Null
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ObjectType {
    Screen(Option<u32>),
    Group(Option<String>),
    Layout(Option<u32>),
    Window(Option<u32>),
    Bar(Option<String>),
    Widget(Option<String>),
    Core,
    Root,
}

impl ObjectType {
    pub(crate) fn with_string(string: &str, s: String) -> anyhow::Result<ObjectType> {
        match string {
            "bar" => Ok(Self::Bar(Some(s))),
            "group" => Ok(Self::Group(Some(s))),
            "widget" => Ok(Self::Widget(Some(s))),
            _ => Err(Error::msg("Failed to parse ".to_string() + string)),
        }
    }
    pub(crate) fn with_number(string: &str, n: u32) -> anyhow::Result<ObjectType> {
        match string {
            "screen" => Ok(Self::Screen(Some(n))),
            "layout" => Ok(Self::Layout(Some(n))),
            "window" => Ok(Self::Window(Some(n))),
            _ => Err(Error::msg("Failed to parse ".to_string() + string)),
        }
    }
    pub(crate) fn with_none(string: &str) -> anyhow::Result<ObjectType> {
        match string {
            "screen" => Ok(Self::Screen(None)),
            "group" => Ok(Self::Group(None)),
            "layout" => Ok(Self::Layout(None)),
            "window" => Ok(Self::Window(None)),
            "bar" => Ok(Self::Bar(None)),
            "widget" => Ok(Self::Widget(None)),
            "core" => Ok(Self::Core),
            "root" => Ok(Self::Root),
            _ => Err(Error::msg("Failed to parse ".to_string() + string)),
        }
    }
}

pub(crate) static OBJ_HASHMAP: Lazy<Mutex<HashMap<&str, ObjectType>>> = Lazy::new(|| {
    let mut hashmap = HashMap::new();
    hashmap.insert("root", ObjectType::Root);
    hashmap.insert("core", ObjectType::Core);
    hashmap.insert("screen", ObjectType::Screen(None));
    hashmap.insert("group", ObjectType::Group(None));
    hashmap.insert("layout", ObjectType::Layout(None));
    hashmap.insert("window", ObjectType::Window(None));
    hashmap.insert("bar", ObjectType::Bar(None));
    hashmap.insert("widget", ObjectType::Widget(None));
    hashmap.into()
});

type SelectorType = (ObjectType, Selector);

#[derive(Clone, Debug)]
pub(crate) struct CommandGraphNode {
    selector: Option<Selector>,
    selectors: Vec<SelectorType>,
    parent: Option<Box<Self>>,
    children: Option<Vec<String>>,
    typ: ObjectType,
}

impl Default for CommandGraphNode {
    fn default() -> Self {
        Self {
            selector: None,
            selectors: vec![],
            parent: None,
            children: [
                "bar", "group", "layout", "screen", "widget", "window", "core",
            ]
            .iter()
            .map(|&s| s.to_string())
            .collect::<Vec<String>>()
            .into(),
            typ: ObjectType::Root {},
        }
    }
}
impl CommandGraphNode {
    pub fn new(selector: Selector, parent: Option<Box<Self>>, typ: ObjectType) -> Self {
        fn new_with_children(
            selector: Selector,
            parent: Option<Box<CommandGraphNode>>,
            typ: ObjectType,
            children: &[&str],
        ) -> CommandGraphNode {
            let mut selectors = parent.clone().unwrap_or_default().clone().selectors;
            selectors.extend(vec![(typ.clone(), selector.clone())]);
            let children = children
                .iter()
                .map(|&s| s.to_string())
                .collect::<Vec<String>>()
                .into();
            CommandGraphNode {
                selector: Some(selector),
                selectors,
                parent,
                children,
                typ,
            }
        }
        match typ {
            ObjectType::Root => Self::default(),
            ObjectType::Bar(_) => {
                let children = ["screen", "widget"];
                new_with_children(selector, parent, typ, &children)
            }
            ObjectType::Group(_) => {
                let children = ["layout", "window", "screen"];
                new_with_children(selector, parent, typ, &children)
            }
            ObjectType::Layout(_) => {
                let children = ["screen", "group", "window"];
                new_with_children(selector, parent, typ, &children)
            }
            ObjectType::Screen(_) => {
                let children = ["layout", "window", "bar", "widget", "group"];
                new_with_children(selector, parent, typ, &children)
            }
            ObjectType::Widget(_) => {
                let children = ["bar", "screen"];
                new_with_children(selector, parent, typ, &children)
            }
            ObjectType::Window(_) => {
                let children = ["layout", "group", "screen"];
                new_with_children(selector, parent, typ, &children)
            }

            ObjectType::Core => {
                let children = [];
                new_with_children(selector, parent, typ, &children)
            }
        }
    }
}

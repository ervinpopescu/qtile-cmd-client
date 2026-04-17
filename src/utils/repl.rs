use crate::utils::client::{CallResult, CommandQuery, QtileClient};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, Helper};
use serde_json::Value;
use std::borrow::Cow;

struct QtileHelper {
    client: QtileClient,
    current_object: Vec<String>,
}

impl QtileHelper {
    /// Attempts to find the "active" object path based on the current line's input.
    fn get_active_path(&self, parts: &[&str]) -> Vec<String> {
        let mut active_path = self.current_object.clone();

        // Skip the first part if it's a built-in like 'cd' or 'ls'
        let start_idx = if !parts.is_empty() && (parts[0] == "cd" || parts[0] == "ls") {
            1
        } else {
            0
        };

        for &part in &parts[start_idx..] {
            if part == ".." {
                if active_path.len() > 1 {
                    active_path.pop();
                }
            } else if part == "/" || part == "root" {
                active_path = vec!["root".to_string()];
            } else {
                active_path.push(part.to_string());
            }
        }
        active_path
    }
}

impl Completer for QtileHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, word) = line[..pos]
            .rsplit_once(char::is_whitespace)
            .map(|(pre, word)| (pre.len() + 1, word))
            .unwrap_or((0, &line[..pos]));

        const ITEM_CLASSES: &[(&str, &str)] = &[
            ("bar", "widget bar on a screen"),
            ("group", "workspace group"),
            ("layout", "window layout"),
            ("screen", "monitor screen"),
            ("widget", "widget inside a bar"),
            ("window", "managed window"),
        ];

        let mut candidates = Vec::new();
        let item_class_names: Vec<&str> = ITEM_CLASSES.iter().map(|(n, _)| *n).collect();

        let all_parts: Vec<&str> = line[..pos].split_whitespace().collect();
        let is_cd = !all_parts.is_empty() && all_parts[0] == "cd";

        let navigation_parts = if line.ends_with(' ') {
            &all_parts[..]
        } else if all_parts.is_empty() {
            &[]
        } else {
            &all_parts[..all_parts.len() - 1]
        };

        let active_path = self.get_active_path(navigation_parts);

        // 1. Commands with inline short doc (one batched eval call)
        if !is_cd {
            let query = CommandQuery::new()
                .object(active_path.clone())
                .function("commands".to_string());

            if let Ok(CallResult::Value(Value::Array(cmds))) = self.client.call(query) {
                let matching: Vec<String> = cmds
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| s.starts_with(word))
                    .map(|s| s.to_string())
                    .collect();

                // Batch-fetch short docs for all matching commands in one eval call
                let docs: Vec<String> = if !matching.is_empty() {
                    let sep = "\u{0001}";
                    let eval_cmd = format!("'{sep}'.join([self.doc(cmd) for cmd in {matching:?}])");
                    let eval_query = CommandQuery::new()
                        .object(active_path.clone())
                        .function("eval".to_string())
                        .args(vec![eval_cmd]);

                    if let Ok(CallResult::Value(Value::String(s))) = self.client.call(eval_query) {
                        s.split(sep)
                            .map(|doc| {
                                // Extract first line up to ')', trim leading whitespace
                                let line = doc.lines().next().unwrap_or("").trim();
                                let start = line.find('(').unwrap_or(0);
                                let end = line.find(')').map(|i| i + 1).unwrap_or(line.len());
                                line[start..end].trim().to_string()
                            })
                            .collect()
                    } else {
                        vec![String::new(); matching.len()]
                    }
                } else {
                    vec![]
                };

                for (name, doc) in matching.iter().zip(docs.iter()) {
                    let display = if doc.is_empty() {
                        name.clone()
                    } else {
                        format!("{name:<20} {doc}")
                    };
                    candidates.push(Pair {
                        display,
                        replacement: name.clone(),
                    });
                }
            }
        }

        // 2. Navigation candidates — same logic as handle_ls:
        //    active_path ends with an item_class → offer instances (no description)
        //    otherwise → offer item_classes with static descriptions
        let active_last = active_path.last().map(|s| s.as_str()).unwrap_or("root");
        if item_class_names.contains(&active_last) {
            let mut parent_path = active_path.clone();
            parent_path.pop();
            let query = CommandQuery::new()
                .object(parent_path)
                .function("items".to_string())
                .args(vec![active_last.to_string()]);

            if let Ok(CallResult::Value(Value::Array(res))) = self.client.call(query) {
                if res.len() >= 2 {
                    if let Value::Array(instances) = &res[1] {
                        for inst in instances {
                            let inst_str = match inst {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                _ => continue,
                            };
                            if inst_str.starts_with(word) {
                                candidates.push(Pair {
                                    display: inst_str.clone(),
                                    replacement: inst_str,
                                });
                            }
                        }
                    }
                }
            }
        } else {
            for (name, doc) in ITEM_CLASSES {
                if name.starts_with(word) {
                    candidates.push(Pair {
                        display: format!("{name:<20} {doc}"),
                        replacement: name.to_string(),
                    });
                }
            }
        }

        candidates.sort_by(|a, b| a.replacement.cmp(&b.replacement));
        candidates.dedup_by(|a, b| a.replacement == b.replacement);

        Ok((start, candidates))
    }
}

impl Hinter for QtileHelper {
    type Hint = String;
}

impl Highlighter for QtileHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        Cow::Borrowed(prompt)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Borrowed(hint)
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }

    fn highlight_char(
        &self,
        _line: &str,
        _pos: usize,
        _forced: rustyline::highlight::CmdKind,
    ) -> bool {
        true
    }
}

impl Validator for QtileHelper {}

impl Helper for QtileHelper {}

/// Interactive shell for navigating the Qtile command graph and invoking functions.
pub struct Repl {
    pub(crate) client: QtileClient,
    pub(crate) current_object: Vec<String>,
}

impl Default for Repl {
    fn default() -> Self {
        Self::new()
    }
}

impl Repl {
    pub fn new() -> Self {
        Self {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
        }
    }

    /// Starts the interactive REPL loop.
    pub fn run(&mut self) -> anyhow::Result<()> {
        let config = Config::builder()
            .completion_type(CompletionType::List)
            .completion_prompt_limit(40)
            .build();
        let mut rl: Editor<QtileHelper, DefaultHistory> = Editor::with_config(config)?;

        println!("Qtile REPL - type 'exit' or 'quit' to leave, 'help' for current object help.");
        println!("Use 'cd <object>' to move through the command graph.");

        loop {
            // Update helper with current state for completions
            let helper = QtileHelper {
                client: QtileClient::new(),
                current_object: self.current_object.clone(),
            };
            rl.set_helper(Some(helper));

            let prompt = format!("({}) > ", self.current_object.join("."));
            let readline = rl.readline(&prompt);
            match readline {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed == "exit" || trimmed == "quit" {
                        break;
                    }
                    rl.add_history_entry(trimmed)?;
                    if self.handle_line(trimmed) {
                        break;
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    break;
                }
                Err(err) => {
                    println!("Error: {err:?}");
                    break;
                }
            }
        }
        Ok(())
    }

    /// Processes a single line of input. Returns true if the REPL should exit.
    pub(crate) fn handle_line(&mut self, line: &str) -> bool {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return false;
        }
        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "exit" | "quit" => return true,
            "ls" => {
                if let Err(e) = self.handle_ls(args) {
                    println!("Error: {e}");
                }
            }
            "cd" => {
                self.handle_cd(args);
            }
            ".." => {
                if self.current_object.len() > 1 {
                    self.current_object.pop();
                }
            }
            _ => {
                self.handle_call(cmd, args);
            }
        }
        false
    }

    pub(crate) fn handle_cd(&mut self, args: &[&str]) {
        if args.is_empty() {
            // Like real cd, no-op or go to root? Let's stay here but print info?
            // Actually, showing classes here is useful.
            let _ = self.handle_ls(&[]);
            return;
        }

        let target = args.join(" ");
        if target == ".." {
            if self.current_object.len() > 1 {
                self.current_object.pop();
            }
        } else if target == "/" || target == "root" {
            self.current_object = vec!["root".to_string()];
        } else {
            let mut next_obj = self.current_object.clone();
            for part in args {
                if *part == ".." {
                    if next_obj.len() > 1 {
                        next_obj.pop();
                    }
                } else {
                    next_obj.push(part.to_string());
                }
            }

            let query = CommandQuery::new()
                .object(next_obj.clone())
                .function("commands".to_string());

            let res = self.client.call(query);

            if res.is_ok() {
                self.current_object = next_obj;
            } else {
                println!("Error: Object '{target}' not found or has no commands.");
            }
        }
    }

    pub(crate) fn handle_call(&self, function: &str, args: &[&str]) {
        let mut query = CommandQuery::new()
            .object(self.current_object.clone())
            .function(function.to_string());
        if !args.is_empty() {
            query = query.args(args.iter().map(|s| s.to_string()).collect());
        }

        match self.client.call(query) {
            Ok(result) => println!("{result}"),
            Err(e) => println!("Error: {e}"),
        }
    }

    /// Returns the items to display for a given target path.
    /// - Path ends with an item_class → instances of that class (from Qtile IPC)
    /// - Otherwise → the fixed set of navigable node types
    pub(crate) fn ls_items(&self, target_path: &[String]) -> Vec<String> {
        const ITEM_CLASSES: &[&str] = &["layout", "group", "screen", "window", "bar", "widget"];

        let last = target_path.last().map(|s| s.as_str()).unwrap_or("root");
        let mut items = if ITEM_CLASSES.contains(&last) {
            let mut parent = target_path.to_vec();
            parent.pop();
            let query = CommandQuery::new()
                .object(parent)
                .function("items".to_string())
                .args(vec![last.to_string()]);

            let mut instances = Vec::new();
            if let Ok(CallResult::Value(Value::Array(res))) = self.client.call(query) {
                if res.len() >= 2 {
                    if let Value::Array(arr) = &res[1] {
                        for inst in arr {
                            let s = match inst {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                _ => continue,
                            };
                            instances.push(s);
                        }
                    }
                }
            }
            instances
        } else {
            ITEM_CLASSES.iter().map(|s| s.to_string()).collect()
        };

        items.sort();
        items.dedup();
        items
    }

    pub(crate) fn handle_ls(&self, args: &[&str]) -> anyhow::Result<()> {
        let mut target_path = self.current_object.clone();
        for &arg in args {
            if arg == ".." {
                if target_path.len() > 1 {
                    target_path.pop();
                }
            } else if arg == "/" || arg == "root" {
                target_path = vec!["root".to_string()];
            } else {
                target_path.push(arg.to_string());
            }
        }

        let items = self.ls_items(&target_path);

        if items.is_empty() {
            println!("(none)");
        } else {
            let max_width = items.iter().map(|i| i.len()).max().unwrap_or(0) + 2;
            let terminal_width = 80;
            let cols = (terminal_width / max_width).max(1);

            for (idx, item) in items.iter().enumerate() {
                print!("{item:<max_width$}");
                if (idx + 1) % cols == 0 {
                    println!();
                }
            }
            if !items.len().is_multiple_of(cols) {
                println!();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_active_path() {
        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
        };

        assert_eq!(
            helper.get_active_path(&["cd", "group"]),
            vec!["root", "group"]
        );
        assert_eq!(
            helper.get_active_path(&["cd", "group", "1"]),
            vec!["root", "group", "1"]
        );
        assert_eq!(
            helper.get_active_path(&["ls", "layout"]),
            vec!["root", "layout"]
        );
        assert_eq!(helper.get_active_path(&["group"]), vec!["root", "group"]);
    }

    #[test]
    fn test_get_active_path_navigation() {
        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string(), "group".to_string(), "1".to_string()],
        };

        assert_eq!(helper.get_active_path(&[".."]), vec!["root", "group"]);
        assert_eq!(helper.get_active_path(&["cd", ".."]), vec!["root", "group"]);
        assert_eq!(
            helper.get_active_path(&["cd", "layout"]),
            vec!["root", "group", "1", "layout"]
        );
        assert_eq!(helper.get_active_path(&["cd", "/"]), vec!["root"]);
    }

    #[test]
    fn test_repl_init() {
        let repl = Repl::new();
        assert_eq!(repl.current_object, vec!["root"]);
        let _default_repl = Repl::default();
    }

    #[test]
    fn test_handle_line() {
        let mut repl = Repl::new();
        assert!(repl.handle_line("exit"));
        assert!(repl.handle_line("quit"));
        assert!(!repl.handle_line("ls"));
        assert!(!repl.handle_line("ls group"));
        assert!(!repl.handle_line("cd group"));
        assert!(!repl.handle_line("cd /"));
        assert!(!repl.handle_line("cd root"));
        assert!(!repl.handle_line("cd .."));
        assert!(!repl.handle_line(".."));
        assert!(!repl.handle_line("invalid_command"));
    }

    #[test]
    fn test_complete() {
        use rustyline::history::DefaultHistory;
        use rustyline::Context;
        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
        };
        let history = DefaultHistory::new();
        let context = Context::new(&history);

        // Standard completion at root
        let (pos, candidates) = helper.complete("sta", 3, &context).unwrap();
        assert_eq!(pos, 0);
        // Fallback if qtile is not running, but in coverage env it is
        if !candidates.is_empty() {
            assert!(candidates.iter().any(|c| c.replacement == "status"));
        }

        // cd completion
        let (pos, candidates) = helper.complete("cd gro", 6, &context).unwrap();
        assert_eq!(pos, 3);
        assert!(candidates.iter().any(|c| c.replacement == "group"));

        // Completion inside a class
        let (pos, _candidates) = helper.complete("cd group ", 9, &context).unwrap();
        assert_eq!(pos, 9);
    }

    #[test]
    fn test_ls_items_at_root_returns_all_classes() {
        let repl = Repl::new();
        let items = repl.ls_items(&["root".to_string()]);
        let expected = {
            let mut v = vec!["bar", "group", "layout", "screen", "widget", "window"];
            v.sort();
            v
        };
        assert_eq!(items, expected);
    }

    #[test]
    fn test_ls_items_at_instance_returns_classes() {
        // Path ending with a specific instance (not an item_class) → same as root
        let repl = Repl::new();
        let path: Vec<String> = ["root", "group", "www"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let items = repl.ls_items(&path);
        assert!(items.contains(&"bar".to_string()));
        assert!(items.contains(&"group".to_string()));
        assert!(!items.is_empty());
    }

    #[test]
    fn test_ls_items_sorted_and_deduped() {
        let repl = Repl::new();
        let items = repl.ls_items(&["root".to_string()]);
        let mut sorted = items.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(items, sorted);
    }

    #[test]
    #[ignore = "requires live Qtile socket"]
    fn test_ls_items_item_class_returns_instances() {
        let repl = Repl::new();
        // screen must have at least one instance (screen 0) in any running qtile
        let items = repl.ls_items(&["root".to_string(), "screen".to_string()]);
        assert!(
            !items.is_empty(),
            "screen should have at least one instance"
        );
        // All returned items should be valid screen indices (numbers) or names
        for item in &items {
            assert!(!item.is_empty());
        }
    }

    #[test]
    #[ignore = "requires live Qtile socket"]
    fn test_ls_items_group_returns_group_names() {
        let repl = Repl::new();
        let items = repl.ls_items(&["root".to_string(), "group".to_string()]);
        assert!(!items.is_empty(), "should have at least one group");
    }

    #[test]
    fn test_complete_cd_at_root_shows_classes_not_commands() {
        use rustyline::history::DefaultHistory;
        use rustyline::Context;
        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
        };
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // `cd ` → only item_classes offered (no commands like "status", "eval", etc.)
        let (_, candidates) = helper.complete("cd ", 3, &ctx).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.replacement.as_str()).collect();
        for class in &["bar", "group", "layout", "screen", "window", "widget"] {
            assert!(names.contains(class), "expected {class} in cd candidates");
        }
        // Commands should NOT appear in cd completion
        assert!(
            !names.contains(&"status"),
            "commands must not appear in cd completion"
        );
        assert!(
            !names.contains(&"eval"),
            "commands must not appear in cd completion"
        );
    }

    #[test]
    fn test_complete_no_cd_shows_commands() {
        use rustyline::history::DefaultHistory;
        use rustyline::Context;
        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
        };
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // Plain `sta` → commands like "status" (if qtile running) plus item_classes
        let (_, candidates) = helper.complete("sta", 3, &ctx).unwrap();
        // item_classes starting with "sta" — none, so if qtile is up "status" must be there
        if !candidates.is_empty() {
            assert!(
                candidates.iter().any(|c| c.replacement == "status"),
                "expected 'status' in plain command completion"
            );
        }
    }

    #[test]
    fn test_complete_prefix_filtering() {
        use rustyline::history::DefaultHistory;
        use rustyline::Context;
        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
        };
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // `cd gr` → only "group" from item_classes matches
        let (_, candidates) = helper.complete("cd gr", 5, &ctx).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.replacement.as_str()).collect();
        assert!(names.contains(&"group"));
        assert!(!names.contains(&"screen"));
        assert!(!names.contains(&"layout"));
    }

    #[test]
    fn test_complete_dedup() {
        use rustyline::history::DefaultHistory;
        use rustyline::Context;
        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
        };
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        // No duplicate candidates
        let (_, candidates) = helper.complete("cd ", 3, &ctx).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.replacement.as_str()).collect();
        let mut sorted = names.clone();
        sorted.dedup();
        assert_eq!(names, sorted, "candidates must not contain duplicates");
    }

    #[test]
    fn test_complete_sorted() {
        use rustyline::history::DefaultHistory;
        use rustyline::Context;
        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
        };
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (_, candidates) = helper.complete("cd ", 3, &ctx).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.replacement.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "candidates must be sorted");
    }

    #[test]
    fn test_handle_cd_empty_args() {
        // cd with no args should not panic and path should stay the same
        let mut repl = Repl::new();
        repl.handle_cd(&[]);
        // path unchanged (handle_cd calls handle_ls which prints but doesn't change path)
        assert_eq!(repl.current_object, vec!["root"]);
    }

    #[test]
    fn test_handle_line_dotdot() {
        let mut repl = Repl::new();
        repl.current_object = vec!["root".into(), "group".into()];
        assert!(!repl.handle_line(".."));
        assert_eq!(repl.current_object, vec!["root"]);
        // .. at root stays at root
        assert!(!repl.handle_line(".."));
        assert_eq!(repl.current_object, vec!["root"]);
    }

    #[test]
    fn test_handle_cd_complex() {
        let mut repl = Repl::new();
        repl.handle_cd(&["group"]);
        // cd /
        repl.handle_cd(&["/"]);
        assert_eq!(repl.current_object, vec!["root"]);
        // cd root
        repl.handle_cd(&["root"]);
        assert_eq!(repl.current_object, vec!["root"]);
        // cd .. at root
        repl.handle_cd(&[".."]);
        assert_eq!(repl.current_object, vec!["root"]);
        // cd with multiple parts
        repl.handle_cd(&["group", "1"]);
    }

    #[test]
    fn test_handle_ls_complex() {
        let repl = Repl::new();
        // List root (default)
        assert!(repl.handle_ls(&[]).is_ok());
        // List specific class
        assert!(repl.handle_ls(&["group"]).is_ok());
        // List parent
        assert!(repl.handle_ls(&[".."]).is_ok());
        // List root explicitly
        assert!(repl.handle_ls(&["/"]).is_ok());
        // List specific group (may or may not exist, but exercises logic)
        let _ = repl.handle_ls(&["group", "1"]);
    }

    #[test]
    fn test_handle_line_navigation() {
        let mut repl = Repl::new();
        // cd built-in
        repl.handle_line("cd group");
        // .. built-in
        repl.handle_line("..");
        assert_eq!(repl.current_object, vec!["root"]);
        // ls built-in
        repl.handle_line("ls");
        // exit built-in
        assert!(repl.handle_line("exit"));
    }

    #[test]
    fn test_handle_call() {
        let repl = Repl::new();
        // This just prints, but we verify it doesn't panic
        repl.handle_call("status", &[]);
    }

    #[test]
    fn test_helper_traits() {
        use rustyline::highlight::Highlighter;
        use rustyline::hint::Hinter;

        let helper = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".into()],
        };

        // Hinter
        assert_eq!(
            helper.hint(
                "test",
                4,
                &Context::new(&rustyline::history::DefaultHistory::new())
            ),
            None
        );

        // Highlighter
        assert_eq!(
            helper.highlight_prompt("prompt", true),
            Cow::Borrowed("prompt")
        );
        assert_eq!(helper.highlight_hint("hint"), Cow::Borrowed("hint"));
        assert_eq!(helper.highlight("line", 0), Cow::Borrowed("line"));
        assert!(helper.highlight_char("line", 0, rustyline::highlight::CmdKind::Other));

        // Validator
        // Note: ValidationContext::new is private in newer rustyline.
        // Since we use the default empty implementation, we don't need to test it here.
    }
}

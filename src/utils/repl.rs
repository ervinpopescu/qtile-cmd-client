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

        let mut candidates = Vec::new();
        let item_classes = vec!["layout", "group", "screen", "window", "bar", "widget"];

        let all_parts: Vec<&str> = line[..pos].split_whitespace().collect();
        let is_cd = !all_parts.is_empty() && all_parts[0] == "cd";

        // Determine the "base" object we are currently "at" in the typed line
        // If the line ends with a space, we are looking for things INSIDE the last part.
        // If not, we are looking for things INSIDE the penultimate part (or current_object).
        let navigation_parts = if line.ends_with(' ') {
            &all_parts[..]
        } else if all_parts.is_empty() {
            &[]
        } else {
            &all_parts[..all_parts.len() - 1]
        };

        let active_path = self.get_active_path(navigation_parts);

        // 1. Fetch available commands for the active path
        if !is_cd {
            let query = CommandQuery::new()
                .object(active_path.clone())
                .function("commands".to_string());

            if let Ok(CallResult::Value(Value::Array(cmds))) = self.client.call(query) {
                for cmd in cmds {
                    if let Some(cmd_str) = cmd.as_str() {
                        if cmd_str.starts_with(word) {
                            candidates.push(Pair {
                                display: cmd_str.to_string(),
                                replacement: cmd_str.to_string(),
                            });
                        }
                    }
                }
            }
        }

        // 2. Fetch available sub-objects/item classes
        for class in &item_classes {
            if class.starts_with(word) {
                candidates.push(Pair {
                    display: class.to_string(),
                    replacement: class.to_string(),
                });
            }
        }

        // 3. Fetch item instances if we just typed a class or are in one
        // Check if navigation_parts ends with a class, or if active_path's last is a class
        let mut class_to_query = None;
        if let Some(last) = navigation_parts.last() {
            if item_classes.contains(last) {
                class_to_query = Some(last.to_string());
            }
        }

        if class_to_query.is_none() {
            if let Some(last) = active_path.last() {
                if item_classes.contains(&last.as_str()) {
                    class_to_query = Some(last.clone());
                }
            }
        }

        if let Some(class) = class_to_query {
            // To get instances of 'class', we query 'items(class)' on its PARENT
            let mut parent_path = active_path.clone();
            if active_path.last() == Some(&class) {
                parent_path.pop();
            }

            let query = CommandQuery::new()
                .object(parent_path)
                .function("items".to_string())
                .args(vec![class]);

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
                                    display: inst_str.to_string(),
                                    replacement: inst_str.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        candidates.sort_by(|a, b| a.display.cmp(&b.display));
        candidates.dedup_by(|a, b| a.display == b.display);

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
        Self::new(false)
    }
}

impl Repl {
    pub fn new(framed: bool) -> Self {
        Self {
            client: QtileClient::new(framed),
            current_object: vec!["root".to_string()],
        }
    }

    /// Starts the interactive REPL loop.
    pub fn run(&mut self) -> anyhow::Result<()> {
        let config = Config::builder()
            .completion_type(CompletionType::List)
            .build();
        let mut rl: Editor<QtileHelper, DefaultHistory> = Editor::with_config(config)?;

        println!("Qtile REPL - type 'exit' or 'quit' to leave, 'help' for current object help.");
        println!("Use 'cd <object>' to move through the command graph.");

        loop {
            // Update helper with current state for completions
            let helper = QtileHelper {
                client: QtileClient::new(self.client.framed()),
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

        let mut items = Vec::new();
        let item_classes = vec!["layout", "group", "screen", "window", "bar", "widget"];

        // 1. Fetch commands
        let query = CommandQuery::new()
            .object(target_path.clone())
            .function("commands".to_string());
        match self.client.call(query) {
            Ok(CallResult::Value(Value::Array(cmds))) => {
                for cmd in cmds {
                    if let Some(s) = cmd.as_str() {
                        items.push(s.to_string());
                    }
                }
            }
            Ok(_) => {} // Not an array, unexpected but skip
            Err(e) => {
                // If it's just 'root' or something valid, we shouldn't fail.
                // But some paths might not support 'commands'.
                if args.is_empty() {
                    // Only report error if we are listing CURRENT and it fails
                    println!("Warning: Could not fetch commands: {e}");
                }
            }
        }

        // 2. Add item classes
        for class in &item_classes {
            items.push(class.to_string());
        }

        // 3. If the target is an item class, add instances
        if let Some(last) = target_path.last() {
            if item_classes.contains(&last.as_str()) {
                let mut parent = target_path.clone();
                parent.pop();
                let query = CommandQuery::new()
                    .object(parent)
                    .function("items".to_string())
                    .args(vec![last.clone()]);

                if let Ok(CallResult::Value(Value::Array(res))) = self.client.call(query) {
                    if res.len() >= 2 {
                        if let Value::Array(instances) = &res[1] {
                            for inst in instances {
                                let inst_str = match inst {
                                    Value::String(s) => s.clone(),
                                    Value::Number(n) => n.to_string(),
                                    _ => continue,
                                };
                                items.push(inst_str);
                            }
                        }
                    }
                }
            }
        }

        items.sort();
        items.dedup();

        if !items.is_empty() {
            let max_width = items.iter().map(|i| i.len()).max().unwrap_or(0) + 2;
            let terminal_width = 80;
            let cols = (terminal_width / max_width).max(1);

            for (idx, item) in items.iter().enumerate() {
                print!("{item:<max_width$}");
                if (idx + 1) % cols == 0 {
                    println!();
                }
            }
            if items.len() % cols != 0 {
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
            client: QtileClient::new(false),
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
            client: QtileClient::new(false),
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
        let repl = Repl::new(true);
        assert!(repl.client.framed());
        assert_eq!(repl.current_object, vec!["root"]);

        let default_repl = Repl::default();
        assert!(!default_repl.client.framed());
    }

    #[test]
    fn test_handle_line() {
        let mut repl = Repl::new(true);
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
            client: QtileClient::new(true),
            current_object: vec!["root".to_string()],
        };
        let history = DefaultHistory::new();
        let context = Context::new(&history);

        // Standard completion at root
        let (pos, candidates) = helper.complete("sta", 3, &context).unwrap();
        assert_eq!(pos, 0);
        // Fallback if qtile is not running, but in coverage env it is
        if !candidates.is_empty() {
            assert!(candidates.iter().any(|c| c.display == "status"));
        }

        // cd completion
        let (pos, candidates) = helper.complete("cd gro", 6, &context).unwrap();
        assert_eq!(pos, 3);
        assert!(candidates.iter().any(|c| c.display == "group"));

        // Completion inside a class
        let (pos, _candidates) = helper.complete("cd group ", 9, &context).unwrap();
        assert_eq!(pos, 9);
    }

    #[test]
    fn test_handle_cd_complex() {
        let mut repl = Repl::new(true);
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
        let repl = Repl::new(true);
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
        let mut repl = Repl::new(true);
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
        let repl = Repl::new(true);
        // This just prints, but we verify it doesn't panic
        repl.handle_call("status", &[]);
    }

    #[test]
    fn test_helper_traits() {
        use rustyline::highlight::Highlighter;
        use rustyline::hint::Hinter;

        let helper = QtileHelper {
            client: QtileClient::new(true),
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

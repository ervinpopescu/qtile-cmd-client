use crate::utils::client::{CallResult, CommandQuery, QtileClient};
use crate::utils::graph::ObjectType;
use rustyline::completion::{Completer, Pair};
use rustyline::config::BellStyle;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hint, Hinter};
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{ColorMode, CompletionType, Config, Context, EditMode, Editor, Helper};
use serde_json::Value;
use std::borrow::Cow;
use std::path::PathBuf;

// ── Config ──────────────────────────────────────────────────────────────────

/// Persistent REPL configuration loaded from `$XDG_CONFIG_HOME/qticc/config.toml`.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct ReplConfig {
    pub completion: CompletionConfig,
    pub history: HistoryConfig,
    pub editor: EditorConfig,
    pub display: DisplayConfig,
}

// ── [completion] ─────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
#[serde(default)]
pub struct CompletionConfig {
    /// Matching mode: `"prefix"` (default) or `"fuzzy"`.
    pub mode: String,
    /// Maximum number of mismatched characters tolerated in fuzzy mode (default 1).
    pub max_errors: usize,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            mode: "prefix".to_string(),
            max_errors: 1,
        }
    }
}

// ── [history] ────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// Maximum number of history entries to keep (default 1000).
    pub max_size: usize,
    /// Ignore consecutive duplicate entries (default true).
    pub ignore_duplicates: bool,
    /// Ignore entries that begin with a space (default false).
    pub ignore_space: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            ignore_duplicates: true,
            ignore_space: false,
        }
    }
}

// ── [editor] ─────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    /// Key binding mode: `"emacs"` (default) or `"vi"`.
    pub mode: String,
    /// Bell style: `"none"` (default), `"audible"`, or `"visible"`.
    pub bell: String,
    /// Show inline hints for uniquely-matched commands (default true).
    pub hints: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            mode: "emacs".to_string(),
            bell: "none".to_string(),
            hints: true,
        }
    }
}

// ── [display] ────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Terminal width used for `ls` column layout (default 80).
    pub terminal_width: usize,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { terminal_width: 80 }
    }
}

impl ReplConfig {
    /// Load from `$XDG_CONFIG_HOME/qticc/config.toml` (or `~/.config/qticc/config.toml`).
    /// Missing file is silently ignored; parse errors are reported but do not abort.
    pub fn load() -> Self {
        let path = {
            let base = std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    std::env::var("HOME")
                        .map(|h| PathBuf::from(h).join(".config"))
                        .unwrap_or_else(|_| PathBuf::from("."))
                });
            base.join("qticc/config.toml")
        };

        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("qticc: config parse error in {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn completion_mode(&self) -> CompletionMode {
        match self.completion.mode.to_ascii_lowercase().as_str() {
            "fuzzy" => CompletionMode::Fuzzy {
                max_errors: self.completion.max_errors,
            },
            _ => CompletionMode::Prefix,
        }
    }

    pub fn edit_mode(&self) -> EditMode {
        match self.editor.mode.to_ascii_lowercase().as_str() {
            "vi" | "vim" => EditMode::Vi,
            _ => EditMode::Emacs,
        }
    }

    pub fn bell_style(&self) -> BellStyle {
        match self.editor.bell.to_ascii_lowercase().as_str() {
            "audible" => BellStyle::Audible,
            "visible" => BellStyle::Visible,
            _ => BellStyle::None,
        }
    }
}

// ── CompletionMode ───────────────────────────────────────────────────────────

/// Controls how typed input is matched against completion candidates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CompletionMode {
    /// Only candidates that start with the typed prefix are shown (default).
    #[default]
    Prefix,
    /// Subsequence match: all typed chars must appear in order; up to `max_errors`
    /// typed chars may fail to match (tolerates typos / transpositions).
    Fuzzy { max_errors: usize },
}

impl CompletionMode {
    /// Returns true if `word` matches `candidate` under this mode.
    pub fn matches(self, candidate: &str, word: &str) -> bool {
        match self {
            Self::Prefix => candidate.starts_with(word),
            Self::Fuzzy { max_errors } => fuzzy_match(candidate, word, max_errors),
        }
    }
}

/// Subsequence fuzzy match: typed chars must appear in order in `candidate`.
/// Up to `max_errors` typed chars from the pattern may be absent; on a miss the
/// candidate position is NOT advanced, so the next pattern char tries from the same spot.
fn fuzzy_match(candidate: &str, pattern: &str, max_errors: usize) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let cand: Vec<u8> = candidate.to_ascii_lowercase().into_bytes();
    let pat: Vec<u8> = pattern.to_ascii_lowercase().into_bytes();
    let mut errors = 0usize;
    let mut ci = 0usize;
    for &pc in &pat {
        match cand[ci..].iter().position(|&cc| cc == pc) {
            Some(offset) => ci += offset + 1,
            None => {
                errors += 1;
                if errors > max_errors {
                    return false;
                }
                // don't advance ci — try next pattern char from same position
            }
        }
    }
    true
}

// ── CommandHint ──────────────────────────────────────────────────────────────

/// Hint that displays signature+description as ghost text but only inserts the
/// name suffix on accept (right arrow / End).
struct CommandHint {
    display: String,
    completion: String,
}

impl Hint for CommandHint {
    fn display(&self) -> &str {
        &self.display
    }
    fn completion(&self) -> Option<&str> {
        Some(&self.completion)
    }
}

// ── QtileHelper ──────────────────────────────────────────────────────────────

struct QtileHelper {
    client: QtileClient,
    current_object: Vec<String>,
    completion_mode: CompletionMode,
    hints: bool,
}

/// Apply a sequence of path segments onto a base path.
fn apply_segments(base: &mut Vec<String>, segments: &[&str]) {
    for &seg in segments {
        match seg {
            ".." => {
                if base.len() > 1 {
                    base.pop();
                }
            }
            "/" | "root" => *base = vec!["root".to_string()],
            _ => base.push(seg.to_string()),
        }
    }
}

/// Expand whitespace-split tokens on `/`, treating a leading `/` as an absolute-path
/// marker (resets to root). `screen/0` and `/screen/0` both work.
fn expand_path_args<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::new();
    for &arg in args {
        if arg.starts_with('/') {
            out.push("/");
            out.extend(
                arg.strip_prefix('/')
                    .unwrap_or("")
                    .split('/')
                    .filter(|s| !s.is_empty()),
            );
        } else {
            out.extend(arg.split('/').filter(|s| !s.is_empty()));
        }
    }
    out
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

        let expanded = expand_path_args(&parts[start_idx..]);
        apply_segments(&mut active_path, &expanded);
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
        let first = all_parts.first().copied().unwrap_or("");
        let is_nav_only = first == "cd" || first == "ls";

        let navigation_parts = if line.ends_with(' ') {
            &all_parts[..]
        } else if all_parts.is_empty() {
            &[]
        } else {
            &all_parts[..all_parts.len() - 1]
        };

        let active_path = self.get_active_path(navigation_parts);
        let mode = self.completion_mode;

        // 1. Commands — name only in display so multi-column layout fits without paging
        if !is_nav_only {
            let query = CommandQuery::new()
                .object(active_path.clone())
                .function("commands".to_string());

            if let Ok(CallResult::Value(Value::Array(cmds))) = self.client.call(query) {
                for v in &cmds {
                    if let Some(name) = v.as_str() {
                        if mode.matches(name, word) {
                            candidates.push(Pair {
                                display: name.to_string(),
                                replacement: name.to_string(),
                            });
                        }
                    }
                }
            }
        }

        // 2. Navigation candidates — same logic as handle_ls:
        //    active_path ends with an item_class → offer instances (no description)
        //    otherwise → offer valid child node types for the current node
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
                            if mode.matches(&inst_str, word) {
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
            // Path ends on a selector — find the enclosing node type and offer its children.
            let node_type = active_path
                .iter()
                .rev()
                .find(|t| item_class_names.contains(&t.as_str()))
                .map(|s| s.as_str())
                .unwrap_or("root");
            let obj = ObjectType::with_none(node_type).unwrap_or(ObjectType::Root);
            let valid_children = obj.children();
            for (name, doc) in ITEM_CLASSES {
                if valid_children.contains(name) && mode.matches(name, word) {
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
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<CommandHint> {
        if !self.hints || pos < line.len() {
            return None;
        }

        let all_parts: Vec<&str> = line.split_whitespace().collect();
        // Don't hint in navigation-only contexts.
        let first = all_parts.first().copied().unwrap_or("");
        if first == "cd" || first == "ls" {
            return None;
        }

        let word = all_parts.last()?;
        if word.is_empty() {
            return None;
        }

        // Derive the object path from everything typed before the current word.
        let nav_parts = if all_parts.len() > 1 {
            &all_parts[..all_parts.len() - 1]
        } else {
            &[]
        };
        let object_path = self.get_active_path(nav_parts);

        let query = CommandQuery::new()
            .object(object_path.clone())
            .function("commands".to_string());
        let cmds = match self.client.call(query) {
            Ok(CallResult::Value(Value::Array(a))) => a,
            _ => return None,
        };

        let matches: Vec<&str> = cmds
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|s| s.starts_with(word))
            .collect();

        if matches.len() != 1 {
            return None; // only hint on a unique match
        }

        let cmd = matches[0];
        let suffix = &cmd[word.len()..];

        let doc_query = CommandQuery::new()
            .object(object_path)
            .function("doc".to_string())
            .args(vec![cmd.to_string()]);

        if let Ok(CallResult::Value(Value::String(doc))) = self.client.call(doc_query) {
            let first_line = doc.lines().next().unwrap_or("").trim();
            let sig_start = first_line.find('(')?;
            let sig = &first_line[sig_start..];
            let desc = doc.lines().nth(1).unwrap_or("").trim();
            let doc_part = if desc.is_empty() {
                format!("  {sig}")
            } else {
                format!("  {sig}  —  {desc}")
            };
            Some(CommandHint {
                display: format!("{suffix}{doc_part}"),
                completion: suffix.to_string(),
            })
        } else if !suffix.is_empty() {
            Some(CommandHint {
                display: suffix.to_string(),
                completion: suffix.to_string(),
            })
        } else {
            None
        }
    }
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

// ── Repl ─────────────────────────────────────────────────────────────────────

/// Interactive shell for navigating the Qtile command graph and invoking functions.
pub struct Repl {
    pub(crate) client: QtileClient,
    pub(crate) current_object: Vec<String>,
    pub(crate) completion_mode: CompletionMode,
    pub(crate) cfg: ReplConfig,
}

impl Default for Repl {
    fn default() -> Self {
        Self::new()
    }
}

impl Repl {
    pub fn new() -> Self {
        let cfg = ReplConfig::load();
        let completion_mode = cfg.completion_mode();
        Self {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
            completion_mode,
            cfg,
        }
    }

    fn history_path() -> PathBuf {
        let base = std::env::var("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".local/state"))
                    .unwrap_or_else(|_| PathBuf::from("."))
            });
        base.join("qticc/history")
    }

    /// Starts the interactive REPL loop.
    pub fn run(&mut self) -> anyhow::Result<()> {
        let rl_config = Config::builder()
            .completion_type(CompletionType::List)
            .completion_show_all_if_ambiguous(true)
            .edit_mode(self.cfg.edit_mode())
            .bell_style(self.cfg.bell_style())
            .color_mode(ColorMode::Enabled)
            .max_history_size(self.cfg.history.max_size)?
            .history_ignore_dups(self.cfg.history.ignore_duplicates)?
            .history_ignore_space(self.cfg.history.ignore_space)
            .build();
        let mut rl: Editor<QtileHelper, DefaultHistory> = Editor::with_config(rl_config)?;
        let history_path = Self::history_path();
        if let Some(parent) = history_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = rl.load_history(&history_path);

        println!("Qtile REPL - type 'exit' or 'quit' to leave, 'help' for current object help.");
        println!("Use 'cd <object>' to move through the command graph.");

        loop {
            // Update helper with current state for completions
            let helper = QtileHelper {
                client: QtileClient::new(),
                current_object: self.current_object.clone(),
                completion_mode: self.completion_mode,
                hints: self.cfg.editor.hints,
            };
            rl.set_helper(Some(helper));

            let path_display = if self.current_object == ["root"] {
                "/".to_string()
            } else {
                format!("/{}", self.current_object[1..].join("/"))
            };
            let prompt = format!("{path_display} > ");
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
        let _ = rl.save_history(&history_path);
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
            let _ = self.handle_ls(&[]);
            return;
        }

        let expanded = expand_path_args(args);
        let mut next_obj = self.current_object.clone();
        apply_segments(&mut next_obj, &expanded);

        if next_obj == self.current_object {
            return;
        }

        // Verify the target exists before committing the navigation.
        let query = CommandQuery::new()
            .object(next_obj.clone())
            .function("commands".to_string());

        if self.client.call(query).is_ok() {
            self.current_object = next_obj;
        } else {
            println!("Error: Object '{}' not found.", expanded.join("/"));
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
    /// - Otherwise → child node types valid for the current node type
    pub(crate) fn ls_items(&self, target_path: &[String]) -> Vec<String> {
        const ITEM_CLASSES: &[&str] = &["layout", "group", "screen", "window", "bar", "widget"];

        let last = target_path.last().map(|s| s.as_str()).unwrap_or("root");
        let mut items = if ITEM_CLASSES.contains(&last) {
            // Path ends on a class name — list instances of that class.
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
            // Path ends on a selector (e.g. "0", "bottom") — find the enclosing node type
            // by scanning backward for the last item-class token, then use its known children.
            let node_type = target_path
                .iter()
                .rev()
                .find(|t| ITEM_CLASSES.contains(&t.as_str()))
                .map(|s| s.as_str())
                .unwrap_or("root");
            let obj = ObjectType::with_none(node_type).unwrap_or(ObjectType::Root);
            obj.children().iter().map(|s| s.to_string()).collect()
        };

        items.sort();
        items.dedup();
        items
    }

    pub(crate) fn handle_ls(&self, args: &[&str]) -> anyhow::Result<()> {
        let mut target_path = self.current_object.clone();
        let expanded = expand_path_args(args);
        apply_segments(&mut target_path, &expanded);

        let items = self.ls_items(&target_path);

        if items.is_empty() {
            println!("(none)");
        } else {
            let max_width = items.iter().map(|i| i.len()).max().unwrap_or(0) + 2;
            let terminal_width = self.cfg.display.terminal_width;
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

    fn helper() -> QtileHelper {
        QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string()],
            completion_mode: CompletionMode::default(),
            hints: true,
        }
    }

    #[test]
    fn test_get_active_path() {
        let h = helper();
        assert_eq!(h.get_active_path(&["cd", "group"]), vec!["root", "group"]);
        assert_eq!(
            h.get_active_path(&["cd", "group", "1"]),
            vec!["root", "group", "1"]
        );
        assert_eq!(h.get_active_path(&["ls", "layout"]), vec!["root", "layout"]);
        assert_eq!(h.get_active_path(&["group"]), vec!["root", "group"]);
    }

    #[test]
    fn test_get_active_path_navigation() {
        let h = QtileHelper {
            client: QtileClient::new(),
            current_object: vec!["root".to_string(), "group".to_string(), "1".to_string()],
            completion_mode: CompletionMode::default(),
            hints: true,
        };
        assert_eq!(h.get_active_path(&[".."]), vec!["root", "group"]);
        assert_eq!(h.get_active_path(&["cd", ".."]), vec!["root", "group"]);
        assert_eq!(
            h.get_active_path(&["cd", "layout"]),
            vec!["root", "group", "1", "layout"]
        );
        assert_eq!(h.get_active_path(&["cd", "/"]), vec!["root"]);
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
        let h = helper();
        let history = DefaultHistory::new();
        let context = Context::new(&history);

        let (pos, candidates) = h.complete("sta", 3, &context).unwrap();
        assert_eq!(pos, 0);
        if !candidates.is_empty() {
            assert!(candidates.iter().any(|c| c.replacement == "status"));
        }

        let (pos, candidates) = h.complete("cd gro", 6, &context).unwrap();
        assert_eq!(pos, 3);
        assert!(candidates.iter().any(|c| c.replacement == "group"));

        let (pos, _candidates) = h.complete("cd group ", 9, &context).unwrap();
        assert_eq!(pos, 9);
    }

    #[test]
    fn test_ls_items_at_root_returns_all_classes() {
        let repl = Repl::new();
        let items = repl.ls_items(&["root".to_string()]);
        // Root can access all node types including core.
        assert!(items.contains(&"bar".to_string()));
        assert!(items.contains(&"group".to_string()));
        assert!(items.contains(&"screen".to_string()));
        assert!(items.contains(&"window".to_string()));
    }

    #[test]
    fn test_ls_items_at_instance_returns_classes() {
        let repl = Repl::new();
        let path: Vec<String> = ["root", "group", "www"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let items = repl.ls_items(&path);
        // group children: layout, screen, window
        assert!(items.contains(&"layout".to_string()));
        assert!(items.contains(&"screen".to_string()));
        assert!(items.contains(&"window".to_string()));
        assert!(!items.contains(&"bar".to_string()));
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
        let items = repl.ls_items(&["root".to_string(), "screen".to_string()]);
        assert!(
            !items.is_empty(),
            "screen should have at least one instance"
        );
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
        let h = helper();
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (_, candidates) = h.complete("cd ", 3, &ctx).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.replacement.as_str()).collect();
        for class in &["bar", "group", "layout", "screen", "window", "widget"] {
            assert!(names.contains(class), "expected {class} in cd candidates");
        }
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
        let h = helper();
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (_, candidates) = h.complete("sta", 3, &ctx).unwrap();
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
        let h = helper();
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (_, candidates) = h.complete("cd gr", 5, &ctx).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.replacement.as_str()).collect();
        assert!(names.contains(&"group"));
        assert!(!names.contains(&"screen"));
        assert!(!names.contains(&"layout"));
    }

    #[test]
    fn test_complete_dedup() {
        use rustyline::history::DefaultHistory;
        use rustyline::Context;
        let h = helper();
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (_, candidates) = h.complete("cd ", 3, &ctx).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.replacement.as_str()).collect();
        let mut sorted = names.clone();
        sorted.dedup();
        assert_eq!(names, sorted, "candidates must not contain duplicates");
    }

    #[test]
    fn test_complete_sorted() {
        use rustyline::history::DefaultHistory;
        use rustyline::Context;
        let h = helper();
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);

        let (_, candidates) = h.complete("cd ", 3, &ctx).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.replacement.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "candidates must be sorted");
    }

    #[test]
    fn test_handle_cd_empty_args() {
        let mut repl = Repl::new();
        repl.handle_cd(&[]);
        assert_eq!(repl.current_object, vec!["root"]);
    }

    #[test]
    fn test_handle_line_dotdot() {
        let mut repl = Repl::new();
        repl.current_object = vec!["root".into(), "group".into()];
        assert!(!repl.handle_line(".."));
        assert_eq!(repl.current_object, vec!["root"]);
        assert!(!repl.handle_line(".."));
        assert_eq!(repl.current_object, vec!["root"]);
    }

    #[test]
    fn test_handle_cd_complex() {
        let mut repl = Repl::new();
        repl.handle_cd(&["group"]);
        repl.handle_cd(&["/"]);
        assert_eq!(repl.current_object, vec!["root"]);
        repl.handle_cd(&["root"]);
        assert_eq!(repl.current_object, vec!["root"]);
        repl.handle_cd(&[".."]);
        assert_eq!(repl.current_object, vec!["root"]);
        repl.handle_cd(&["group", "1"]);
    }

    #[test]
    fn test_handle_ls_complex() {
        let repl = Repl::new();
        assert!(repl.handle_ls(&[]).is_ok());
        assert!(repl.handle_ls(&["group"]).is_ok());
        assert!(repl.handle_ls(&[".."]).is_ok());
        assert!(repl.handle_ls(&["/"]).is_ok());
        let _ = repl.handle_ls(&["group", "1"]);
    }

    #[test]
    fn test_handle_line_navigation() {
        let mut repl = Repl::new();
        repl.handle_line("cd group");
        repl.handle_line("..");
        assert_eq!(repl.current_object, vec!["root"]);
        repl.handle_line("ls");
        assert!(repl.handle_line("exit"));
    }

    #[test]
    fn test_handle_call() {
        let repl = Repl::new();
        repl.handle_call("status", &[]);
    }

    #[test]
    fn test_fuzzy_match() {
        // Exact subsequence (0 errors)
        assert!(fuzzy_match("focus_back", "focus", 0));
        assert!(fuzzy_match("focus_back", "fcus", 0)); // valid subsequence: f→f c→c u→u s→s
                                                       // Non-subsequence character forces an error
        assert!(!fuzzy_match("status", "stxatus", 0)); // 'x' not in remaining "atus"
        assert!(fuzzy_match("status", "stxatus", 1)); // 1 error allowed
        assert!(fuzzy_match("focus_back", "focs", 1));
        assert!(fuzzy_match("status", "statxs", 1));
        assert!(!fuzzy_match("status", "xxxxx", 1));
        assert!(fuzzy_match("", "", 0));
        assert!(fuzzy_match("anything", "", 0));
    }

    #[test]
    fn test_completion_mode_prefix() {
        let mode = CompletionMode::Prefix;
        assert!(mode.matches("focus_back", "focus"));
        assert!(!mode.matches("focus_back", "fcus"));
        assert!(mode.matches("status", "sta"));
        assert!(!mode.matches("status", "tatus"));
    }

    #[test]
    fn test_completion_mode_fuzzy() {
        let mode = CompletionMode::Fuzzy { max_errors: 1 };
        assert!(mode.matches("focus_back", "focus"));
        assert!(mode.matches("focus_back", "fcus")); // 1 error
        assert!(!mode.matches("focus_back", "xyz")); // too many errors
    }

    #[test]
    fn test_helper_traits() {
        use rustyline::highlight::Highlighter;
        use rustyline::hint::Hinter;

        let h = helper();

        // Hinter returns None for non-unique / no-Qtile
        assert!(h
            .hint(
                "test",
                4,
                &Context::new(&rustyline::history::DefaultHistory::new())
            )
            .is_none());

        // Highlighter
        assert_eq!(h.highlight_prompt("prompt", true), Cow::Borrowed("prompt"));
        assert_eq!(h.highlight_hint("hint"), Cow::Borrowed("hint"));
        assert_eq!(h.highlight("line", 0), Cow::Borrowed("line"));
        assert!(h.highlight_char("line", 0, rustyline::highlight::CmdKind::Other));
    }
}

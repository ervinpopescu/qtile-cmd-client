use crate::utils::client::{CallResult, InteractiveCommandClient};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

/// An interactive shell for navigating the Qtile command graph and executing functions.
pub struct Repl {
    current_object: Vec<String>,
}

impl Default for Repl {
    fn default() -> Self {
        Self::new()
    }
}

impl Repl {
    pub fn new() -> Self {
        Self {
            current_object: vec!["root".to_string()],
        }
    }

    /// Starts the interactive REPL loop.
    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut rl = DefaultEditor::new()?;
        println!("Qtile REPL - type 'exit' or 'quit' to leave, 'help' for current object help.");
        println!("Use 'cd <object>' to move through the command graph.");

        loop {
            let prompt = format!("({}) > ", self.current_object.join("."));
            let readline = rl.readline(&prompt);
            match readline {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if line == "exit" || line == "quit" {
                        break;
                    }

                    rl.add_history_entry(line)?;

                    // Handle graph navigation
                    if let Some(stripped) = line.strip_prefix("cd ") {
                        let target = stripped.trim();
                        if target == ".." {
                            if self.current_object.len() > 1 {
                                self.current_object.pop();
                            }
                        } else if target == "/" || target == "root" {
                            self.current_object = vec!["root".to_string()];
                        } else {
                            // Validate existence by attempting to list commands
                            let mut next_obj = self.current_object.clone();
                            next_obj.extend(target.split_whitespace().map(|s| s.to_string()));

                            let res = InteractiveCommandClient::call(
                                Some(next_obj.clone()),
                                Some("commands".to_string()),
                                None,
                                false,
                            );

                            if res.is_ok() {
                                self.current_object = next_obj;
                            } else {
                                println!("Error: Object '{target}' not found or has no commands.");
                            }
                        }
                        continue;
                    }

                    // Execute line as a function call
                    let parts: Vec<String> =
                        line.split_whitespace().map(|s| s.to_string()).collect();
                    let function = parts[0].clone();
                    let args = if parts.len() > 1 {
                        Some(parts[1..].to_vec())
                    } else {
                        None
                    };

                    let res = InteractiveCommandClient::call(
                        Some(self.current_object.clone()),
                        Some(function),
                        args,
                        false,
                    );

                    match res {
                        Ok(CallResult::Value(val)) => println!("{val:#}"),
                        Ok(CallResult::Text(text)) => println!("{text}"),
                        Err(e) => println!("Error: {e}"),
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("CTRL-C");
                    break;
                }
                Err(ReadlineError::Eof) => {
                    println!("CTRL-D");
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
}

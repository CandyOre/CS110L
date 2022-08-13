use crate::debugger_command::DebuggerCommand;
use crate::inferior::{Inferior, Status};
use rustyline::error::ReadlineError;
use rustyline::Editor;

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    running: bool,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // TODO (milestone 3): initialize the DwarfData

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            running: false,
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if let Some(inferior) = Inferior::new(&self.target, &args) {
                        // Kill old inferior
                        self.try_kill_inferior();
                        // Create the inferior
                        self.inferior = Some(inferior);
                        self.running = true;
                        // Run the inferior
                        self.cont_inferior();
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Quit => {
                    self.try_kill_inferior();
                    return;
                }
                DebuggerCommand::Continue => {
                    self.cont_inferior();
                }
                DebuggerCommand::Kill => {
                    if self.running {
                        self.try_kill_inferior();
                    } else {
                        println!("No subprocess running");
                    }
                }
            }
        }
    }

    fn cont_inferior(&mut self) {
        if self.running {
            let inferior = self.inferior.as_mut().unwrap();
            match inferior.cont() {
                Ok(status) => {
                    println!("{}", status);
                    match status {
                        Status::Exited(_) |
                        Status::Signaled(_) => self.running = false,
                        _ => (),
                    }
                }
                Err(err) => {
                    println!("Error continuing subprocess: {}", err)
                }
            }
        } else {
            println!("No subprocess running!");
        }
    }

    fn try_kill_inferior(&mut self) {
        if self.running {
            let inferior = self.inferior.as_mut().unwrap();
            println!("Killing running subprocess (pid {})", inferior.pid());
            match inferior.kill() {
                Ok(status) => {
                    println!("{}", status);
                    self.running = false;
                }
                Err(err) => {
                    println!("Error killing subprocess: {}", err)
                }
            }
        }
        self.inferior = None;
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}

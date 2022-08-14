use crate::debugger_command::DebuggerCommand;
use crate::inferior::{Inferior, Status as InferiorStatus};
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::collections::HashMap;

pub struct Debugger {
    target: String,
    debug_data: DwarfData,
    history_path: String,
    readline: Editor<()>,
    breakpoints: HashMap<usize, Breakpoint>,
    inferior: Option<Inferior>,
    running: bool,
}

#[derive(Clone)]
pub struct Breakpoint {
    pub addr: usize,
    pub inst: u8,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // Initialize the DwarfData
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };
        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);
        debug_data.print();

        Debugger {
            target: target.to_string(),
            debug_data,
            history_path,
            readline,
            breakpoints: HashMap::new(),
            inferior: None,
            running: false,
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if let Some(inferior)
                            = Inferior::new(&self.target, &args, &mut self.breakpoints) {
                        // Kill old inferior
                        self.try_kill_inferior();
                        // Bind the inferior
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
                DebuggerCommand::Backtrace => {
                    self.print_inferior_backtrace();
                }
                DebuggerCommand::Breakpoint(location) => {
                    if let Some(addr) = self.parse_location(&location) {
                        self.try_add_breakpoint(addr);
                    } else {
                        println!("Invalid break location format!");
                    }
                }
            }
        }
    }

    fn cont_inferior(&mut self) {
        if self.running {
            let inferior = self.inferior.as_mut().unwrap();
            match inferior.cont(&self.breakpoints) {
                Ok(status) => {
                    println!("{}", status);
                    match status {
                        InferiorStatus::Exited(_) |
                        InferiorStatus::Signaled(_) => self.running = false,
                        InferiorStatus::Stopped(_, ip) => {
                            print!("Stopped at ");
                            inferior.try_print_location(&self.debug_data, Some(ip))
                                    .expect("Error printing stopped location");
                        }
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

    fn print_inferior_backtrace(&self) {
        if self.running {
            let inferior = self.inferior.as_ref().unwrap();
            match inferior.print_backtrace(&self.debug_data) {
                Ok(_) => (),
                Err(err) => {
                    println!("Error printing backtrace: {}", err);
                }
            }
        } else {
            println!("No subprocess running");
        }
    }

    fn parse_location(&self, loc: &str) -> Option<usize> {
        if loc.starts_with("*") {
            let loc = if loc.to_lowercase().starts_with("*0x") {
                &loc[3..]
            }
            else {
                &loc[1..]
            };
            usize::from_str_radix(loc, 16).ok()
        }
        else {
            match usize::from_str_radix(loc, 10) {
                Ok(line) => {
                    self.debug_data.get_addr_for_line(None, line)
                }
                Err(_) => {
                    self.debug_data.get_addr_for_function(None, loc)
                }
            }
        }
    }

    fn try_add_breakpoint(&mut self, addr: usize) {
        let mut bp = Breakpoint {addr: addr, inst: 0};
        if !self.running || self.inferior.as_mut().unwrap().add_breakpoint(&mut bp) {
            self.breakpoints.insert(addr, bp);
            println!("Set breakpoint {} at {:#x}",
                self.breakpoints.len() - 1,
                self.breakpoints.get(&addr).unwrap().addr);
        }
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

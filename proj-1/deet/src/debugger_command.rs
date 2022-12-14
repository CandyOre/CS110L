pub enum DebuggerCommand {
    Quit,
    Run(Vec<String>),
    Continue,
    Kill,
    Backtrace,
    Breakpoint(String),
}

impl DebuggerCommand {
    pub fn from_tokens(tokens: &Vec<&str>) -> Option<DebuggerCommand> {
        match tokens[0] {
            "q" | "quit" => Some(DebuggerCommand::Quit),
            "r" | "run" => {
                let args = tokens[1..].to_vec();
                Some(DebuggerCommand::Run(
                    args.iter().map(|s| s.to_string()).collect(),
                ))
            }
            "c" | "cont" | "continue" => Some(DebuggerCommand::Continue),
            "k" | "kill" => Some(DebuggerCommand::Kill),
            "bt" | "back" | "backtrace" => Some(DebuggerCommand::Backtrace),
            "b" | "break" => {
                let location = tokens[1];
                Some(DebuggerCommand::Breakpoint(location.to_string()))
            }
            // Default case:
            _ => None,
        }
    }
}

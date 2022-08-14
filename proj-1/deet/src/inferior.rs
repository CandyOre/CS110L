
use crate::dwarf_data::DwarfData;
use crate::debugger::Breakpoint;
use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::os::unix::process::CommandExt;
use std::process::Child;
use std::process::Command;
use std::fmt;
use std::mem::size_of;
use std::collections::HashMap;
use regex::Regex;

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Status::Stopped(sign, ip) => {
                write!(f, "Subprocess stopped (signal {}) (inst ptr {})", sign, ip)
            }
            Status::Exited(code) => {
                write!(f, "Subprocess exited (status {})", code)
            }
            Status::Signaled(sign) => {
                write!(f, "Subprocess exited due to a signal (signal {})", sign)
            }
        }
    }
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}

pub struct Inferior {
    child: Child,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, breakpoints: &mut HashMap<usize, Breakpoint>)
            -> Option<Inferior> {
        let child;
        unsafe {
            child = Command::new(target)
                .args(args)
                .pre_exec(child_traceme)
                .spawn()
                .ok()?;
        }
        let mut inferior = Inferior {child};
        inferior.wait(None).ok()?;
        for bp in breakpoints.clone().values() {
            match inferior.write_byte(bp.addr, 0xcc) {
                Ok(inst) => breakpoints.get_mut(&bp.addr).unwrap().inst = inst,
                Err(_) => println!("Invalid breapoint {:#x}", bp.addr),
            }
        }
        Some(inferior)
    }

    fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        Ok(orig_byte as u8)
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn cont(&self) -> Result<Status, nix::Error> {
        ptrace::cont(self.pid(), None)?;
        self.wait(None)
    }

    pub fn kill(&mut self) -> Result<Status, nix::Error> {
        self.child.kill().expect("Error killing inferior");
        self.wait(None)
    }

    pub fn print_backtrace(&self, data: &DwarfData) -> Result<(), nix::Error> {
        let regs = ptrace::getregs(self.pid())?;
        let mut rip = regs.rip as usize;
        let mut rbp = regs.rbp as usize;
        loop {
            let func = self.try_print_location(data, Some(rip))?;
            if func.unwrap_or("".to_string()) == "main" {
                break;
            }
            rip = ptrace::read(self.pid(), (rbp + 8) as ptrace::AddressType)? as usize;
            rbp = ptrace::read(self.pid(), rbp as ptrace::AddressType)? as usize;
        }
        Ok(())
    }

    pub fn try_print_location(&self, data: &DwarfData, rip: Option<usize>)
            -> Result<Option<String>, nix::Error> {
        let regs = ptrace::getregs(self.pid())?;
        let rip = rip.unwrap_or(regs.rip as usize);
        let line = data.get_line_from_addr(rip);
        let func = data.get_function_from_addr(rip);
        match (line, func) {
            (Some(line), Some(func)) => {
                let re = Regex::new(r"(.*deet/)").unwrap();
                let line = line.to_string();
                let line = re.replace_all(&line, "/deet/").to_string();

                println!("{} ({})", func, line);
                Ok(Some(func))
            }
            (_, _) => Ok(None),
        }
    }
}

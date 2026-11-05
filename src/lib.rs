// SPDX-License-Identifier: Apache-2.0
//! SigmaOS-alpm-hooks — ALPM hook runner inspired by Arch Linux pacman hooks.
//!
//! Arch runs executables from `/etc/pacman.d/hooks/` before and after package
//! transactions. This crate models that mechanism: hooks declare when they run
//! and receive transaction details on stdin.
//!
//! Principles: single responsibility, fail-fast, defensive parsing, stable API.

use std::fmt;
use std::io::{self, Read};

/// Errors produced while parsing hook input or running hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookError {
    /// Stdin did not match the expected `<OP>\n<pkg>...` format.
    MalformedInput,
    /// IO failure while reading hook input.
    Io(String),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedInput => write!(f, "malformed hook input"),
            Self::Io(msg) => write!(f, "io error: {msg}"),
        }
    }
}

impl std::error::Error for HookError {}

/// When a hook executes relative to a package transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookWhen {
    /// Before the transaction is committed.
    PreTransaction,
    /// After the transaction completes.
    PostTransaction,
}

impl HookWhen {
    /// Parse the `[Trigger]` operation token used by Arch hook files.
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "PreTransaction" => Some(Self::PreTransaction),
            "PostTransaction" => Some(Self::PostTransaction),
            _ => None,
        }
    }
}

/// A parsed hook trigger plus the packages involved in the transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookInput {
    /// Which transaction phase triggered the hook.
    pub when: HookWhen,
    /// Package names passed on stdin, one per line.
    pub packages: Vec<String>,
}

impl HookInput {
    /// Parse ALPM hook stdin: first line is the operation, remaining lines are
    /// package names. Fails fast on malformed input (empty stream, unknown op).
    pub fn parse(input: &str) -> Result<Self, HookError> {
        let mut lines = input.lines();
        let op = lines.next().ok_or(HookError::MalformedInput)?;
        let when = HookWhen::parse(op).ok_or(HookError::MalformedInput)?;
        let packages = lines
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(Self { when, packages })
    }

    /// Read and parse hook input from any `Read` source (stdin in production).
    pub fn read_from(mut r: impl Read) -> Result<Self, HookError> {
        let mut buf = String::new();
        r.read_to_string(&mut buf)
            .map_err(|e| HookError::Io(e.to_string()))?;
        Self::parse(&buf)
    }
}

/// A registered hook: a command to run for a given transaction phase.
#[derive(Debug, Clone)]
pub struct Hook {
    /// Execution phase.
    pub when: HookWhen,
    /// Executable path.
    pub executable: String,
    /// Optional arguments.
    pub args: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pre_transaction_with_packages() {
        let input = HookInput::parse("PreTransaction\nlinux\nmkinitcpio\n").unwrap();
        assert_eq!(input.when, HookWhen::PreTransaction);
        assert_eq!(input.packages, vec!["linux", "mkinitcpio"]);
    }

    #[test]
    fn parse_rejects_unknown_op() {
        assert_eq!(HookInput::parse("Bogus\nfoo\n"), Err(HookError::MalformedInput));
    }

    #[test]
    fn parse_rejects_empty_input() {
        assert_eq!(HookInput::parse(""), Err(HookError::MalformedInput));
    }

    #[test]
    fn read_from_stdin_bytes() {
        let input = HookInput::read_from(&b"PostTransaction\npacman\n"[..]).unwrap();
        assert_eq!(input.when, HookWhen::PostTransaction);
        assert_eq!(input.packages, vec!["pacman"]);
    }

    #[test]
    fn io_error_is_wrapped() {
        struct Bad;
        impl Read for Bad {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::Other, "boom"))
            }
        }
        assert!(matches!(HookInput::read_from(Bad), Err(HookError::Io(_))));
    }
}

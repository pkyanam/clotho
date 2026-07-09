//! Minimal argv helpers (no clap — keep the binary light, ADR-0010).

use std::path::PathBuf;

use anyhow::{bail, Result};

pub fn take_option(args: &mut Vec<String>, name: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == name)?;
    args.remove(pos);
    if pos >= args.len() {
        return None;
    }
    Some(args.remove(pos))
}

pub fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(pos) = args.iter().position(|a| a == name) {
        args.remove(pos);
        true
    } else {
        false
    }
}

pub fn take_repeated(args: &mut Vec<String>, name: &str) -> Vec<PathBuf> {
    let mut values = Vec::new();
    while let Some(pos) = args.iter().position(|a| a == name) {
        args.remove(pos);
        if pos < args.len() {
            values.push(PathBuf::from(args.remove(pos)));
        }
    }
    values
}

pub fn require_one(args: &[String], usage: &str) -> Result<String> {
    if args.len() != 1 {
        bail!("usage: {usage}");
    }
    Ok(args[0].clone())
}

pub fn require_two(args: &[String], usage: &str) -> Result<(String, String)> {
    if args.len() != 2 {
        bail!("usage: {usage}");
    }
    Ok((args[0].clone(), args[1].clone()))
}

pub fn strip_globals(args: &mut Vec<String>) -> (Option<String>, bool) {
    let api = take_option(args, "--api");
    let json = take_flag(args, "--json");
    (api, json)
}

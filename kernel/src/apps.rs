use alloc::string::String;
use crate::fs;

pub fn calculator(expression: &str) -> Option<i64> {
    let mut parts = expression.split_whitespace();
    let left: i64 = parts.next()?.parse().ok()?;
    let op = parts.next()?;
    let right: i64 = parts.next()?.parse().ok()?;
    match op {
        "+" => Some(left + right),
        "-" => Some(left - right),
        "*" => Some(left * right),
        "/" if right != 0 => Some(left / right),
        "%" if right != 0 => Some(left % right),
        _ => None,
    }
}

pub fn cat(cwd: &[String], name: &str) -> Option<String> {
    let root = fs::ROOT.lock();
    let mut dir = &*root;
    for component in cwd {
        dir = dir.dirs.iter().find(|d| d.name == *component)?;
    }
    dir.files.iter().find(|f| f.name == name).map(|f| f.data.clone())
}

use std::{
    io::{self, IsTerminal, Write},
    sync::LazyLock,
};

use anyhow::{bail, Error, Result};
use regex::Regex;
use serde_json::Value;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

const TAB_WIDTH: usize = 2;

fn normal(color: Color) -> ColorSpec {
    let mut spec = ColorSpec::new();
    spec.set_fg(Some(color));
    spec
}

fn bold(color: Color) -> ColorSpec {
    let mut spec = normal(color);
    spec.set_bold(true);
    spec
}

static KEY: LazyLock<ColorSpec> = LazyLock::new(|| normal(Color::Blue));
static STR: LazyLock<ColorSpec> = LazyLock::new(|| normal(Color::Green));
static HEADER: LazyLock<ColorSpec> = LazyLock::new(|| bold(Color::Blue));
static ERR: LazyLock<ColorSpec> = LazyLock::new(|| bold(Color::Red));

macro_rules! write_with_color {
    ($dst:expr, $color:expr, $($arg:tt)*) => {
        $dst.set_color(&$color)
            .and_then(|_| write!($dst, $($arg)*))
            .and_then(|_| $dst.reset())
    };
}

fn color_choice(t: &impl IsTerminal) -> ColorChoice {
    if t.is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    }
}

fn write_json(w: &mut impl WriteColor, depth: usize, value: &Value) -> Result<()> {
    match value {
        Value::Array(arr) => {
            write!(w, "[")?;
            for (i, e) in arr.iter().enumerate() {
                write!(w, "\n{}", " ".repeat((depth + 1) * TAB_WIDTH))?;
                write_json(w, depth + 1, e)?;
                if i == arr.len() - 1 {
                    write!(w, "\n{}", " ".repeat(depth * TAB_WIDTH))?;
                } else {
                    write!(w, ",")?;
                }
            }
            write!(w, "]")?;
        }
        Value::Object(obj) => {
            write!(w, "{{")?;
            for (i, (k, v)) in obj.iter().enumerate() {
                write!(w, "\n{}", " ".repeat((depth + 1) * TAB_WIDTH))?;
                write_with_color!(w, KEY, "{}", Value::String(k.clone()))?;
                write!(w, ": ")?;
                write_json(w, depth + 1, v)?;
                if i == obj.len() - 1 {
                    write!(w, "\n{}", " ".repeat(depth * TAB_WIDTH))?;
                } else {
                    write!(w, ",")?;
                }
            }
            write!(w, "}}")?;
        }
        Value::String(_) => write_with_color!(w, STR, "{value}")?,
        _ => write!(w, "{value}")?,
    }
    Ok(())
}

fn quote(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

fn yaml_flow_string(s: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\u{20}-\u{7e}]+$").unwrap());
    if RE.is_match(s)
        && !s.starts_with(|c: char| {
            c.is_whitespace() || c.is_ascii_digit() || "-?:,[]{}#&*!|>'\"%@`+.".contains(c)
        })
        && !s.contains(": ")
        && !s.contains(" #")
        && !s.ends_with(char::is_whitespace)
    {
        s.to_string()
    } else {
        quote(s)
    }
}

fn yaml_block_string(depth: usize, s: &str) -> String {
    let mut res = String::from("|");
    if s.starts_with(char::is_whitespace) {
        res.push_str(&format!("{TAB_WIDTH}"));
    }
    for line in s.lines() {
        res.push_str(&format!("\n{}{}", " ".repeat(depth * TAB_WIDTH), line));
    }
    res
}

fn yaml_string(depth: usize, s: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\u{20}-\u{7e}\n]+$").unwrap());
    if s.contains('\n') && RE.is_match(s) {
        yaml_block_string(depth, s)
    } else {
        yaml_flow_string(s)
    }
}

fn write_yaml(w: &mut impl WriteColor, depth: usize, obj_value: bool, value: &Value) -> Result<()> {
    match value {
        Value::Array(arr) => {
            if arr.is_empty() {
                if obj_value {
                    write!(w, " ")?;
                }
                write!(w, "[]")?;
            } else {
                for (i, e) in arr.iter().enumerate() {
                    if i > 0 || obj_value {
                        write!(w, "\n{}", " ".repeat(depth * TAB_WIDTH))?;
                    }
                    write!(w, "- ")?;
                    write_yaml(w, depth + 1, false, e)?;
                }
            }
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                if obj_value {
                    write!(w, " ")?;
                }
                write!(w, "{{}}")?;
            } else {
                for (i, (k, v)) in obj.iter().enumerate() {
                    if i > 0 || obj_value {
                        write!(w, "\n{}", " ".repeat(depth * TAB_WIDTH))?;
                    }
                    write_with_color!(w, KEY, "{}", yaml_flow_string(k))?;
                    write!(w, ":")?;
                    write_yaml(w, depth + 1, true, v)?;
                }
            }
        }
        Value::String(s) => {
            if obj_value {
                write!(w, " ")?;
            }
            write_with_color!(w, STR, "{}", yaml_string(depth, s))?;
        }
        _ => {
            if obj_value {
                write!(w, " ")?;
            }
            write!(w, "{value}")?;
        }
    }
    Ok(())
}

fn toml_key(s: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_\-]+$").unwrap());
    if RE.is_match(s) {
        s.to_string()
    } else {
        quote(s)
    }
}

fn write_toml_inline(w: &mut impl WriteColor, value: &Value) -> Result<()> {
    match value {
        Value::Array(arr) => {
            let arr = arr.iter().filter(|v| !v.is_null()).collect::<Vec<_>>();
            write!(w, "[")?;
            for (i, e) in arr.iter().enumerate() {
                write_toml_inline(w, e)?;
                if i != arr.len() - 1 {
                    write!(w, ", ")?;
                }
            }
            write!(w, "]")?;
        }
        Value::Object(obj) => {
            let obj = obj.iter().filter(|(_, v)| !v.is_null()).collect::<Vec<_>>();
            write!(w, "{{")?;
            for (i, (k, v)) in obj.iter().enumerate() {
                write_with_color!(w, KEY, " {}", toml_key(k))?;
                write!(w, " = ")?;
                write_toml_inline(w, v)?;
                if i == obj.len() - 1 {
                    write!(w, " ")?;
                } else {
                    write!(w, ",")?;
                }
            }
            write!(w, "}}")?;
        }
        _ => write_toml(w, "", value)?,
    }
    Ok(())
}

// TODO write objects with a single key using a dotted key rather than a new header
fn write_toml(w: &mut impl WriteColor, context: &str, value: &Value) -> Result<()> {
    fn is_object_array(value: &Value) -> bool {
        if let Value::Array(arr) = value {
            arr.iter().all(Value::is_object)
        } else {
            false
        }
    }

    fn should_nest(value: &Value) -> bool {
        value.is_object() || is_object_array(value)
    }

    match value {
        Value::Array(_) => write_toml_inline(w, value)?,
        Value::Object(obj) => {
            let obj = obj.iter().filter(|(_, v)| !v.is_null()).collect::<Vec<_>>();
            let flat = obj
                .iter()
                .filter(|(_, v)| !should_nest(v))
                .collect::<Vec<_>>();
            let nested = obj
                .iter()
                .filter(|(_, v)| should_nest(v))
                .collect::<Vec<_>>();

            for (i, &(k, v)) in flat.iter().enumerate() {
                write_with_color!(w, KEY, "{}", toml_key(k))?;
                write!(w, " = ")?;
                write_toml(w, context, v)?;
                if i != flat.len() - 1 {
                    writeln!(w)?;
                }
            }

            for (i, &(k, v)) in nested.iter().enumerate() {
                let k = format!("{}{}", context, toml_key(k));
                if !flat.is_empty() || i > 0 {
                    write!(w, "\n\n")?;
                }
                match v {
                    Value::Object(obj) => {
                        if obj.iter().any(|(_, v)| !should_nest(v)) {
                            write_with_color!(w, HEADER, "[{k}]\n")?;
                        }
                        write_toml(w, &format!("{k}."), v)?;
                    }
                    Value::Array(arr) => {
                        for (i, e) in arr.iter().enumerate() {
                            if i > 0 {
                                write!(w, "\n\n")?;
                            }
                            let Value::Object(obj) = e else {
                                unreachable!("arr only contains objects by construction");
                            };
                            write_with_color!(w, HEADER, "[[{k}]]")?;
                            if !obj.is_empty() {
                                writeln!(w)?;
                            }
                            write_toml(w, &format!("{k}."), e)?;
                        }
                    }
                    _ => unreachable!("nested contains objects and arrays by construction"),
                }
            }
        }
        Value::String(_) => write_with_color!(w, STR, "{value}")?,
        Value::Null => bail!("can't convert null to TOML"),
        _ => write!(w, "{value}")?,
    }
    Ok(())
}

pub fn json(s: &str) -> Result<()> {
    let mut stdout = StandardStream::stdout(color_choice(&io::stdout()));
    write_json(&mut stdout, 0, &s.parse()?)?;
    writeln!(&mut stdout)?;
    Ok(())
}

pub fn yaml(s: &str) -> Result<()> {
    let mut stdout = StandardStream::stdout(color_choice(&io::stdout()));
    write_yaml(&mut stdout, 0, false, &s.parse()?)?;
    writeln!(&mut stdout)?;
    Ok(())
}

pub fn toml(s: &str) -> Result<()> {
    let mut stdout = StandardStream::stdout(color_choice(&io::stdout()));
    write_toml(&mut stdout, "", &s.parse()?)?;
    writeln!(&mut stdout)?;
    Ok(())
}

pub fn error(err: &Error) -> Result<()> {
    let mut stderr = StandardStream::stderr(color_choice(&io::stderr()));
    write_with_color!(&mut stderr, ERR, "error")?;
    writeln!(&mut stderr, ": {err:#}")?;
    Ok(())
}

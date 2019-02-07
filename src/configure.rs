// configure.rs

use rlua::{Lua, Result, Table, Value};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;

// errors
#[derive(Debug, Clone)]
pub enum ConfigError {
    NilValueError(String),
    TypeError(String),
    NotSevenDaysError(String),
    LuaError(rlua::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ConfigError::NilValueError(ref message) => write!(fmt, "nil value for: {}", message),
            ConfigError::TypeError(ref message) => write!(fmt, "incorrect type for: {}", message),
            ConfigError::NotSevenDaysError(ref message) => {
                write!(fmt, "require seven day entries in: {}", message)
            }
            ConfigError::LuaError(ref message) => write!(fmt, "Lua error: {}", message),
        }
    }
}

impl std::error::Error for ConfigError {
    fn description(&self) -> &str {
        match *self {
            ConfigError::NilValueError(_) => "nil value",
            ConfigError::TypeError(_) => "incorrect type",
            ConfigError::NotSevenDaysError(_) => "require seven day entries",
            ConfigError::LuaError(_) => "Lua eror",
        }
    }
}

impl From<rlua::Error> for ConfigError {
    fn from(e: rlua::Error) -> Self {
        match e {
            _ => ConfigError::LuaError(e),
        }
    }
}

pub type StrMap = HashMap<String, String>;
pub type PointMap = HashMap<String, Point>;
pub type ThemeMap = HashMap<String, StrMap>;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, PartialEq)]
pub struct Configuration {
    pub socket: String,
    pub width: i32,
    pub height: i32,
    pub days: [String; 7],
    pub coordinates: PointMap,
    pub fonts: StrMap,
    pub themes: ThemeMap,
}

//type MyResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type MyResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn read(filename: &std::path::PathBuf, debug: bool) -> MyResult<Configuration> {
    if debug {
        println!("configuration file: {:?}", filename);
    }

    let file = File::open(filename)?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;

    if debug {
        println!("configuration text: {}", contents);
    }

    let lua = Lua::new();
    lua.context(|lua| {
        let arg = lua.create_table()?;

        let n = filename.to_str().unwrap().to_string();
        arg.set(0, n)?;

        let globals = lua.globals();
        globals.set("arg", arg)?;

        let config = lua.load(&contents).set_name("config")?.eval::<Table>()?;

        let socket = match config.get("socket") {
            Ok(v) => match v {
                Value::String(s) => Ok(s.to_str()?.to_string()),
                Value::Nil => Err(ConfigError::NilValueError("socket".to_string())),
                _ => Err(ConfigError::TypeError("socket".to_string())),
            },
            Err(_) => Err(ConfigError::TypeError("socket".to_string())),
        }?;

        let width = match config.get("width") {
            Ok(v) => match v {
                Value::Integer(n) => n as i32,
                _ => 0,
            },
            Err(_) => 0,
        };

        let height = match config.get("height") {
            Ok(v) => match v {
                Value::Integer(n) => n as i32,
                _ => 0,
            },
            Err(_) => 0,
        };

        let days = match config.get("days") {
            Ok(v) => match v {
                Value::Table(t) => {
                    if 7 != t.len()? {
                        Err(ConfigError::NotSevenDaysError("days".to_string()))
                    } else {
                        Ok(t)
                    }
                }
                Value::Nil => Err(ConfigError::NilValueError("dayss".to_string())),
                _ => Err(ConfigError::TypeError("fonts".to_string())),
            },
            Err(e) => Err(ConfigError::LuaError(e)),
        }?;
        let mut wd: [String; 7] = Default::default();
        let mut i = 0;
        for item in days.sequence_values::<String>() {
            wd[i] = item?;
            i += 1;
        }

        let fonts = match config.get("fonts") {
            Ok(v) => match v {
                Value::Table(t) => Ok(t),
                Value::Nil => Err(ConfigError::NilValueError("fonts".to_string())),
                _ => Err(ConfigError::TypeError("fonts".to_string())),
            },
            Err(e) => Err(ConfigError::LuaError(e)),
        }?;

        let themes = match config.get("themes") {
            Ok(v) => match v {
                Value::Table(t) => Ok(t),
                Value::Nil => Err(ConfigError::NilValueError("themes".to_string())),
                _ => Err(ConfigError::TypeError("themes".to_string())),
            },
            Err(e) => Err(ConfigError::LuaError(e)),
        }?;

        let coordinates = match config.get("coordinates") {
            Ok(v) => match v {
                Value::Table(t) => Ok(t),
                Value::Nil => Err(ConfigError::NilValueError("coordinates".to_string())),
                _ => Err(ConfigError::TypeError("coordinates".to_string())),
            },
            Err(e) => Err(ConfigError::LuaError(e)),
        }?;

        let cfg = Configuration {
            socket: socket,
            width: width,
            height: height,
            days: wd,
            fonts: make_map(fonts)?,
            coordinates: points_map(coordinates)?,
            themes: nested_map(themes)?,
        };

        Ok(cfg)
    })
}

fn make_map(item: Table) -> Result<HashMap<String, String>> {
    let mut m: HashMap<String, String> = HashMap::new();
    for pair in item.pairs::<String, String>() {
        let (key, value) = pair?;
        m.insert(key, value);
    }
    Ok(m)
}

fn nested_map(item: Table) -> Result<HashMap<String, StrMap>> {
    let mut m: HashMap<String, StrMap> = HashMap::new();
    for pair in item.pairs::<String, Table>() {
        let (key, value) = pair?;
        m.insert(key, make_map(value)?);
    }
    Ok(m)
}

fn points_map(item: Table) -> Result<HashMap<String, Point>> {
    let mut m: HashMap<String, Point> = HashMap::new();
    for pair in item.pairs::<String, Table>() {
        let (key, value) = pair?;
        m.insert(key, make_point(value));
    }
    Ok(m)
}

fn make_point(item: Table) -> Point {
    let x = match item.get("x") {
        Ok(v) => match v {
            Value::Integer(n) => n as i32,
            _ => 0,
        },
        Err(_) => 0,
    };

    let y = match item.get("y") {
        Ok(v) => match v {
            Value::Integer(n) => n as i32,
            _ => 0,
        },
        Err(_) => 0,
    };

    Point { x: x, y: y }
}

//! `script-*` attributes (Blish HUD Pathing only, docs/03): checks that each names a Lua function
//! the pack's scripts define, the way Pathing calls it. The editor doesn't run or syntax-check
//! Lua (that's for a real Lua tool): it reads the `.lua` files just enough to find their
//! functions and `Pack:Require` calls.
//!
//! How Pathing calls them (`Behavior/Script.cs`, `Scripting/ScriptEngine.cs`):
//! - `script-once="F(1,'a')"` calls the global `F(entity, 1, "a")`; the other hooks pass one
//!   more value first (`HOOKS`). The name is looked up as one global (ignoring case), so
//!   `Table.F` never resolves, and a missing function silently does nothing.
//! - The value is split on the first `(`, the arguments on `,`, with no quoting. Each argument
//!   is typed by [`read_arg`]. An empty argument makes Pathing drop the whole attribute.
//! - Scripts start at `pack.lua` (archive root) and load others with `Pack:Require("path")`
//!   (lowercased, `.lua` added, case-insensitive lookup, once per file).

use crate::{
    attrs,
    editor::Selection,
    files::resolve,
    pack::{Category, Pack},
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// The entry point Pathing loads.
pub const ENTRY: &str = "pack.lua";
/// Files bigger than this aren't read.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FILES: usize = 512;
const MAX_DEPTH: usize = 12;

/// One `script-*` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hook {
    /// The attribute, in its canonical casing.
    pub attr: &'static str,
    pub label: &'static str,
    /// What Pathing passes before the attribute's own arguments.
    pub passes: &'static [&'static str],
    pub when: &'static str,
}

pub const HOOKS: [Hook; 5] = [
    Hook {
        attr: "script-once",
        label: "Once",
        passes: &["marker"],
        when: "Called once, after all packs are loaded.",
    },
    Hook {
        attr: "script-tick",
        label: "Every tick",
        passes: &["marker", "gameTime"],
        when: "Called on every update. Keep it cheap.",
    },
    Hook {
        attr: "script-focus",
        label: "Focus",
        passes: &["marker", "isFocused"],
        when: "Called with true when the player comes within triggerRange, false when they leave.",
    },
    Hook {
        attr: "script-trigger",
        label: "Trigger",
        passes: &["marker", "isAutoTriggered"],
        when: "Called when the player presses the interact key in range, or on auto-trigger.",
    },
    Hook {
        attr: "script-filter",
        label: "Filter",
        passes: &["marker"],
        when: "Called every frame while it would show. Returning true hides it.",
    },
];

#[cfg(test)]
pub fn hook(attr: &str) -> Option<&'static Hook> {
    HOOKS.iter().find(|h| h.attr.eq_ignore_ascii_case(attr))
}

// ---- Calls ----

/// An argument as Pathing reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Nil,
    Int(i32),
    Float(f32),
    Bool(bool),
    Text(String),
}

impl Arg {
    /// How it reaches Lua, e.g. `"abc"` or `12`.
    pub fn describe(&self) -> String {
        match self {
            Arg::Nil => "nil".to_owned(),
            Arg::Int(n) => n.to_string(),
            Arg::Float(f) => format!("{f:?}"),
            Arg::Bool(b) => b.to_string(),
            Arg::Text(s) => format!("{s:?}"),
        }
    }
}

/// A `script-*` value as Pathing splits it.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub name: String,
    pub args: Vec<Arg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Pathing won't call the function as meant.
    Error,
    /// It's called, but probably not as the author thinks.
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    pub text: String,
}

fn error(text: impl Into<String>) -> Problem {
    Problem {
        severity: Severity::Error,
        text: text.into(),
    }
}

fn warning(text: impl Into<String>) -> Problem {
    Problem {
        severity: Severity::Warning,
        text: text.into(),
    }
}

/// Pathing's `GetValueFromString`: unquoted `nil`, a float if it has a `.`, an int, `true` /
/// `false` (any casing), else text with the `'` and `"` around it trimmed. Nothing else is
/// trimmed, except the whitespace .NET's number parsing allows.
pub fn read_arg(raw: &str) -> Arg {
    let quoted = raw.starts_with(['\'', '"']);
    if !quoted {
        if raw == "nil" {
            return Arg::Nil;
        }
        if raw.contains('.') {
            return match raw.trim().parse::<f32>() {
                Ok(f) => Arg::Float(f),
                Err(_) => Arg::Text(raw.to_owned()),
            };
        }
        if let Ok(n) = raw.trim().parse::<i32>() {
            return Arg::Int(n);
        }
        if raw.eq_ignore_ascii_case("true") {
            return Arg::Bool(true);
        }
        if raw.eq_ignore_ascii_case("false") {
            return Arg::Bool(false);
        }
    }
    Arg::Text(raw.trim_matches(['\'', '"']).to_owned())
}

/// Splits `value` like Pathing's `SplitFunc`. `None` (with an error) when Pathing would ignore
/// it.
pub fn parse_call(value: &str) -> (Option<Call>, Vec<Problem>) {
    let mut problems = Vec::new();
    if value.trim().is_empty() {
        problems.push(error("Empty: nothing is called. Clear it instead."));
        return (None, problems);
    }
    let (name, args) = match value.split_once('(') {
        None => (value, Vec::new()),
        Some((name, rest)) => {
            if rest.contains('(') {
                problems.push(error(
                    "Only one ( is allowed: Pathing drops everything after a second one, so \
                     arguments can't contain brackets.",
                ));
            }
            let rest = rest.split('(').next().unwrap_or_default();
            if let Some(close) = rest.find(')')
                && !rest[close..].trim_start_matches([' ', ')']).is_empty()
            {
                problems.push(warning(
                    "Text after the ) is read as part of the last argument.",
                ));
            }
            let args_text = rest.trim_matches([' ', ')']);
            if args_text.is_empty() {
                (name, Vec::new())
            } else {
                let raw: Vec<&str> = args_text.split(',').collect();
                if raw.iter().any(|a| a.is_empty()) {
                    problems.push(error(
                        "An argument is empty (two commas in a row, or one at the end): \
                         Pathing ignores the whole attribute.",
                    ));
                    return (None, problems);
                }
                for a in &raw {
                    if *a != a.trim() && read_arg(a) != read_arg(a.trim()) {
                        problems.push(warning(format!(
                            "\"{a}\" is read as {} because of the spaces: Pathing doesn't trim \
                             them. Write the arguments without spaces after the commas.",
                            read_arg(a).describe()
                        )));
                    }
                }
                (name, raw.into_iter().map(read_arg).collect())
            }
        }
    };
    if name != name.trim() {
        problems.push(error(format!(
            "Remove the spaces around \"{}\": Pathing looks the name up with them.",
            name.trim()
        )));
    }
    let trimmed = name.trim();
    if trimmed.contains(['.', ':']) {
        problems.push(error(format!(
            "Pathing calls a global function by its exact name, so \"{trimmed}\" (a function \
             in a table) is never found. Use a global function, e.g. {}.",
            trimmed.replace(['.', ':'], "_")
        )));
    } else if !is_identifier(trimmed) {
        problems.push(error(format!(
            "\"{trimmed}\" isn't a Lua function name (letters, digits and _, not starting with a digit)."
        )));
    }
    (
        Some(Call {
            name: name.to_owned(),
            args,
        }),
        problems,
    )
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !KEYWORDS.contains(&s)
}

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

// ---- Lua files ----

/// A function definition found in a `.lua` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaFunction {
    pub name: String,
    pub params: Vec<String>,
    /// Ends with `...`.
    pub vararg: bool,
    /// `local function F` or `local F = function`: Pathing can't call it.
    pub local: bool,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaFile {
    /// Relative to the pack folder, with `/`.
    pub path: String,
    /// Whether Pathing runs it (`pack.lua`, or required from a file it runs), and in which
    /// order.
    pub load_order: Option<usize>,
    pub functions: Vec<LuaFunction>,
    /// `Pack:Require` targets, as written, with their line.
    pub requires: Vec<(String, usize)>,
}

/// The pack's `.lua` files and what they define.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LuaScan {
    /// Loaded files first, in load order, then the others by path.
    pub files: Vec<LuaFile>,
    /// Problems with the files themselves (missing requires, unreadable files).
    pub problems: Vec<Problem>,
}

impl LuaScan {
    /// Reads every `.lua` file under `dir`, and follows `Pack:Require` from `pack.lua`.
    pub fn scan(dir: &Path) -> Self {
        let mut on_disk = Vec::new();
        find_lua_files(dir, dir, 0, &mut on_disk);
        on_disk.sort();
        let mut scan = Self::default();
        let mut files: Vec<LuaFile> = Vec::new();
        for (relative, path) in &on_disk {
            match read_capped(path) {
                Ok(source) => {
                    let (functions, requires) = read_lua(&source);
                    files.push(LuaFile {
                        path: relative.clone(),
                        load_order: None,
                        functions,
                        requires,
                    });
                }
                Err(text) => scan
                    .problems
                    .push(warning(format!("{relative} couldn't be read: {text}."))),
            }
        }

        // Follow the requires like Pathing: depth first, each file once.
        let mut order = 0;
        let mut stack: Vec<(String, Option<(String, usize)>)> = vec![(ENTRY.to_owned(), None)];
        let mut seen = BTreeSet::new();
        while let Some((wanted, from)) = stack.pop() {
            let key = wanted.to_lowercase();
            if !seen.insert(key.clone()) {
                continue;
            }
            match files.iter_mut().find(|f| f.path.to_lowercase() == key) {
                Some(file) => {
                    file.load_order = Some(order);
                    order += 1;
                    for (target, line) in file.requires.iter().rev() {
                        stack.push((require_path(target), Some((file.path.clone(), *line))));
                    }
                }
                None => {
                    if let Some((file, line)) = from {
                        scan.problems.push(error(format!(
                            "{file}:{line} requires {wanted}, which isn't in the pack folder."
                        )));
                    }
                }
            }
        }
        files.sort_by(|a, b| match (a.load_order, b.load_order) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.path.cmp(&b.path),
        });
        scan.files = files;
        scan
    }

    pub fn has_entry(&self) -> bool {
        self.files.first().is_some_and(|f| f.load_order.is_some())
    }

    /// Global functions Pathing can call named `name` (any casing), in load order.
    fn globals(&self, name: &str) -> Vec<(&LuaFile, &LuaFunction)> {
        self.files
            .iter()
            .filter(|f| f.load_order.is_some())
            .flat_map(|f| f.functions.iter().map(move |func| (f, func)))
            .filter(|(_, func)| !func.local && func.name.eq_ignore_ascii_case(name))
            .collect()
    }

    /// The function Pathing calls for `name`: the last global one loaded.
    pub fn function(&self, name: &str) -> Option<(&LuaFile, &LuaFunction)> {
        self.globals(name).last().copied()
    }

    /// Every definition named `name` (any casing) that Pathing can't call.
    fn unreachable(&self, name: &str) -> Vec<(&LuaFile, &LuaFunction)> {
        self.files
            .iter()
            .flat_map(|f| f.functions.iter().map(move |func| (f, func)))
            .filter(|(f, func)| {
                (func.local || f.load_order.is_none()) && func.name.eq_ignore_ascii_case(name)
            })
            .collect()
    }
}

/// What Pathing opens for `Pack:Require(target)`.
fn require_path(target: &str) -> String {
    let mut path = target
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_lowercase();
    if !path.ends_with(".lua") {
        path.push_str(".lua");
    }
    path
}

fn find_lua_files(root: &Path, dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf)>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let Some(relative) = path
            .strip_prefix(root)
            .ok()
            .and_then(|p| p.to_str())
            .map(|p| p.replace('\\', "/"))
        else {
            continue;
        };
        if kind.is_dir() {
            // Type stubs for IntelliSense (docs/03 §6) define the API, not the pack.
            if !relative.to_lowercase().contains("libdef") {
                find_lua_files(root, &path, depth + 1, out);
            }
        } else if kind.is_file()
            && relative.to_lowercase().ends_with(".lua")
            && resolve(root, &relative).is_some()
        {
            out.push((relative, path));
        }
    }
}

fn read_capped(path: &Path) -> Result<String, String> {
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_FILE_BYTES {
        return Err(format!("bigger than {} MB", MAX_FILE_BYTES / 1024 / 1024));
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ---- A minimal Lua tokenizer: enough to find definitions and requires ----

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Str(String),
    Number,
    Sym(&'static str),
    Other,
}

fn tokenize(source: &str) -> Vec<(Token, usize)> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '-' && at(i + 1) == '-' {
            i += 2;
            if let Some(level) = long_bracket(&chars, i) {
                let (end, lines) = skip_long(&chars, i, level);
                line += lines;
                i = end;
            } else {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
        } else if c == '[' && long_bracket(&chars, i).is_some() {
            let level = long_bracket(&chars, i).unwrap_or(0);
            let start_line = line;
            let open = i + level + 2;
            let (end, lines) = skip_long(&chars, i, level);
            line += lines;
            let close = end.saturating_sub(level + 2).max(open);
            let text: String = chars[open.min(chars.len())..close.min(chars.len())]
                .iter()
                .collect();
            tokens.push((
                Token::Str(text.trim_start_matches('\n').to_owned()),
                start_line,
            ));
            i = end;
        } else if c == '"' || c == '\'' {
            let start_line = line;
            let mut text = String::new();
            i += 1;
            while i < chars.len() && chars[i] != c && chars[i] != '\n' {
                if chars[i] == '\\' {
                    if at(i + 1) == '\n' {
                        line += 1;
                    }
                    text.push(at(i + 1));
                    i += 2;
                } else {
                    text.push(chars[i]);
                    i += 1;
                }
            }
            i += 1;
            tokens.push((Token::Str(text), start_line));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push((Token::Name(chars[start..i].iter().collect()), line));
        } else if c.is_ascii_digit() || (c == '.' && at(i + 1).is_ascii_digit()) {
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric()
                    || chars[i] == '.'
                    || ((chars[i] == '-' || chars[i] == '+')
                        && matches!(chars[i - 1], 'e' | 'E' | 'p' | 'P')))
            {
                i += 1;
            }
            tokens.push((Token::Number, line));
        } else {
            const SYMS: &[&str] = &[
                "...", "..", "==", "~=", "<=", ">=", "::", "//", "<<", ">>", "(", ")", ",", "=",
                ":", ".", "{", "}", "[", "]", ";",
            ];
            match SYMS
                .iter()
                .find(|s| s.chars().enumerate().all(|(k, sc)| at(i + k) == sc))
            {
                Some(sym) => {
                    tokens.push((Token::Sym(sym), line));
                    i += sym.len();
                }
                None => {
                    tokens.push((Token::Other, line));
                    i += 1;
                }
            }
        }
    }
    tokens
}

/// The level of a long bracket `[[` / `[==[` starting at `i`.
fn long_bracket(chars: &[char], i: usize) -> Option<usize> {
    if chars.get(i) != Some(&'[') {
        return None;
    }
    let level = chars[i + 1..].iter().take_while(|&&c| c == '=').count();
    (chars.get(i + 1 + level) == Some(&'[')).then_some(level)
}

/// Skips a long bracket opened at `i`. Returns the index after it and the newlines inside.
fn skip_long(chars: &[char], i: usize, level: usize) -> (usize, usize) {
    let mut j = i + level + 2;
    let mut lines = 0;
    while j < chars.len() {
        if chars[j] == '\n' {
            lines += 1;
        }
        if chars[j] == ']'
            && chars[j + 1..].iter().take(level).all(|&c| c == '=')
            && chars.get(j + 1 + level) == Some(&']')
        {
            return (j + level + 2, lines);
        }
        j += 1;
    }
    (chars.len(), lines)
}

/// The functions a Lua source defines and the files it requires.
fn read_lua(source: &str) -> (Vec<LuaFunction>, Vec<(String, usize)>) {
    let tokens = tokenize(source);
    let tok = |i: usize| tokens.get(i).map(|(t, _)| t);
    let name_at = |i: usize| match tok(i) {
        Some(Token::Name(n)) => Some(n.as_str()),
        _ => None,
    };
    let sym_at = |i: usize, s: &str| matches!(tok(i), Some(Token::Sym(x)) if *x == s);
    let mut functions = Vec::new();
    let mut requires = Vec::new();

    for i in 0..tokens.len() {
        let line = tokens[i].1;
        // Pack:Require("file") or Pack:Require "file"
        if name_at(i) == Some("Pack") && sym_at(i + 1, ":") && name_at(i + 2) == Some("Require") {
            let arg = if sym_at(i + 3, "(") { i + 4 } else { i + 3 };
            if let Some(Token::Str(path)) = tok(arg) {
                requires.push((path.clone(), line));
            }
            continue;
        }
        let local = i > 0 && name_at(i - 1) == Some("local");
        // function F(...) / function _G.F(...); not function T.F / T:F (table members).
        if name_at(i) == Some("function") {
            let mut n = i + 1;
            if name_at(n) == Some("_G") && sym_at(n + 1, ".") {
                n += 2;
            }
            if let Some(name) = name_at(n)
                && sym_at(n + 1, "(")
            {
                let (params, vararg) = read_params(&tokens, n + 2);
                functions.push(LuaFunction {
                    name: name.to_owned(),
                    params,
                    vararg,
                    local,
                    line,
                });
            }
            continue;
        }
        // F = function(...) / _G.F = function(...), not inside a table constructor.
        if let Some(name) = name_at(i)
            && sym_at(i + 1, "=")
            && name_at(i + 2) == Some("function")
            && sym_at(i + 3, "(")
        {
            let before = i.checked_sub(1).and_then(tok);
            let global_table = i >= 2 && name_at(i - 2) == Some("_G") && sym_at(i - 1, ".");
            let member = matches!(before, Some(Token::Sym("." | ":" | "{" | ",")));
            if global_table || !member {
                let (params, vararg) = read_params(&tokens, i + 4);
                functions.push(LuaFunction {
                    name: name.to_owned(),
                    params,
                    vararg,
                    local,
                    line,
                });
            }
        }
    }
    (functions, requires)
}

fn read_params(tokens: &[(Token, usize)], mut i: usize) -> (Vec<String>, bool) {
    let mut params = Vec::new();
    let mut vararg = false;
    while let Some((token, _)) = tokens.get(i) {
        match token {
            Token::Name(n) => params.push(n.clone()),
            Token::Sym("...") => vararg = true,
            Token::Sym(",") => {}
            _ => break,
        }
        i += 1;
    }
    (params, vararg)
}

// ---- Checking attributes against the scripts ----

/// What's wrong with `value` for `hook`, given the pack's scripts.
pub fn check(hook: &Hook, value: &str, scan: &LuaScan) -> Vec<Problem> {
    let (call, mut problems) = parse_call(value);
    let Some(call) = call else {
        return problems;
    };
    if problems.iter().any(|p| p.severity == Severity::Error) {
        return problems;
    }
    let name = call.name.as_str();
    let found = scan.globals(name);
    let Some(&(file, func)) = found.last() else {
        let unreachable = scan.unreachable(name);
        if let Some((file, func)) = unreachable.iter().find(|(f, _)| f.load_order.is_some()) {
            problems.push(error(format!(
                "{} is local in {}:{}: Pathing can only call global functions. Remove the `local`.",
                func.name, file.path, func.line
            )));
        } else if let Some((file, func)) = unreachable.first() {
            problems.push(error(format!(
                "{} is in {}:{}, but Pathing never loads that file. {}",
                func.name,
                file.path,
                func.line,
                if scan.has_entry() {
                    format!(
                        "Add Pack:Require(\"{}\") to {ENTRY} (or to a file it requires).",
                        file.path
                    )
                } else {
                    format!("Pathing starts at {ENTRY}, which the pack doesn't have.")
                }
            )));
        } else if !scan.has_entry() {
            problems.push(error(format!(
                "The pack has no {ENTRY}, so Pathing loads no scripts and {name} doesn't exist."
            )));
        } else {
            problems.push(error(format!(
                "No global function {name} in the scripts Pathing loads. Pathing silently skips the call."
            )));
        }
        return problems;
    };

    if func.name != name {
        problems.push(warning(format!(
            "It's spelled {} in {}:{}. Pathing ignores case here, but use the same spelling.",
            func.name, file.path, func.line
        )));
    }
    if found.len() > 1 {
        let places: Vec<String> = found
            .iter()
            .map(|(f, func)| format!("{}:{}", f.path, func.line))
            .collect();
        problems.push(warning(format!(
            "{name} is defined {} times ({}): the last one loaded wins.",
            found.len(),
            places.join(", ")
        )));
    }

    let given: Vec<String> = hook
        .passes
        .iter()
        .map(|p| (*p).to_owned())
        .chain(call.args.iter().map(Arg::describe))
        .collect();
    let takes = func.params.len();
    if takes < given.len() && !func.vararg {
        problems.push(warning(format!(
            "{}({}) takes {takes} parameter(s), but Pathing passes {}: {}. The extra ones are dropped.",
            func.name,
            func.params.join(", "),
            given.len(),
            given.join(", ")
        )));
    } else if takes > given.len() {
        problems.push(warning(format!(
            "{}({}) gets {}: {} will be nil.",
            func.name,
            func.params.join(", "),
            given.join(", "),
            func.params[given.len()..].join(", ")
        )));
    }
    problems
}

/// One `script-*` attribute set in the pack, with its problems.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub item: Selection,
    /// "Marker #3", "Category mypack.chests", ...
    pub label: String,
    pub hook: &'static Hook,
    pub value: String,
    pub problems: Vec<Problem>,
}

/// Every `script-*` attribute the pack sets (on categories, markers and trails), checked.
pub fn pack_report(pack: &Pack, scan: &LuaScan) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut add = |item: Selection, label: String, attrs: &[(String, String)]| {
        for hook in &HOOKS {
            if let Some(value) = attrs::get(attrs, hook.attr) {
                out.push(Finding {
                    item: item.clone(),
                    label: label.clone(),
                    hook,
                    value: value.to_owned(),
                    problems: check(hook, value, scan),
                });
            }
        }
    };
    fn walk(
        categories: &[Category],
        parent: &str,
        add: &mut impl FnMut(Selection, String, &[(String, String)]),
    ) {
        for category in categories {
            let ns = if parent.is_empty() {
                category.name.clone()
            } else {
                format!("{parent}.{}", category.name)
            };
            add(
                Selection::Category(ns.clone()),
                format!("Category {ns}"),
                &category.attrs,
            );
            walk(&category.children, &ns, add);
        }
    }
    walk(&pack.categories, "", &mut add);
    for (i, poi) in pack.pois.iter().enumerate() {
        add(Selection::poi(i), format!("Marker #{}", i + 1), &poi.attrs);
    }
    for (i, trail) in pack.trails.iter().enumerate() {
        add(
            Selection::Trail(i),
            format!("Trail #{}", i + 1),
            &trail.attrs,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::Poi;

    fn hook_of(attr: &str) -> &'static Hook {
        hook(attr).unwrap()
    }

    #[test]
    fn reading_args_like_pathing() {
        assert_eq!(read_arg("nil"), Arg::Nil);
        assert_eq!(read_arg("12"), Arg::Int(12));
        assert_eq!(read_arg(" 12"), Arg::Int(12), ".NET trims numbers");
        assert_eq!(read_arg("1.5"), Arg::Float(1.5));
        assert_eq!(
            read_arg("a.b"),
            Arg::Text("a.b".into()),
            "a . but no number"
        );
        assert_eq!(read_arg("TRUE"), Arg::Bool(true));
        assert_eq!(read_arg(" true"), Arg::Text(" true".into()));
        assert_eq!(read_arg("'x'"), Arg::Text("x".into()));
        assert_eq!(
            read_arg("\"12\""),
            Arg::Text("12".into()),
            "quoted stays text"
        );
        assert_eq!(read_arg(" 'x'"), Arg::Text(" 'x".into()));
        assert_eq!(read_arg("hello"), Arg::Text("hello".into()));
    }

    #[test]
    fn splitting_calls() {
        let (call, problems) = parse_call("F");
        assert_eq!(call.unwrap().args, vec![]);
        assert!(problems.is_empty());

        let (call, problems) = parse_call("Ep_Go(1,'a',true)");
        let call = call.unwrap();
        assert_eq!(call.name, "Ep_Go");
        assert_eq!(
            call.args,
            vec![Arg::Int(1), Arg::Text("a".into()), Arg::Bool(true)]
        );
        assert!(problems.is_empty());
        assert!(parse_call("F( )").1.is_empty(), "spaces only: no arguments");
        assert!(parse_call("F(1").1.is_empty(), "the ) is optional");

        let (call, problems) = parse_call("F(1,,2)");
        assert!(call.is_none());
        assert_eq!(problems[0].severity, Severity::Error);
        assert!(parse_call("F(1,)").0.is_none());

        let problems = parse_call("F(1, true)").1;
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].severity, Severity::Warning);
        assert!(parse_call("F(1, 2)").1.is_empty(), "same value either way");

        for bad in ["My.Fn", "My:Fn()", "F (1)", "1F", "end", "F(g(1))", " "] {
            assert!(
                parse_call(bad)
                    .1
                    .iter()
                    .any(|p| p.severity == Severity::Error),
                "{bad}"
            );
        }
        assert_eq!(parse_call("F(1) x").1[0].severity, Severity::Warning);
    }

    #[test]
    fn reading_lua() {
        let source = r#"
            -- function Commented(a)
            --[[ function Block(a) ]]
            local s = "function InString(a)"
            local t = [==[ function InLong(a) ]==]
            function Global(marker, isFocused, extra) end
            local function Hidden(marker) end
            Assigned = function(marker, ...) end
            local LocalAssigned = function(m) end
            T = { Field = function(m) end, other = 1, Second = function() end }
            function T.Member(m) end
            function T:Method(m) end
            function _G.Through(m) end
            _G.Also = function(m) end
            Pack:Require("Data/Ep/Scripts/a.lua")
            Pack:Require 'b'
        "#;
        let (functions, requires) = read_lua(source);
        let names: Vec<(&str, bool)> = functions
            .iter()
            .map(|f| (f.name.as_str(), f.local))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Global", false),
                ("Hidden", true),
                ("Assigned", false),
                ("LocalAssigned", true),
                ("Through", false),
                ("Also", false),
            ]
        );
        assert_eq!(functions[0].params, vec!["marker", "isFocused", "extra"]);
        assert_eq!(functions[0].line, 6);
        assert!(functions[2].vararg);
        assert_eq!(
            requires,
            vec![
                ("Data/Ep/Scripts/a.lua".to_owned(), 15),
                ("b".to_owned(), 16)
            ]
        );
    }

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "pathing_editor_lua_{}",
            crate::guid::new_guid().replace(['/', '+', '='], "")
        ))
    }

    #[test]
    fn scanning_and_checking() {
        let dir = temp_dir();
        let scan = LuaScan::scan(&dir);
        assert!(!scan.has_entry());
        assert!(
            check(hook_of("script-trigger"), "Go", &scan)[0]
                .text
                .contains("no pack.lua")
        );

        let scripts = dir.join("Data").join("Scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(
            dir.join("Pack.lua"),
            "Ep = {}\nPack:Require(\"data/scripts/Main\")\nPack:Require(\"data/scripts/gone.lua\")\n",
        )
        .unwrap();
        fs::write(
            scripts.join("Main.lua"),
            "function Go(marker, auto, count)\nend\nlocal function Quiet(m) end\nfunction Filter(m) end\nfunction go2(m) end\n",
        )
        .unwrap();
        fs::write(scripts.join("Unused.lua"), "function Orphan(m) end\n").unwrap();
        fs::write(scripts.join("Again.lua"), "x = 1").unwrap();

        let scan = LuaScan::scan(&dir);
        assert!(scan.has_entry());
        let order: Vec<(&str, Option<usize>)> = scan
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.load_order))
            .collect();
        assert_eq!(
            order,
            vec![
                ("Pack.lua", Some(0)),
                ("Data/Scripts/Main.lua", Some(1)),
                ("Data/Scripts/Again.lua", None),
                ("Data/Scripts/Unused.lua", None),
            ]
        );
        assert_eq!(scan.problems.len(), 1, "{:?}", scan.problems);
        assert!(scan.problems[0].text.contains("gone.lua"));

        let trigger = hook_of("script-trigger");
        assert!(check(trigger, "Go(3)", &scan).is_empty());
        let nil = check(trigger, "Go", &scan);
        assert_eq!(nil.len(), 1);
        assert!(nil[0].text.contains("count will be nil"), "{nil:?}");
        let extra = check(trigger, "Go(1,2)", &scan);
        assert!(extra[0].text.contains("dropped"), "{extra:?}");
        assert!(
            check(trigger, "GO(3)", &scan)[0]
                .text
                .contains("spelled Go")
        );
        assert!(check(trigger, "Quiet", &scan)[0].text.contains("local"));
        assert!(
            check(trigger, "Orphan", &scan)[0]
                .text
                .contains("never loads")
        );
        assert!(
            check(trigger, "Nope", &scan)[0]
                .text
                .contains("No global function")
        );
        assert!(check(hook_of("script-filter"), "Filter", &scan).is_empty());

        let mut pack = Pack::default();
        let mut poi = Poi::new(15, [0.0; 3], "ep.cat");
        attrs::set(&mut poi.attrs, "script-trigger", "Nope");
        pack.pois.push(poi);
        let report = pack_report(&pack, &scan);
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].item, Selection::poi(0));
        assert_eq!(report[0].problems[0].severity, Severity::Error);
        fs::remove_dir_all(&dir).unwrap();
    }
}

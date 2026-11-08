use super::{AppliedRename, RenameKind, SceneReference};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Edits and references that could not be tied to a script in this scene.
pub struct ScopedSceneRenames {
    pub source: String,
    pub applied: Vec<SceneReference>,
    pub unresolved: Vec<SceneReference>,
    pub conflicted: bool,
}

/// Rename connection endpoints only when their node has the declaring script
/// attached directly. Inherited, instanced and built-in scripts are unresolved.
pub fn apply_scoped_scene_renames(
    source: &str,
    path: &Path,
    renames: &[AppliedRename],
) -> ScopedSceneRenames {
    let scene = Scene::read(source);
    let project = path
        .ancestors()
        .skip(1)
        .find(|dir| dir.join("project.godot").is_file());
    let script_paths: HashMap<_, _> = scene
        .scripts
        .iter()
        .filter_map(|(id, path)| {
            let relative = path.strip_prefix("res://")?;
            let full = project?.join(relative).canonicalize().ok()?;
            Some((*id, full))
        })
        .collect();
    let declaring_paths: HashMap<&str, PathBuf> = renames
        .iter()
        .filter_map(|rename| {
            Some((
                rename.source_file.as_str(),
                Path::new(&rename.source_file).canonicalize().ok()?,
            ))
        })
        .collect();
    scene.rewrite(source, renames, |row, endpoint, rename| {
        let node = row.value(endpoint)?;
        let script = script_paths.get(scene.nodes.get(node)?)?;
        Some(declaring_paths.get(rename.source_file.as_str())? == script)
    })
}

/// Unscoped library entry point: callers supply the relevant renames.
pub(super) fn rewrite_connections(source: &str, renames: &[AppliedRename]) -> ScopedSceneRenames {
    Scene::read(source).rewrite(source, renames, |_, _, _| Some(true))
}

#[derive(Default)]
struct Scene<'a> {
    scripts: HashMap<&'a str, &'a str>,
    nodes: HashMap<String, &'a str>,
    connections: Vec<(usize, usize, Header<'a>)>,
    conflicted: bool,
}

impl<'a> Scene<'a> {
    fn read(source: &'a str) -> Self {
        let mut scene = Self::default();
        let mut node = None;
        let mut offset = 0;
        let mut in_string = false;
        for (line_no, line) in source.split_inclusive('\n').enumerate() {
            let start = offset;
            offset += line.len();
            if !in_string && crate::lexer::may_hold_conflict_marker(line) {
                scene.conflicted = true;
                continue;
            }
            let starts_in_string = in_string;
            in_string = string_state_after(line, in_string);
            if starts_in_string {
                continue;
            }

            let trimmed = line.trim_start_matches([' ', '\t', '\u{feff}']);
            if trimmed.starts_with('[') {
                node = None;
                let Some(row) = Header::parse(trimmed) else {
                    continue;
                };
                match row.kind {
                    "ext_resource" => {
                        if let (Some(id), Some(path)) = (row.value("id"), row.value("path")) {
                            scene.scripts.insert(id, path);
                        }
                    }
                    "node" => {
                        if let Some(name) = row.value("name") {
                            node = Some(match row.value("parent") {
                                None => ".".to_owned(),
                                Some(".") => name.to_owned(),
                                Some(parent) => format!("{parent}/{name}"),
                            });
                        }
                    }
                    "connection" => scene.connections.push((
                        line_no + 1,
                        start + line.len() - trimmed.len(),
                        row,
                    )),
                    _ => {}
                }
            } else if let Some(path) = node.as_ref() {
                if let Some(id) = script_property(trimmed) {
                    scene.nodes.insert(path.clone(), id);
                }
            }
        }
        scene
    }

    fn rewrite(
        &self,
        source: &str,
        renames: &[AppliedRename],
        ownership: impl Fn(&Header<'a>, &str, &AppliedRename) -> Option<bool>,
    ) -> ScopedSceneRenames {
        let mut edits = Vec::new();
        let mut unresolved = Vec::new();
        for (line, base, row) in &self.connections {
            for (attribute, endpoint, kind) in [
                ("signal", "from", RenameKind::Signal),
                ("method", "to", RenameKind::Function),
            ] {
                let Some(range) = row.attributes.get(attribute) else {
                    continue;
                };
                let name = &row.text[range.clone()];
                let mut resolved = None;
                let mut uncertain = None;
                let mut ambiguous = false;
                for rename in renames
                    .iter()
                    .filter(|r| r.kind == kind && r.old_name == name)
                {
                    match ownership(row, endpoint, rename) {
                        Some(true) => {
                            ambiguous |= resolved.is_some_and(|previous: &AppliedRename| {
                                previous.new_name != rename.new_name
                            });
                            resolved = Some(rename);
                        }
                        None => uncertain = Some(rename),
                        Some(false) => {}
                    }
                }
                if let Some(rename) = resolved.or(uncertain) {
                    let reference = SceneReference {
                        line: *line,
                        attribute,
                        old_name: name.to_owned(),
                        new_name: rename.new_name.clone(),
                    };
                    if resolved.is_none() || ambiguous || self.conflicted {
                        unresolved.push(reference);
                    } else {
                        edits.push((base + range.start..base + range.end, reference));
                    }
                }
            }
        }
        edits.sort_unstable_by_key(|(range, _)| range.start);
        let mut rewritten = String::with_capacity(source.len());
        let mut cursor = 0;
        let mut applied = Vec::new();
        for (range, reference) in edits {
            rewritten.push_str(&source[cursor..range.start]);
            rewritten.push_str(&reference.new_name);
            cursor = range.end;
            applied.push(reference);
        }
        rewritten.push_str(&source[cursor..]);
        ScopedSceneRenames {
            source: rewritten,
            applied,
            unresolved,
            conflicted: self.conflicted,
        }
    }
}

fn script_property(line: &str) -> Option<&str> {
    let (key, value) = line.split_once('=')?;
    if key.trim() != "script" {
        return None;
    }
    let id = value
        .trim()
        .strip_prefix("ExtResource(")?
        .strip_suffix(')')?
        .trim();
    id.strip_prefix('"')?.strip_suffix('"')
}

/// Keep track of multiline strings so property text cannot masquerade as a
/// section header or a conflict marker. Semicolon comments are inert.
fn string_state_after(line: &str, mut in_string: bool) -> bool {
    let mut chars = line.bytes();
    while let Some(ch) = chars.next() {
        match ch {
            b'"' => in_string = !in_string,
            b'\\' if in_string => {
                chars.next();
            }
            b';' if !in_string => break,
            _ => {}
        }
    }
    in_string
}

struct Header<'a> {
    kind: &'a str,
    text: &'a str,
    attributes: HashMap<&'a str, Range<usize>>,
}

impl<'a> Header<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let text = text.trim_end();
        let bytes = text.as_bytes();
        let mut at = 1;
        while bytes
            .get(at)
            .is_some_and(|b| !b.is_ascii_whitespace() && *b != b']')
        {
            at += 1;
        }
        let kind = text.get(1..at)?;
        let mut attributes = HashMap::new();
        loop {
            skip_space(bytes, &mut at);
            if *bytes.get(at)? == b']' {
                break;
            }
            let start = at;
            while bytes
                .get(at)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                at += 1;
            }
            let key = text.get(start..at)?;
            skip_space(bytes, &mut at);
            if *bytes.get(at)? != b'=' || key.is_empty() {
                return None;
            }
            at += 1;
            skip_space(bytes, &mut at);
            if *bytes.get(at)? == b'"' {
                let start = at + 1;
                at = end_quote(bytes, start)?;
                if attributes.insert(key, start..at).is_some() {
                    return None;
                }
                at += 1;
            } else {
                at = skip_value(bytes, at)?;
            }
        }
        Some(Self {
            kind,
            text,
            attributes,
        })
    }

    fn value(&self, key: &str) -> Option<&'a str> {
        Some(&self.text[self.attributes.get(key)?.clone()])
    }
}

fn skip_space(bytes: &[u8], at: &mut usize) {
    while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
}

fn end_quote(bytes: &[u8], mut at: usize) -> Option<usize> {
    loop {
        match *bytes.get(at)? {
            b'"' => return Some(at),
            b'\\' => at += 2,
            _ => at += 1,
        }
    }
}

fn skip_value(bytes: &[u8], mut at: usize) -> Option<usize> {
    let mut depth = 0usize;
    loop {
        match *bytes.get(at)? {
            b']' if depth == 0 => return Some(at),
            ch if ch.is_ascii_whitespace() && depth == 0 => return Some(at),
            b'[' | b'(' | b'{' => depth += 1,
            b']' | b')' | b'}' => depth = depth.checked_sub(1)?,
            b'"' => at = end_quote(bytes, at + 1)?,
            _ => {}
        }
        at += 1;
    }
}

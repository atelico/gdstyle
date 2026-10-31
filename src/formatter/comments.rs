//! Wrapping long comment lines for `fmt`, as set by `comment_wrap`.
//!
//! Both wrapping modes share [`wrap_long_comment`]. `"line"` rewraps the long
//! line alone. `"paragraph"` also pulls in the comment lines after it, up to
//! the first line [`ends_prose`] says can't continue it. Either way, a long
//! line [`must_keep_whole`] says is code, a table, a URL or a `gdstyle:`
//! directive is left as it is, and so are doc-comment `[codeblock]` contents.

use super::{visual_line_len, TAB_WIDTH};

/// Whether a doc comment `[codeblock]` is open after `line`, given whether one
/// was open before it.
///
/// A tag counts only at the start of a `##` line, as Godot's docs write them,
/// so prose that mentions `[codeblock]` opens nothing. The variants
/// `[codeblocks]` and `[codeblock lang=...]` count too. A block opened and
/// closed on one line leaves nothing open, and any line that is not a doc
/// comment ends the block.
pub(super) fn code_block_state_after(line: &str, is_open: bool) -> bool {
    let Some(text) = line.trim_start().strip_prefix("##") else {
        return false;
    };
    let text = text.trim();
    if text.contains("[/codeblock") {
        false
    } else {
        is_open || (text.starts_with("[codeblock") && text.ends_with(']'))
    }
}

/// Rewrap the long comment line `lines[start]`. With `refill_paragraph`, the
/// rest of its paragraph is refilled too (see `CommentWrap::Paragraph`).
/// Returns the new lines and how many source lines they replace, or `None`
/// to leave the line as it is.
///
/// A list item's continuation lines are indented under its text, so a
/// following line belongs to the item when it has that indent.
pub(super) fn wrap_long_comment(
    lines: &[&str],
    start: usize,
    max_len: usize,
    refill_paragraph: bool,
    is_editable: &dyn Fn(usize) -> bool,
) -> Option<(Vec<String>, usize)> {
    let first = split_comment_line(lines[start])?;
    if first.text.trim().is_empty() || must_keep_whole(first.text) {
        return None;
    }
    let continuation_prefix = match list_marker_len(first.text) {
        Some(marker_len) => format!("{}{}", first.prefix, " ".repeat(marker_len)),
        None => first.prefix.to_string(),
    };

    let mut words: Vec<&str> = first.text.split_whitespace().collect();
    let mut end = start + 1;
    while refill_paragraph && end < lines.len() && is_editable(end) {
        let Some(text) = lines[end].strip_prefix(continuation_prefix.as_str()) else {
            break;
        };
        if !continues_paragraph(text.trim_end()) {
            break;
        }
        words.extend(text.split_whitespace());
        end += 1;
    }

    let wrapped = wrap_comment_words(&words, first.prefix, &continuation_prefix, max_len);
    let consumed = end - start;
    if consumed == 1 && wrapped.len() < 2 {
        return None;
    }
    Some((wrapped, consumed))
}

/// True when `text`, what follows the paragraph's prefix on a later line,
/// is more of the same paragraph. Anything that starts something else ends
/// it: an empty comment line, a deeper indent, a list item or tag, code, a
/// table, or a `[codeblock]` tag.
fn continues_paragraph(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with([' ', '\t', '#'])
        && !starts_own_paragraph(text)
        && !ends_prose(text)
        && !text.contains("[codeblock")
        && !text.contains("[/codeblock")
}

/// A `#` or `##` comment line split into its parts: the prefix (indent,
/// marker and every space after it) and the text.
struct CommentLine<'a> {
    prefix: &'a str,
    text: &'a str,
}

/// Split `line` into a [`CommentLine`]. `None` for anything but a plain
/// comment: code, `#!`, `#region`, or a comment with no space after the
/// marker, none of which are wrapped. The spaces after the marker belong to
/// the prefix, so a nested list item (`#   - item`) keeps its indent.
fn split_comment_line(line: &str) -> Option<CommentLine<'_>> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let content = &line[indent_len..];
    let marker_len = if content.starts_with("##") { 2 } else { 1 };
    if !content.starts_with('#') || content.as_bytes().get(marker_len) != Some(&b' ') {
        return None;
    }
    let after_marker = &content[marker_len..];
    let spaces_len = after_marker.len() - after_marker.trim_start_matches(' ').len();
    let prefix_len = indent_len + marker_len + spaces_len;
    Some(CommentLine {
        prefix: &line[..prefix_len],
        text: &line[prefix_len..],
    })
}

/// Byte length of the list marker and its space at the start of comment
/// text: `- `, `* `, `+ `, `1. ` or `1) `.
fn list_marker_len(text: &str) -> Option<usize> {
    if text.starts_with("- ") || text.starts_with("* ") || text.starts_with("+ ") {
        return Some(2);
    }
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let after_digits = text.as_bytes().get(digits..digits + 2)?;
    (digits > 0 && matches!(after_digits, [b'.' | b')', b' '])).then_some(digits + 2)
}

/// True when comment text starts a paragraph of its own: a list item or a
/// tag like `TODO:`.
fn starts_own_paragraph(text: &str) -> bool {
    const TAGS: [&str; 6] = ["TODO", "FIXME", "NOTE", "HACK", "XXX", "BUG"];
    list_marker_len(text).is_some()
        || TAGS.iter().any(|tag| {
            text.strip_prefix(tag)
                .is_some_and(|rest| rest.starts_with([':', '(', ' ']))
        })
}

/// True when a long comment line must not be wrapped: commented-out code, a
/// table row, a URL (which can't be split), or a `gdstyle:` directive.
///
/// A false hit leaves prose long, where `line` mode used to wrap it, so only
/// unambiguous signs count. The main one is that code has no two plain
/// words in a row: `x = y means that` and `if you need this:` are prose,
/// `speed += 1` and `if is_ready:` are not. [`ends_prose`] adds the looser
/// signs that only end a paragraph.
fn must_keep_whole(text: &str) -> bool {
    const DECLARATIONS: [&str; 8] = [
        "var ",
        "const ",
        "func ",
        "signal ",
        "enum ",
        "class ",
        "extends ",
        "class_name ",
    ];
    const BLOCK_KEYWORDS: [&str; 4] = ["if ", "elif ", "while ", "match "];
    let text = text.trim();
    if text.contains("://") || text.starts_with("gdstyle:") || column_gap_count(text) >= 2 {
        return true;
    }
    if text.starts_with('@') || DECLARATIONS.iter().any(|keyword| text.starts_with(keyword)) {
        return true;
    }
    if matches!(text, "pass" | "break" | "continue" | "return" | "else:") || is_for_header(text) {
        return true;
    }
    if let Some(header) = text.strip_suffix(':') {
        let condition = BLOCK_KEYWORDS
            .iter()
            .find_map(|keyword| header.strip_prefix(keyword));
        if condition.is_some_and(|condition| !has_prose_words(condition)) {
            return true;
        }
    }
    // `return x`, `await get_tree().process_frame`.
    for keyword in ["return ", "await "] {
        if let Some(expression) = text.strip_prefix(keyword) {
            return !has_prose_words(expression);
        }
    }
    let target_len = statement_target_len(text);
    if target_len == 0 {
        return false;
    }
    let after_target = &text[target_len..];
    // `queue_free()`, `print("hi", x)`: no space before the call's `(`.
    let is_call = after_target.starts_with('(') && text.ends_with(')');
    let is_assignment = ["= ", "+= ", "-= ", "*= ", "/= ", ":= "]
        .iter()
        .find_map(|operator| after_target.trim_start().strip_prefix(operator))
        .is_some_and(|value| !has_prose_words(value));
    is_call || is_assignment
}

/// True when comment text can't continue a prose paragraph: anything
/// [`must_keep_whole`] keeps, plus looser signs of code or a table that are
/// wrong to merge into prose but too weak to leave a long line unwrapped: a
/// single column gap, a trailing `;`, a block header, or `return` / `await`
/// with a one-word or bracketed expression.
fn ends_prose(text: &str) -> bool {
    const BLOCK_KEYWORDS: [&str; 6] = ["if ", "elif ", "else", "while ", "match ", "static func "];
    let text = text.trim();
    if must_keep_whole(text) || column_gap_count(text) > 0 || text.ends_with(';') {
        return true;
    }
    if text.ends_with(':')
        && BLOCK_KEYWORDS
            .iter()
            .any(|keyword| text.starts_with(keyword))
    {
        return true;
    }
    ["return ", "await "].iter().any(|keyword| {
        text.strip_prefix(keyword)
            .is_some_and(|expression| !expression.contains(' ') || expression.ends_with([')', ']']))
    })
}

/// True when `text` has two plain words in a row (`means that`, `you
/// need`), which prose nearly always has and code nearly never does.
fn has_prose_words(text: &str) -> bool {
    // A word may end a clause or sentence: `value.`, `this,`.
    let is_word = |token: &str| {
        let word = token.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        !word.is_empty() && word.chars().all(|c| c.is_alphabetic() || c == '\'')
    };
    let tokens: Vec<&str> = text.split_whitespace().collect();
    tokens
        .windows(2)
        .any(|pair| is_word(pair[0]) && is_word(pair[1]))
}

/// True for a `for` loop header: `for item in items:` or, with a typed loop
/// variable, `for i: int in range(3):`. Prose like "for the given filter in
/// this order:" has more than a name before ` in `.
fn is_for_header(text: &str) -> bool {
    if !text.ends_with(':') {
        return false;
    }
    let Some((variable, _)) = text
        .strip_prefix("for ")
        .and_then(|rest| rest.split_once(" in "))
    else {
        return false;
    };
    let name = variable.split_once(':').map_or(variable, |(name, _)| name);
    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Byte length of the name an assignment or call statement starts with:
/// dotted names, node paths (`$HUD/Label`, `%Panel`) and subscripts
/// (`cache[key]`, `items[i].visible`). Zero when `text` starts with
/// anything else.
fn statement_target_len(text: &str) -> usize {
    let is_name_char = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '%' | '/');
    let mut bracket_depth = 0usize;
    for (index, c) in text.char_indices() {
        match c {
            '[' if index > 0 => bracket_depth += 1,
            ']' if bracket_depth > 0 => bracket_depth -= 1,
            _ if bracket_depth > 0 => {}
            _ if is_name_char(c) => {}
            _ => return index,
        }
    }
    if bracket_depth > 0 {
        0
    } else {
        text.len()
    }
}

/// How many gaps `text` has that aren't the double space some writers put
/// after a sentence: tabs, and runs of spaces between the columns of a table
/// (`speed   float   1.0`).
fn column_gap_count(text: &str) -> usize {
    let tabs = text.matches('\t').count();
    let space_runs = text
        .match_indices("  ")
        .filter(|&(index, _)| {
            // Count each run once, at its first two spaces.
            !text[..index].ends_with([' ', '.', '!', '?', ':'])
        })
        .count();
    tabs + space_runs
}

/// Lay `words` out as comment lines of at most `max_len` columns. The first
/// line starts with `first_prefix` (indent and `# `), the others with
/// `continuation_prefix`. A word wider than a line gets a line of its own.
///
/// A wrapped line never starts with a `gdstyle:` word: `# gdstyle:ignore`
/// at the start of a comment is a suppression directive, so the word before
/// it moves down too, or it stays on the line it was on.
fn wrap_comment_words(
    words: &[&str],
    first_prefix: &str,
    continuation_prefix: &str,
    max_len: usize,
) -> Vec<String> {
    // Columns, not bytes: an accented word takes one column per char.
    let line_width = |prefix: &str, line_words: &[&str]| {
        let words_width: usize = line_words.iter().map(|word| word.chars().count()).sum();
        visual_line_len(prefix, TAB_WIDTH) + words_width + line_words.len().saturating_sub(1)
    };
    let mut lines_out: Vec<String> = Vec::new();
    let mut prefix = first_prefix;
    let mut line_words: Vec<&str> = Vec::new();

    for &word in words {
        let fits = line_width(prefix, &line_words) + 1 + word.chars().count() <= max_len;
        let is_directive = word.starts_with("gdstyle:");
        if line_words.is_empty() || fits || (is_directive && line_words.len() == 1) {
            line_words.push(word);
            continue;
        }
        let carried = if is_directive { line_words.pop() } else { None };
        lines_out.push(format!("{}{}", prefix, line_words.join(" ")));
        prefix = continuation_prefix;
        line_words = carried.into_iter().chain([word]).collect();
    }
    lines_out.push(format!("{}{}", prefix, line_words.join(" ")));
    lines_out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_counts_columns_not_bytes() {
        // Each word is 8 columns but 16 bytes. The comment fills exactly 100
        // columns before the last word; measuring bytes wrapped it at 50.
        let word = "é".repeat(8);
        let words = [word.as_str(); 12];
        let lines = wrap_comment_words(&words, "# ", "# ", 100);
        assert_eq!(
            lines
                .iter()
                .map(|line| line.chars().count())
                .collect::<Vec<_>>(),
            vec![100, 10]
        );
    }

    #[test]
    fn code_blocks_open_and_close_only_on_their_own_doc_comment_lines() {
        assert!(code_block_state_after("## [codeblock]", false));
        assert!(code_block_state_after(
            "\t## [codeblock lang=gdscript]",
            false
        ));
        assert!(!code_block_state_after(
            "## Wrap examples in [codeblock] tags.",
            false
        ));
        assert!(!code_block_state_after(
            "## [codeblock]print(1)[/codeblock]",
            false
        ));
        assert!(code_block_state_after("## var x = 1", true));
        assert!(!code_block_state_after("## [/codeblock]", true));
        assert!(
            !code_block_state_after("var x = 1", true),
            "code ends the block"
        );
    }

    #[test]
    fn statement_targets_cover_names_paths_and_subscripts() {
        assert_eq!(statement_target_len("cache[key] = 1"), "cache[key]".len());
        assert_eq!(
            statement_target_len("items[i].visible = false"),
            "items[i].visible".len()
        );
        assert_eq!(
            statement_target_len("$HUD/Label.text = x"),
            "$HUD/Label.text".len()
        );
        assert_eq!(statement_target_len("[1] see above"), 0);
        assert_eq!(statement_target_len("open[ bracket"), 0);
    }
}

//! `comment_wrap`: how `fmt` handles a comment line over `max_line_length`.
//!
//! The width is 40 here so the inputs stay readable.

use gdstyle::config::{CommentWrap, Config};
use gdstyle::formatter::format_source;
use gdstyle::linter;

fn config(comment_wrap: CommentWrap) -> Config {
    Config {
        max_line_length: 40,
        comment_wrap,
        ..Config::default()
    }
}

/// Format with `mode` and check that a second run changes nothing.
fn format(source: &str, mode: CommentWrap) -> String {
    let formatted = format_source(source, &config(mode));
    assert_eq!(
        format_source(&formatted, &config(mode)),
        formatted,
        "not idempotent"
    );
    formatted
}

// The paragraph from issue #85, scaled down: the first line runs past the
// limit and the second line goes on with the same sentence.
const PARAGRAPH: &str = "\
# aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk
# lll mmm.
";

#[test]
fn line_is_the_default_and_wraps_only_the_long_line() {
    assert_eq!(Config::default().comment_wrap, CommentWrap::Line);
    assert_eq!(
        format(PARAGRAPH, CommentWrap::Line),
        "\
# aaa bbb ccc ddd eee fff ggg hhh iii
# jjj kkk
# lll mmm.
"
    );
}

#[test]
fn paragraph_refills_the_lines_after_the_long_one() {
    assert_eq!(
        format(PARAGRAPH, CommentWrap::Paragraph),
        "\
# aaa bbb ccc ddd eee fff ggg hhh iii
# jjj kkk lll mmm.
"
    );
}

#[test]
fn paragraph_leaves_the_lines_above_the_long_one_alone() {
    let source = "\
# zzz
# aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk
";
    assert_eq!(
        format(source, CommentWrap::Paragraph),
        "\
# zzz
# aaa bbb ccc ddd eee fff ggg hhh iii
# jjj kkk
"
    );
}

#[test]
fn off_leaves_long_comments_alone_and_check_still_reports_them() {
    assert_eq!(format(PARAGRAPH, CommentWrap::Off), PARAGRAPH);
    let lines: Vec<usize> = linter::lint_source(PARAGRAPH, "test.gd", &config(CommentWrap::Off))
        .iter()
        .filter(|d| d.rule == "format/max-line-length")
        .map(|d| d.span.line)
        .collect();
    assert_eq!(lines, vec![1]);
}

#[test]
fn off_still_wraps_long_code() {
    let source = "func f() -> void:\n\tprint(aaaaaaaa, bbbbbbbb, cccccccc, dddddddd)\n";
    let formatted = format(source, CommentWrap::Off);
    assert_ne!(
        formatted, source,
        "code wrapping is not part of comment_wrap"
    );
}

/// Format `long + "\n" + next` in paragraph mode and return the non-blank
/// lines. Blank lines come from member spacing (a `##` line at script level
/// gets one above it), not from wrapping.
fn paragraph_lines(long: &str, next: &str) -> Vec<String> {
    let source = format!("{long}\n{next}\n");
    format(&source, CommentWrap::Paragraph)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

const LONG: &str = "# aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk";

#[test]
fn paragraph_stops_before_lines_that_start_something_else() {
    // Each next line keeps its own line: the overflow `jjj kkk` is not
    // merged into it.
    for next in [
        "#",                  // empty comment line: paragraph break
        "## doc text",        // different marker
        "\t# indented",       // different indent
        "#   indented block", // deeper text indent
        "# - list item",      // list items
        "# * list item",
        "# 1. list item",
        "# TODO: something",     // tags start their own paragraph
        "# var speed: int = 10", // commented-out code
        "# if is_ready:",
        "# speed += 1",
        "# queue_free()",
        "# print(\"hello\", x)",
        "# $Timer.start(0.5)",
        "# return x",
        "# await get_tree().process_frame",
        "# name    type    default",
        "#     var indented = 1",
        "# see https://godotengine.org",
        "# gdstyle:ignore format/max-line-length",
    ] {
        let lines = paragraph_lines(LONG, next);
        assert_eq!(
            lines,
            vec!["# aaa bbb ccc ddd eee fff ggg hhh iii", "# jjj kkk", next],
            "next line {next:?}"
        );
    }
}

#[test]
fn paragraph_does_not_wrap_a_long_line_of_code_a_url_or_a_directive() {
    for long in [
        "# var speed: float = clampf(speed, minimum, maximum)",
        "# see https://docs.godotengine.org/en/stable/index.html",
        "# gdstyle:ignore format/max-line-length format/comment-spacing",
    ] {
        let source = format!("{long}\n");
        assert_eq!(format(&source, CommentWrap::Paragraph), source, "{long:?}");
    }
}

#[test]
fn paragraph_wraps_a_list_item_under_its_text() {
    let source = "\
# - aaa bbb ccc ddd eee fff ggg hhh iii jjj
#   kkk.
# - next
";
    assert_eq!(
        format(source, CommentWrap::Paragraph),
        "\
# - aaa bbb ccc ddd eee fff ggg hhh iii
#   jjj kkk.
# - next
"
    );
}

#[test]
fn paragraph_keeps_doc_comment_code_blocks_as_written() {
    let source = "\
## Example:
## [codeblock]
## var aaa = bbb + ccc + ddd + eee + fff + ggg
## [/codeblock]
";
    assert_eq!(format(source, CommentWrap::Paragraph), source);
}

#[test]
fn paragraph_refills_doc_comments_and_indented_comments() {
    let source = "\
func f() -> void:
\t## aaa bbb ccc ddd eee fff ggg hhh iii
\t## jjj.
\tpass
";
    assert_eq!(
        format(source, CommentWrap::Paragraph),
        "\
func f() -> void:
\t## aaa bbb ccc ddd eee fff ggg hhh
\t## iii jjj.
\tpass
"
    );
}

#[test]
fn comment_wrap_reads_from_toml() {
    let parse = |text: &str| toml::from_str::<Config>(text).map(|config| config.comment_wrap);
    assert_eq!(parse("comment_wrap = \"line\"").unwrap(), CommentWrap::Line);
    assert_eq!(
        parse("comment_wrap = \"paragraph\"").unwrap(),
        CommentWrap::Paragraph
    );
    assert_eq!(parse("comment_wrap = \"off\"").unwrap(), CommentWrap::Off);
    assert!(parse("comment_wrap = \"reflow\"").is_err());
}

#[test]
fn paragraph_still_refills_prose_that_looks_a_little_like_code() {
    // "return the value" is prose: more than one word, no bracket at the end.
    // A double space after a sentence is not a table column.
    for (next, refilled) in [
        ("# return the value.", "# jjj kkk return the value."),
        ("# end.  Then more.", "# jjj kkk end. Then more."),
    ] {
        assert_eq!(
            paragraph_lines(LONG, next),
            vec!["# aaa bbb ccc ddd eee fff ggg hhh iii", refilled],
            "next line {next:?}"
        );
    }
}

#[test]
fn paragraph_wraps_after_prose_that_mentions_a_codeblock_tag() {
    // Only a tag on its own line opens a code block, so a later long comment
    // still wraps.
    let source = "\
## Wrap examples in [codeblock] tags.
func f() -> void:
\t# aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk
\tpass
";
    let formatted = format(source, CommentWrap::Paragraph);
    assert!(
        formatted.contains("\t# aaa bbb ccc ddd eee fff ggg hhh\n\t# iii jjj kkk\n"),
        "{formatted}"
    );
}

#[test]
fn paragraph_wraps_a_nested_list_item_under_its_text() {
    let source = "\
# - item
#   - nested sub item aaa bbb ccc ddd eee
";
    assert_eq!(
        format(source, CommentWrap::Paragraph),
        "\
# - item
#   - nested sub item aaa bbb ccc ddd
#     eee
"
    );
}

#[test]
fn wrapping_never_starts_a_line_with_a_suppression_directive() {
    // `# gdstyle:ignore-file` at the start of a comment would silence the
    // whole file, so the word before it moves down with it.
    for mode in [CommentWrap::Line, CommentWrap::Paragraph] {
        let source = "# aaa bbb ccc ddd eee fff ggg by gdstyle:ignore-file x\nvar badName = 1\n";
        let formatted = format(source, mode);
        assert!(
            formatted
                .lines()
                .all(|line| !line.starts_with("# gdstyle:")),
            "{mode:?}: {formatted}"
        );
        let names: Vec<String> = linter::lint_source(&formatted, "test.gd", &config(mode))
            .into_iter()
            .map(|d| d.rule)
            .collect();
        assert!(
            names.contains(&"naming/variable-name-snake-case".to_string()),
            "{mode:?}: {names:?}"
        );
    }
}

#[test]
fn line_mode_keeps_directives_code_blocks_and_commented_code_whole() {
    // Splitting a directive moved its target, wrapping a code block changed
    // the documented code, and a commented-out call was split at its commas
    // like real code.
    for source in [
        "# gdstyle:ignore=naming/variable-name-snake-case legacy save name\nvar BadName: int = 5\n",
        "## Example:\n## [codeblock]\n## var aaa = bbb + ccc + ddd + eee + fff\n## [/codeblock]\nvar a: int = 1\n",
        "# print(aaaaaaaa, bbbbbbbb, cccccccc, dddddddd)\n",
    ] {
        assert_eq!(format(source, CommentWrap::Line), source);
    }
}

#[test]
fn line_mode_wraps_a_nested_list_item_under_its_text() {
    let source = "# - item\n#   - nested sub item aaa bbb ccc ddd eee\n";
    assert_eq!(
        format(source, CommentWrap::Line),
        "# - item\n#   - nested sub item aaa bbb ccc ddd\n#     eee\n"
    );
}

#[test]
fn paragraph_wraps_after_a_one_line_code_block() {
    let source = "\
## [codeblock]print(1)[/codeblock]
func f() -> void:
\t# aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk
\tpass
";
    let formatted = format(source, CommentWrap::Paragraph);
    assert!(
        formatted.contains("\t# aaa bbb ccc ddd eee fff ggg hhh\n\t# iii jjj kkk\n"),
        "{formatted}"
    );
}

#[test]
fn paragraph_stops_at_subscripts_and_tab_tables_but_not_at_prose_for() {
    for (next, is_merged) in [
        ("# cache[key] = compute(key)", false),
        ("# items[i].visible = false", false),
        ("# name\ttype\tdefault", false),
        ("# for the given filter in this order:", true),
    ] {
        // Merged when the next line's first word joins the overflow `kkk`.
        let first_word = next
            .trim_start_matches("# ")
            .split_whitespace()
            .next()
            .unwrap();
        let lines = paragraph_lines(LONG, next);
        let joined = lines
            .iter()
            .any(|line| line.contains(&format!("kkk {first_word}")));
        assert_eq!(joined, is_merged, "next line {next:?}: {lines:?}");
    }
}

#[test]
fn line_mode_still_wraps_prose_that_only_looks_a_little_like_code() {
    // Only unambiguous code is left unwrapped: these are prose, so they wrap
    // as they always did.
    for source in [
        "# x = y means that the left side takes the value\n",
        "# Speed (in pixels per second, scaled by the zoom)\n",
        "# if you need this, then the block starts:\n",
        "# this one\thas a tab in the middle of it and goes on\n",
        "# a typo  double space in the middle of it\n",
        "# ends with a semicolon like old C comments;\n",
    ] {
        let formatted = format(source, CommentWrap::Line);
        assert_ne!(formatted, source, "{source:?} was not wrapped");
        assert!(
            formatted.lines().all(|line| line.len() <= 40),
            "{formatted}"
        );
    }
}

#[test]
fn wrapping_never_starts_a_doc_comment_line_with_an_at_word() {
    // `## @experimental` on a line of its own is a Godot doc tag: it would
    // mark the member experimental. The word before it moves down instead.
    let source = "## aaa bbb ccc ddd eee fff ggg hhh iii @experimental\nvar x := 1\n";
    for mode in [CommentWrap::Line, CommentWrap::Paragraph] {
        assert_eq!(
            format(source, mode),
            "## aaa bbb ccc ddd eee fff ggg hhh\n## iii @experimental\nvar x := 1\n",
            "{mode:?}"
        );
    }
}

#[test]
fn wrapping_carries_a_run_of_at_words_with_the_word_before_them() {
    let source = "## aaa bbb ccc ddd eee fff ggg hhh @a @b @c\nvar x := 1\n";
    let formatted = format(source, CommentWrap::Line);
    assert_eq!(
        formatted,
        "## aaa bbb ccc ddd eee fff ggg\n## hhh @a @b @c\nvar x := 1\n"
    );
    assert!(
        formatted.lines().all(|line| !line.starts_with("## @")),
        "{formatted}"
    );
}

#[test]
fn wrapping_may_start_a_plain_comment_line_with_an_at_word() {
    // Only `##` comments have doc tags; a `#` comment wraps as usual.
    let source = "# aaa bbb ccc ddd eee fff ggg hhh iii @experimental\n";
    assert_eq!(
        format(source, CommentWrap::Line),
        "# aaa bbb ccc ddd eee fff ggg hhh iii\n# @experimental\n"
    );
}

#[test]
fn word_operators_do_not_make_commented_out_code_prose() {
    // `and not` and `if not` are two words in a row, but they are code.
    for source in [
        "# if not is_valid(t) and not t.dead and not flag:\n",
        "# return a == b and not c and not d and not e\n",
        "# speed = run_speed if not is_crouching else crouch_speed\n",
    ] {
        for mode in [CommentWrap::Line, CommentWrap::Paragraph] {
            assert_eq!(format(source, mode), source, "{mode:?}: {source:?}");
        }
    }
}

#[test]
fn paragraph_does_not_merge_code_with_word_operators_into_prose() {
    let next = "# speed = run if not crouching else crouch";
    assert_eq!(
        paragraph_lines(LONG, next),
        vec!["# aaa bbb ccc ddd eee fff ggg hhh iii", "# jjj kkk", next]
    );
}

#[test]
fn prose_with_word_operators_still_wraps() {
    // Two plain words in a row besides the operators: still prose.
    let source = "# this is not in the list and it goes on and on\n";
    let formatted = format(source, CommentWrap::Line);
    assert_ne!(formatted, source);
    assert!(
        formatted.lines().all(|line| line.len() <= 40),
        "{formatted}"
    );
}

#[test]
fn pipe_table_rows_are_neither_wrapped_nor_merged() {
    // Wrapping a row used to leave a lone `# |` on the next line.
    let row = "# | col a | col b | col c | col d | col e |";
    let source = format!("{row}\n");
    for mode in [CommentWrap::Line, CommentWrap::Paragraph] {
        assert_eq!(format(&source, mode), source, "{mode:?}");
    }
    let next = "# | a | b |";
    assert_eq!(
        paragraph_lines(LONG, next),
        vec!["# aaa bbb ccc ddd eee fff ggg hhh iii", "# jjj kkk", next]
    );
}

#[test]
fn prose_mentioning_a_pipe_still_wraps() {
    // Only a row that starts with `|` is a table; a `|` later on is prose.
    let source = "# split the words on | and then join them all again\n";
    assert_ne!(format(source, CommentWrap::Line), source);
}

#[test]
fn prose_starting_with_static_still_wraps() {
    // `static var` / `static func` mark commented-out code; `static` alone
    // starts a sentence.
    assert_eq!(
        format(
            "# static typing makes this function faster\n",
            CommentWrap::Line
        ),
        "# static typing makes this function\n# faster\n"
    );
    let code = "# static func helper(a: int, b: int) -> void:\n";
    assert_eq!(format(code, CommentWrap::Line), code);
}

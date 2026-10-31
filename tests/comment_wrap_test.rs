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

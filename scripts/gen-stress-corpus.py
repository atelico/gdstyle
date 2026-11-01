#!/usr/bin/env python3
"""Write large synthetic GDScript files for the CI performance gate.

The demo-project corpus is made of small files, so a pass whose cost grows
with the square of the file length looks fine on it. Each file here repeats
one shape (a long `elif` chain, thousands of members, many regions, ...)
until it is a few thousand lines long, where such a pass stands out.

Usage: scripts/gen-stress-corpus.py OUTPUT_DIR [REPEATS]

The output is deterministic, so two runs compare like for like.
"""

import os
import sys


def numbered(count):
    """Small numbers only: past 9999, format/large-number-underscores starts
    reporting, which would change the work per line as the size grows."""
    return [index % 9000 for index in range(count)]


def write_corpus(output_dir, repeats):
    shapes = {}

    lines = ["extends Node", "", "", "func pick(x):"]
    for index, number in enumerate(numbered(repeats)):
        keyword = "if" if index == 0 else "elif"
        lines += [f"\t{keyword} x == {number}:", f"\t\tprint({number})", f"\t\treturn {number}"]
    lines += ["\telse:", "\t\treturn -1"]
    shapes["elif_chain"] = lines

    lines = ["extends Node", "", "", "func pick(x):"]
    for index, number in enumerate(numbered(repeats)):
        keyword = "if" if index == 0 else "elif"
        lines.append(f"\t{keyword} x == {number}: return {number}")
    lines += ["\telse:", "\t\treturn -1"]
    shapes["inline_elif_chain"] = lines

    lines = ["extends Node", ""]
    lines += ["# " + " ".join(["word"] * 25) for _ in range(repeats)]
    lines.append("var x := 1")
    shapes["long_comments"] = lines

    lines = ["extends Node", ""]
    for index in range(repeats // 4):
        lines += [
            f"func f{index}():", "\tpass", "", "",
            f"var v{index} := 1", f"@export var e{index} := 1",
            f"const C{index} = 1", f"signal s{index}", "", "",
        ]
    shapes["unordered_members"] = lines

    lines = ["extends Node", ""]
    for index in range(repeats // 8):
        lines += [
            f"#region R{index}", '@warning_ignore_start("unused_signal")',
            f"signal s{index}", f"var _p{index} := 1",
            '@warning_ignore_restore("unused_signal")',
            f"func f{index}():", "\tpass", "#endregion", "",
        ]
    shapes["regions"] = lines

    lines = ["extends Node", ""]
    for index in range(repeats // 4):
        lines += [f'@export_group("G{index}")', f"@export var a{index} := 1",
                  f"var b{index} := 2", f"@export var c{index} := 3"]
    shapes["export_groups"] = lines

    lines = ["extends Node", "", "", "func build():"]
    lines += [
        f"\tvar v{index} = some_function(argument_one, argument_two, argument_three, "
        "argument_four, argument_five)"
        for index in range(repeats)
    ]
    shapes["long_lines"] = lines

    lines = ["extends Node", "", "", "func pick(x):", "\tmatch x:"]
    for number in numbered(repeats):
        lines += [f"\t\t{number}:", f"\t\t\treturn {number}"]
    shapes["match_arms"] = lines

    lines = ["extends Node", "", "", "func _process(_delta):"]
    lines += [f'\tvar n{index} = get_node("A") if %B else $C' for index in range(repeats)]
    shapes["process_lookups"] = lines

    os.makedirs(output_dir, exist_ok=True)
    for name, shape_lines in shapes.items():
        with open(os.path.join(output_dir, name + ".gd"), "w", newline="\n") as file:
            file.write("\n".join(shape_lines) + "\n")


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit("usage: gen-stress-corpus.py OUTPUT_DIR [REPEATS]")
    repeats = int(sys.argv[2]) if len(sys.argv) == 3 else 1500
    if repeats < 8:
        sys.exit(f"error: REPEATS must be at least 8, got {repeats}")
    write_corpus(sys.argv[1], repeats)


if __name__ == "__main__":
    main()

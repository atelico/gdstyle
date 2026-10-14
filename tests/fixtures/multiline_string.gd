extends Node

const HELP = """usage: tool [options]



Options are listed after three blank lines.
Trailing spaces matter here:   
	a tab-indented line
    four-space line
call_me(aaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb, cccccccccccccccccccccccccc, ddddddddddddddddddd)
"""


func _ready() -> void:
	var block := """
	    spaces after the indent tab
	"""
	# A string after the closing `"""` must not clear the closing line's mask.
	var tail := """
line
    closing indent kept""" % "x"
	var pair := """a
    b""" + """c
    d"""
	print(HELP + block + tail + pair)

extends Node


# Out of order on purpose, so the reorder passes run over the strings below.
func _ready() -> void:
	print(TEMPLATE + SPACED + ENDS_LIKE_A_COMMENT + str(b))


# A script template whose text looks like a doc block and a declaration.
const TEMPLATE = """
## Generated script
extends Node
"""

# Three blank-looking lines; the enum gives the fixer something to fix.
enum Mode {A, B}
const SPACED = """
a
   
	

b
"""

const ENDS_LIKE_A_COMMENT = """
x

# note
## doc"""
var b = 1


class Inner:
	var v = 1
	const BODY = """
class Fake:
    four spaces
   
done"""
@warning_ignore_restore("unused_variable")

## Demonstrates long line handling.
##
## Shows how gdstyle fmt breaks long lines at commas inside
## parentheses, brackets, and braces. Lines without delimiters
## or with only one item are left alone.

class_name LongLineExamples
extends Node

# Long function signature: will be broken into one parameter per line
func spawn_projectile(origin: Vector2, direction: Vector2, speed: float, damage: int, lifetime: float, owner: Node) -> Node:
	pass

# Long function call: will be broken at argument boundaries
func _ready():
	var projectile = spawn_projectile(Vector2(100, 200), Vector2(1, 0).normalized(), 500.0, 25, 3.0, self)
	var combined = merge_dictionaries(base_config, override_config, user_preferences, runtime_flags, debug_overrides)

# Long dictionary: will be broken into one entry per line
var default_bindings = {"move_left": KEY_A, "move_right": KEY_D, "jump": KEY_SPACE, "attack": KEY_J, "dash": KEY_K, "interact": KEY_E}

# Long array: will be broken into one element per line
var spawn_points = [Vector2(100, 100), Vector2(200, 100), Vector2(300, 100), Vector2(400, 100), Vector2(500, 100), Vector2(600, 100)]

# Nested calls: outer call broken, inner calls preserved intact
func calculate_score():
	var score = compute_final_score(get_base_damage(weapon, level), apply_multiplier(combo, streak), calculate_bonus(artifacts, buffs))

# Line without delimiters: cannot be broken, left as-is
var description = "This is a very long description string that exceeds the maximum line length but has no comma-separated items to break at"

# Short lines: not broken even though they have delimiters
func heal(amount: int, source: Node) -> void:
	var targets = [self, get_parent()]
	var result = compute(amount, source)

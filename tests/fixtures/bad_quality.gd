class_name BadQuality
extends Node

signal health_changed(old_value: int, new_value: int)

const MAX_HEALTH: int = 100

var speed: float = 10.0
var health: int = 100

# --- Rule: no-debug-print ---
func debug_stuff() -> void:
	print("hello")
	prints("a", "b")
	printerr("error!")
	print_debug("debug")

# --- Rule: self-comparison ---
func compare_self() -> void:
	var x: int = 5
	if x == x:
		pass
	if x != x:
		pass
	if x < x:
		pass

# --- Rule: no-self-assign ---
func assign_self() -> void:
	var y: int = 10
	y = y

# --- Rule: duplicate-dict-key ---
func make_dict() -> Dictionary:
	var d: Dictionary = {
		"name": "player",
		"health": 100,
		"name": "enemy",
	}
	return d

# --- Rule: duplicated-load ---
func load_stuff() -> void:
	var scene_a: PackedScene = preload("res://scenes/player.tscn")
	var scene_b: PackedScene = preload("res://scenes/player.tscn")
	var _unused_a: PackedScene = scene_a
	var _unused_b: PackedScene = scene_b

# --- Rule: no-else-return ---
func classify(value: int) -> String:
	if value > 100:
		return "high"
	elif value > 50:
		return "medium"
	else:
		return "low"

# --- Rule: unreachable-code ---
func unreachable_example() -> int:
	return 42
	var dead: int = 0
	dead += 1

# --- Rule: await-in-loop ---
func fetch_all(urls: Array) -> void:
	for url: String in urls:
		await get_tree().create_timer(1.0).timeout

# --- Rule: allocation-in-loop ---
func spawn_enemies(count: int) -> void:
	for i: int in range(count):
		var enemy: Node = Node.new()
		add_child(enemy)

# --- Rule: process-get-node ---
func _process(delta: float) -> void:
	var label: Label = $HUD/Label
	label.text = str(delta)

func _physics_process(delta: float) -> void:
	var body: Node = get_node("Body")
	body.position.x += delta

# --- Rule: unnecessary-pass ---
func has_extra_pass() -> void:
	var z: int = 5
	z += 1
	pass

# --- Rule: max-nesting-depth (depth 5, default max is 4) ---
func deeply_nested() -> void:
	if true:
		if true:
			if true:
				if true:
					if true:
						pass

# --- Rule: max-returns (7 returns, default max is 6) ---
func many_returns(x: int) -> int:
	if x == 1:
		return 1
	if x == 2:
		return 2
	if x == 3:
		return 3
	if x == 4:
		return 4
	if x == 5:
		return 5
	if x == 6:
		return 6
	return 0

# --- Rule: max-branches (9 branches, default max is 8) ---
func many_branches(x: int) -> void:
	if x == 1:
		pass
	if x == 2:
		pass
	if x == 3:
		pass
	if x == 4:
		pass
	if x == 5:
		pass
	if x == 6:
		pass
	if x == 7:
		pass
	if x == 8:
		pass
	if x == 9:
		pass

# --- Rule: max-local-variables (11 vars, default max is 10) ---
func too_many_locals() -> void:
	var a: int = 1
	var b: int = 2
	var c: int = 3
	var d: int = 4
	var e: int = 5
	var f: int = 6
	var g: int = 7
	var h: int = 8
	var i: int = 9
	var j: int = 10
	var k: int = 11
	var _sum: int = a + b + c + d + e + f + g + h + i + j + k

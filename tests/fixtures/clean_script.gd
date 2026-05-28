@tool
class_name StateMachine
extends Node
## Hierarchical state machine for the player.
##
## Initializes states and delegates engine callbacks
## to the active state.

signal state_changed(previous: String, current: String)

enum State {
	IDLE,
	RUNNING,
	JUMPING,
}

const MAX_STATES: int = 10

static var instance_count: int = 0

@export var initial_state: Node

var is_active: bool = true

@onready var _state: Node = $State

func _init() -> void:
	add_to_group("state_machine")
	instance_count += 1

func _ready() -> void:
	state_changed.connect(_on_state_changed)
	_state.enter()

func _physics_process(delta: float) -> void:
	_state.physics_process(delta)

func transition_to(target_path: String, msg: Dictionary = {}) -> void:
	if not has_node(target_path):
		return
	var target_state: Node = get_node(target_path)
	_state.exit()
	_state = target_state
	_state.enter(msg)

func _on_state_changed(previous: String, current: String) -> void:
	pass

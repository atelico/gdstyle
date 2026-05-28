## A generic finite state machine.

class_name StateMachine
extends Node

signal StateChanged(old_state: StringName, new_state: StringName)

@export var initial_state: NodePath

var currentState: StringName = &""
var previousState: StringName = &""
var _states: Dictionary = {}
var stateTimer: float = 0.0

func _ready() -> void:
	for child in get_children():
		if child.has_method('enter') and child.has_method('exit') and child.has_method('update'):
			_states[child.name] = child
			child.set_process(false)
			child.set_physics_process(false)

	if initial_state:
		var node = get_node_or_null(initial_state)
		if node:
			transition_to(node.name)

func _process(delta: float) -> void:
	if currentState == &"":
		return
	stateTimer+=delta
	if _states.has(currentState):
		_states[currentState].update(delta)

func _physics_process(delta: float) -> void:
	if currentState == &"" :
		return
	if _states.has(currentState):
		if _states[currentState].has_method("physics_update"):
			_states[currentState].physics_update(delta)

func transition_to(new_state: StringName, params: Dictionary = {}) -> void:
	if !_states.has(new_state):
		push_warning("StateMachine: state '%s' does not exist" % new_state)
		return

	if currentState != &"":
		_states[currentState].exit()
		_states[currentState].set_process(false)
		_states[currentState].set_physics_process(false)

	previousState = currentState
	currentState = new_state
	stateTimer = 0.0
	_states[currentState].set_process(true)
	_states[currentState].set_physics_process(true)
	_states[currentState].enter(params)

	StateChanged.emit(previousState, currentState)

func get_current_state_node() -> Node:
	if _states.has(currentState):
		return _states[currentState]
	return null

func HasState(state_name: StringName) -> bool:
	return _states.has(state_name)

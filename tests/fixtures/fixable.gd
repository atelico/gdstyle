class_name my_player
extends CharacterBody2D

signal healthChanged(old_value: int, new_value: int)

const maxSpeed: float = 200.0

var PlayerHealth: int = 100
var x = 5

func takeDamage(amount: int) -> void:
	if (is_alive):
		pass

func _ready() -> void:
	var y = 'hello'
	if a && b:
		pass
	if !c:
		pass
	var z = 0xFF

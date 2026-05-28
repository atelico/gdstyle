class_name my_player
extends CharacterBody2D

signal healthChanged(oldValue: int, newValue: int)

enum player_state {
	idle,
	Walking,
	RUNNING,
}

const maxSpeed: float = 200.0

var PlayerHealth: int = 100

func takeDamage(Amount: int) -> void:
	pass

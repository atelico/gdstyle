## A player character controller.
##
## Handles movement, health, and combat for the main player.

class_name Player
extends CharacterBody2D

signal healthChanged(new_health: int)
signal died

const maxSpeed = 400.0
const JUMP_FORCE: float = -600.0
const GRAVITY = 980

enum PlayerState {IDLE, RUNNING, JUMPING, FALLING, ATTACKING}

@export var startHealth: int = 100
@export var _internal_debug: bool = false

var health: int = 100
var state: PlayerState = PlayerState.IDLE
var combo_count: int = 0
var DamageMultiplier: float = 1.0

@onready var animation_player: AnimationPlayer = $AnimationPlayer
@onready var sprite: Sprite2D = $Sprite2D
@onready var hitBox: Area2D = $hit_box
@onready var health_bar: ProgressBar = $UILayer/health_bar

func _ready() -> void:
	health = startHealth
	healthChanged.emit(health)

func _physics_process(delta: float) -> void:
	if (!is_on_floor()):
		velocity.y += GRAVITY * delta

	var direction := Input.get_axis("move_left","move_right")

	if direction:
		velocity.x = direction * maxSpeed
		sprite.flip_h = direction < 0
		if is_on_floor():
			state = PlayerState.RUNNING
	else:
		velocity.x = move_toward(velocity.x, 0, maxSpeed)
		if is_on_floor():
			state = PlayerState.IDLE

	if Input.is_action_just_pressed("jump") && is_on_floor():
		velocity.y = JUMP_FORCE
		state = PlayerState.JUMPING

	move_and_slide()

func take_damage(amount: int, source: Node, damage_type: String, is_critical: bool, knockback_force: float, status_effect: String) -> void:
	var actual_damage = amount * DamageMultiplier
	health -= int(actual_damage)
	if health <= 0:
		health = 0
		died.emit()
	healthChanged.emit(health)
	_apply_knockback(knockback_force)

func heal(amount: int) -> void:
	health = min(health + amount, startHealth)
	healthChanged.emit(health)

func _apply_knockback(Force: float) -> void:
	velocity.x += Force * (-1 if sprite.flip_h else 1)

func get_state_name() -> String:
	match state:
		PlayerState.IDLE: return "Idle"
		PlayerState.RUNNING: return "Running"
		PlayerState.JUMPING: return "Jumping"
		PlayerState.FALLING: return "Falling"
		PlayerState.ATTACKING: return "Attacking"
	return "Unknown"

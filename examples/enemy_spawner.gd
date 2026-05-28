extends Node2D

const ENEMY_SCENE = preload("res://scenes/enemy.tscn")
const spawnInterval: float = 3.0
const maxEnemies = 50
const SPAWN_RADIUS:float = 500.0

var active_enemies: Array[Node2D] = []
var totalSpawned: int = 0
var _timer: Timer
var WaveNumber: int = 1
var enemiesPerWave: int = 5
var is_spawning: bool = false

signal waveStarted(wave: int)
signal wave_complete(wave: int)
signal enemy_die(enemy: Node2D)

@onready var spawn_area: CollisionShape2D = $SpawnArea/CollisionShape2D

func _ready() -> void:
	_timer = Timer.new()
	_timer.wait_time = spawnInterval
	_timer.timeout.connect(_on_spawn_timer)
	add_child(_timer)


func start_wave() -> void:
	WaveNumber += 1
	waveStarted.emit(WaveNumber)
	is_spawning = true
	totalSpawned = 0
	_timer.start()


func _on_spawn_timer() -> void:
	if totalSpawned>=enemiesPerWave*WaveNumber:
		_timer.stop()
		is_spawning = false
		return

	if active_enemies.size()>=maxEnemies:
		return

	_spawn_enemy()

func _spawn_enemy() -> void:
	var enemy = ENEMY_SCENE.instantiate()
	var angle = randf() * TAU
	var dist = randf_range(100.0, SPAWN_RADIUS)
	enemy.position = global_position + Vector2(cos(angle), sin(angle)) * dist
	enemy.tree_exiting.connect(_on_enemy_removed.bind(enemy))
	get_parent().add_child(enemy)
	active_enemies.append(enemy)
	totalSpawned+=1

func _on_enemy_removed(enemy: Node2D) -> void:
	active_enemies.erase(enemy)
	enemy_die.emit(enemy)
	if !is_spawning and active_enemies.size() == 0:
		wave_complete.emit(WaveNumber)


func get_spawn_info() -> Dictionary:
	return {
		"wave": WaveNumber,
		"active": active_enemies.size(),
		"total_spawned": totalSpawned,
		"is_spawning": is_spawning,
		"enemies_per_wave": enemiesPerWave * WaveNumber
	}


func set_difficulty(base_enemies: int, spawn_rate: float, max_alive: int) -> void:
	enemiesPerWave = base_enemies
	_timer.wait_time = spawn_rate

func reset() -> void:
	_timer.stop()
	is_spawning = false
	for enemy in active_enemies:
		if is_instance_valid(enemy):
			enemy.queue_free()
	active_enemies.clear()
	WaveNumber = 0
	totalSpawned = 0

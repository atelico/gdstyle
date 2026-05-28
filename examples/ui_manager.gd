extends CanvasLayer

## Manages UI screens: HUD, pause menu, game over, and transitions.

const fadeTime: float = 0.3
const HEX_WHITE = 0xFFFFFF
const NOTIFICATION_DURATION = 5000

var _screens: Dictionary = {}
var _screen_stack: Array[StringName] = []
var _is_transitioning: bool = false

@onready var hud: Control = $HUD
@onready var pauseMenu: Control = $PauseMenu
@onready var gameOverScreen: Control = $GameOverScreen
@onready var fade_overlay: ColorRect = $FadeOverlay
@onready var notification_container: VBoxContainer = $NotificationContainer
@onready var healthLabel: Label = $HUD/TopBar/HealthLabel
@onready var scoreLabel: Label = $HUD/TopBar/ScoreLabel

signal screenChanged(screen_name: StringName)

func _ready() -> void:
	_screens = {
		&"hud": hud,
		&"pause": pauseMenu,
		&"game_over": gameOverScreen,
	}
	for screen_name in _screens:
		_screens[screen_name].visible = false

	show_screen(&"hud")
	fade_overlay.modulate.a = 0.0

func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("pause"):
		if _screen_stack.back() == &"hud":
			push_screen(&"pause")
			get_tree().paused = true
		elif _screen_stack.back() == &"pause":
			pop_screen()
			get_tree().paused = false
		get_viewport().set_input_as_handled()


func show_screen(screen_name: StringName) -> void:
	if !_screens.has(screen_name):
		push_warning("UIManager: unknown screen '%s'" % screen_name)
		return

	# Hide all
	for key in _screens:
		_screens[key].visible = false

	_screens[screen_name].visible = true
	_screen_stack = [screen_name]
	screenChanged.emit(screen_name)

func push_screen(screen_name: StringName) -> void:
	if !_screens.has(screen_name):
		return
	if _screen_stack.size() > 0:
		_screens[_screen_stack.back()].visible = false
	_screen_stack.append(screen_name)
	_screens[screen_name].visible = true
	screenChanged.emit(screen_name)

func pop_screen() -> void:
	if _screen_stack.size() <= 1:
		return
	var current = _screen_stack.pop_back()
	_screens[current].visible = false
	var previous = _screen_stack.back()
	_screens[previous].visible = true
	screenChanged.emit(previous)


func fade_to_screen(screen_name: StringName) -> void:
	if _is_transitioning:
		return
	_is_transitioning = true
	var tween = create_tween()
	tween.tween_property(fade_overlay, "modulate:a", 1.0, fadeTime)
	await tween.finished
	show_screen(screen_name)
	var tween2 = create_tween()
	tween2.tween_property(fade_overlay, "modulate:a", 0.0, fadeTime)
	await tween2.finished
	_is_transitioning = false

func update_health_display(current: int, maximum: int) -> void:
	healthLabel.text = "HP: %d/%d" % [current, maximum]
	var ratio: float = float(current) / float(maximum)
	if ratio < .25:
		healthLabel.add_theme_color_override("font_color", Color.RED)
	elif ratio < .5:
		healthLabel.add_theme_color_override("font_color", Color.YELLOW)
	else:
		healthLabel.add_theme_color_override("font_color", Color.WHITE)

func update_score(score: int) -> void:
	scoreLabel.text = "Score: %d" % score

func show_notification(text: String, duration: float = 3.0) -> void:
	var label = Label.new()
	label.text = text
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.add_theme_color_override('font_color',Color(1, 1, 1, 1))
	notification_container.add_child(label)
	await get_tree().create_timer(duration).timeout
	if is_instance_valid(label):
		var tween = create_tween()
		tween.tween_property(label, "modulate:a", 0.0, .5)
		await tween.finished
		label.queue_free()

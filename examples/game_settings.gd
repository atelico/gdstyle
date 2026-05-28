class_name GameSettings
extends Resource

## Persistent game settings saved to user://settings.tres.

const CONFIG_PATH = 'user://settings.tres'
const defaultVolume: float = .8
const MAX_FPS_OPTIONS = [30, 60, 120, 0]
const MIN_WINDOW_SIZE: Vector2i = Vector2i(1280, 720)
const VERSION := "1.0.0"

enum WindowMode {
	FULLSCREEN,
	BORDERLESS,
	WINDOWED
}

enum quality_level {
	LOW,
	MEDIUM,
	HIGH,
	ULTRA
}

@export var master_volume: float = .8
@export var music_volume: float = .8
@export var sfx_volume: float = .8
@export var window_mode: WindowMode = WindowMode.FULLSCREEN
@export var vsync_enabled: bool = true
@export var target_fps: int = 60
@export var quality: quality_level = quality_level.HIGH
@export var mouse_sensitivity: float = .5
@export var language: String = 'en'
@export var show_fps: bool = false
@export var screen_shake: bool = true
@export var colorblind_mode: int = 0
@export var keybindings: Dictionary = {}

func apply() -> void:
	# Audio
	AudioServer.set_bus_volume_db(0, linear_to_db(master_volume))
	AudioServer.set_bus_volume_db(1,linear_to_db(music_volume))
	AudioServer.set_bus_volume_db(2,linear_to_db(sfx_volume))

	# Display
	match window_mode:
		WindowMode.FULLSCREEN:
			DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_FULLSCREEN)
		WindowMode.BORDERLESS:
			DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_EXCLUSIVE_FULLSCREEN)
		WindowMode.WINDOWED:
			DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_WINDOWED)

	DisplayServer.window_set_vsync_mode(
		DisplayServer.VSYNC_ENABLED if vsync_enabled else DisplayServer.VSYNC_DISABLED
	)

	if target_fps == 0:
		Engine.max_fps = 0
	else:
		Engine.max_fps = target_fps

func save_to_disk() -> void:
	var err = ResourceSaver.save(self, CONFIG_PATH)
	if err!=OK:
		push_error("Failed to save settings: %s" % error_string(err))

static func load_from_disk() -> GameSettings:
	if ResourceLoader.exists(CONFIG_PATH):
		var loaded = ResourceLoader.load(CONFIG_PATH)
		if loaded is GameSettings:
			return loaded
	var settings = GameSettings.new()
	settings.save_to_disk()
	return settings

func reset_to_defaults() -> void:
	master_volume = defaultVolume
	music_volume = defaultVolume
	sfx_volume = defaultVolume
	window_mode = WindowMode.FULLSCREEN
	vsync_enabled = true
	target_fps = 60
	quality = quality_level.HIGH
	mouse_sensitivity = .5
	language = "en"
	show_fps = false
	screen_shake = true
	colorblind_mode = 0
	keybindings = {}
	apply()

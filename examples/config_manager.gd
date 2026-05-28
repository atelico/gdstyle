## Autoload singleton that manages game settings.
##
## Showcases common style issues found in real Godot projects:
## - Doc comments (##) before class_name (should be after)
## - Naming violations (PascalCase vars, SCREAMING_CASE vars with @export)
## - Ordering violations (@onready before @export, vars after funcs)
## - Cross-file reference patterns (SETTINGS is referenced by other scripts)
## - Private constants with underscore prefix
## - Multi-line var declarations that must survive reordering intact

class_name SettingsManager
extends Node

const _DEFAULT_SETTINGS_PATH = "user://settings.cfg"
const _FALLBACK_VOLUME = 80

@onready var settings_panel: Control = %SettingsPanel
@onready var status_label: Label = %StatusLabel
@export var SETTINGS_FILE: String = "user://game_settings.cfg"
@export var auto_save: bool = true

var SETTINGS: ConfigFile = null
var DEFAULT_VOLUME = 80
var DEFAULT_DIFFICULTY = 2
var DEFAULT_BRIGHTNESS = 0.7
var cached_display = SettingsManager.get_instance().load_value(
	"display", "BRIGHTNESS"
)

signal settings_change
signal on_settings_written
signal menu_ready
signal finished_loading
signal network_event
signal scene_load_progress
signal on_query_result

enum SettingsSection {General, Graphics, Audio, Controls}

func _ready():
	SETTINGS = _load_settings_file()
	# Apply saved values
	self.DEFAULT_VOLUME = SETTINGS.get_value("audio", "DEFAULT_VOLUME", 80)
	self.DEFAULT_DIFFICULTY = SETTINGS.get_value("gameplay", "DIFFICULTY", 2)
	self.DEFAULT_BRIGHTNESS = SETTINGS.get_value("display", "BRIGHTNESS", 0.7)
	print(DEFAULT_VOLUME)

func get_instance() -> SettingsManager:
	return self

func _load_settings_file() -> ConfigFile:
	var cf = ConfigFile.new()
	var err = cf.load(SETTINGS_FILE)
	if err != OK:
		# Use defaults
		pass
	return cf

## Persist current settings to disk.
## Creates the file if it doesn't exist.
static func SaveAndFlush(path: String):
	var cf = ConfigFile.new()
	cf.save(path)

func SaveSettings():
	SETTINGS.save(SETTINGS_FILE)
	on_settings_written.emit()

func GetValue(section: String, key: String, default_val = null):
	return SETTINGS.get_value(section, key, default_val)

static var ToggleSettingsUI = "toggle_settings_ui"

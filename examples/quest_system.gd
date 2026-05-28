## Quest management system for tracking player objectives.
##
## Demonstrates formatting and ordering edge cases:
## - Doc comments (##) appearing before class_name (formatter moves them after)
## - Doc comments between vars and static funcs (attached to the function)
## - Multi-line variable declarations that must survive reordering
## - Inner class with static methods out of canonical order
## - Enum expansion from single-line to multi-line

class_name QuestSystem
extends Node

@onready var quest_log_ui: Control = %QuestLogUI
@onready var tracker_label: Label = %TrackerLabel
@export var max_active_quests: int = 10
@export var auto_track: bool = true

var _active_quests: Array = []
var _completed_ids: Dictionary = {}

var default_reward_xp = GameSettings.get_instance().load_value(
	"quests", "DEFAULT_REWARD_XP"
)
var default_reward_gold = GameSettings.get_instance().load_value(
	"quests", "DEFAULT_REWARD_GOLD"
)

const MAX_QUEST_HISTORY = 500
const DECAY_FACTOR = 0.9

enum QuestPriority { Low, Medium, High, Critical }
enum QuestState {NotStarted, Active, Completed, Failed, Expired}

signal quest_accept
signal quest_complete
signal quest_fail
signal reward_grant
signal on_objective_start_tracking
signal tracker_ready

## Normalize quest data for strict validation.
## Ensures all required fields are present and
## removes deprecated properties.
static func normalize_quest_data(data: Dictionary) -> Dictionary:
	if not data.has("id"):
		data["id"] = "unknown"
	return data

func _ready():
	# Initialize quest cache
	var saved = _load_saved_quests()
	for quest in saved:
		_active_quests.append(quest)

func _process(delta: float):
	if _active_quests.size() > 0:
		_check_quest_timers(delta)

func accept_quest(quest_id: String) -> bool:
	if _active_quests.size() >= max_active_quests:
		return false
	_active_quests.append(quest_id)
	quest_accept.emit()
	return true

func complete_quest(quest_id: String):
	_completed_ids[quest_id] = true
	_active_quests.erase(quest_id)
	quest_complete.emit()

func _load_saved_quests() -> Array:
	return []

func _check_quest_timers(delta: float):
	pass

# Inner class with members out of canonical order
class QuestObjective:
	var description: String
	var target_count: int
	var current_count: int

	func _init(desc: String = "", target: int = 1):
		self.description = desc
		self.target_count = target
		self.current_count = 0

	# static func should appear before _init per canonical order
	static func from_dict(d: Dictionary) -> QuestObjective:
		return QuestObjective.new(
			d.get("description", ""),
			d.get("target", 1)
		)

	func is_complete() -> bool:
		return current_count >= target_count

	func _to_string():
		return "%s (%d/%d)" % [description, current_count, target_count]

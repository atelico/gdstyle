## Enemy NPC controller for a dungeon crawler.
##
## Showcases common style issues found in real Godot projects:
## - Enum member naming (PascalCase instead of SCREAMING_CASE)
## - Acronym handling (NPCInCombat → NPC_IN_COMBAT)
## - Local variables inside function bodies (should not trigger ordering)
## - Signal past-tense with irregular verbs and gerunds
## - Member ordering (vars after funcs, signals after enums)
## - Single-line enums
## - Inner class with static methods out of order

class_name EnemyController
extends CharacterBody2D

enum EnemyBehaviorMode { None, NPCInCombat, PlayerInControl }
enum QuestStatus {NotStarted, InProgress, Completed, Failed}

signal combat_event_begin
signal combat_event_finish
signal quest_change
signal on_hp_start_draining
signal hud_view_changed_status
signal _save_db_ready

var behavior_mode = EnemyBehaviorMode.NPCInCombat

func _ready():
	# comment inside function body
	var timer = Timer.new()
	timer.wait_time = 1.0
	add_child(timer)

@export var enemy_name: String
@export var aggro_chance: float = 1.0
@export var use_ranged_attack: bool = true
@onready var nav_agent: Node = %NavigationAgent
@onready var loot_table: Node = %LootTable
@onready var hit_detector: Node = %HitDetector
@onready var patrol_handler: Node = %PatrolHandler
@onready var ability_system: Node = %AbilitySystem

@export var DETECTION_RADIUS: int = 300

const C_HEALTH_HIGH = Color(0.2, 0.8, 0.2, 1.0)
const C_HEALTH_MID = Color(0.9, 0.7, 0.1, 0.96)
const C_HEALTH_LOW = Color(0.9, 0.2, 0.1, 0.58)

var _sprite_idle: Texture = null
var _sprite_attack: Texture = null

var WeaponSlotClass: Resource = null

signal on_target_reached(pos: Vector2)
signal on_enemy_moved(direction: Vector2, speed: float)
signal on_enemy_stopped()

func _choose_target_to_attack():
	# Local variables: should NOT trigger ordering warnings
	var nearby_players = []
	var valid_targets = []

	for entity in []:
		var e = entity
		if e:
			var data = {}
			data["key"] = "value"

func _is_valid_hitbox_area_2D(node):
	if node is Area2D:
		return self._is_valid_hitbox_area_2D(node)
	return false

func _process(delta: float):
	if behavior_mode == EnemyBehaviorMode.NPCInCombat:
		_handle_combat_logic(delta)
	elif behavior_mode == EnemyBehaviorMode.PlayerInControl:
		_handle_player_override(delta)

func _handle_combat_logic(delta: float):
	pass

func _handle_player_override(delta: float):
	pass

func SetBehaviorMode(mode: EnemyBehaviorMode):
	behavior_mode = mode

func GetWeaponSlot():
	return self.WeaponSlotClass

# Inner class with ordering issues: static func after regular methods
class LootDrop:
	var WeaponSlotClass: Resource
	var rarity: String

	func _init():
		self.WeaponSlotClass = null
		rarity = ""

	func create():
		var item = self.WeaponSlotClass.instantiate()
		return item

	static func from_dict(d: Dictionary) -> LootDrop:
		var drop = LootDrop.new()
		drop.rarity = d.get("rarity", "common")
		return drop

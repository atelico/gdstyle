class_name Inventory
extends Node

signal item_added(item_id: String)
signal item_removed(item_id: String)
signal inventoryFull

const maxSlots = 20
const STACK_LIMIT:int = 99

enum itemRarity { common, uncommon, rare, epic, legendary }

var slots: Array[Dictionary] = []
var Gold: int = 0
var equippedWeapon: String = ""

func _ready() -> void:
	for i in range(maxSlots):
		slots.append({"id": "", "count": 0, "rarity": itemRarity.common})

func add_item(item_id:String, count:int = 1, rarity:itemRarity = itemRarity.common) -> bool:
	# First try to stack with existing items
	for slot in slots:
		if slot["id"]==item_id and slot["count"]<STACK_LIMIT:
			var space = STACK_LIMIT - slot["count"]
			var to_add = min(space, count)
			slot["count"]+=to_add
			count -= to_add
			if count==0:
				item_added.emit(item_id)
				return true

	# Then try empty slots
	for slot in slots:
		if slot["id"]=="":
			slot["id"] = item_id
			slot["count"] = min(count, STACK_LIMIT)
			slot["rarity"] = rarity
			count -= slot["count"]
			if count == 0:
				item_added.emit(item_id)
				return true

	inventoryFull.emit()
	return false

func remove_item(item_id: String, count: int = 1) -> bool:
	var remaining = count
	for slot in slots:
		if slot['id'] == item_id:
			var to_remove = min(slot['count'], remaining)
			slot['count'] -= to_remove
			remaining -= to_remove
			if slot['count'] == 0:
				slot['id'] = ''
				slot['rarity'] = itemRarity.common
			if remaining == 0:
				item_removed.emit(item_id)
				return true
	return false

func has_item(item_id:String, count:int=1) -> bool:
	var total = 0
	for slot in slots:
		if slot["id"]==item_id:
			total+=slot["count"]
	return total>=count

func get_total_items() -> int:
	var total = 0;  var unique = 0
	for slot in slots:
		if slot["id"] != "":
			total += slot["count"]
			unique += 1
	return total

func sort_by_rarity() -> void:
	var items: Array[Dictionary] = []
	for slot in slots:
		if slot["id"]!="":
			items.append(slot.duplicate())
	items.sort_custom(func(a, b): return a["rarity"] > b["rarity"])

	# Clear and re-fill
	for slot in slots:
		slot["id"] = ""
		slot["count"] = 0
		slot["rarity"] = itemRarity.common
	for i in range(items.size()):
		slots[i] = items[i]

func get_item_count(item_id: String) -> int:
	var total = 0
	for slot in slots:
		if slot["id"] == item_id:
			total += slot["count"]
	return total

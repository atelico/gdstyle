## Tests long-line edge cases from real-world projects.
## These patterns must NOT be corrupted by the formatter.

class_name LongLineEdgeCases
extends Node

@export var entity_name: String = "default"

# Pattern: % string formatting with multi-line argument array.
# The [ is on the format string line, args continue on the next line.
func log_status(curr_time, activity, start_time, duration, count):
	Log.info("Controller::select_next::%s at %s: plan_activity='%s' (start=%s, dur=%dmin), items=%d"
		% [self.entity_name, curr_time.strftime("%H:%M"),
			activity,
			start_time.strftime("%H:%M") if start_time else "?",
			duration,
			count,])

# Pattern: multi-line % formatting with trailing comma in array.
func log_action(action_name, finish_time):
	Log.info("Controller::on_action_finish::%s at %s: action='%s'"
		% [self.entity_name, finish_time.strftime("%H:%M"),
			action_name if action_name else "null",])

# Pattern: long format string with multiple args on continuation line.
func log_transition(prev_state, next_state):
	push_warning("StateChange::%s: prev='%s' -> next='%s'"
		% [self.entity_name, prev_state,
			next_state,])

# Pattern: already-broken function call with args on continuation line.
func check_validity(target, items):
	if not Controller._is_in_list(
		self.current_target,
		items,
	):
		return false
	return true

# Pattern: long @export/preload initializer.
@export var header_packed: PackedScene = preload("res://scenes/ui/scene_loader/scene_group_header.tscn")
@onready var engagement_animation: CircularTimerAnimation = %EngagementTimerAnimation

# Pattern: long property chain access.
func get_display_name():
	return self.entity_behavior.get_interactable().display_name.split(" ")[0]

# Pattern: conditional expression spanning multiple lines.
func get_prev_state():
	var _prev = (
		self.working_memory
			.curr_selected_activity
			.name
		if self.working_memory
			.curr_selected_activity
		else "null")
	return _prev

# Pattern: long string literal that cannot be broken.
func show_warning():
	push_warning("Planning: LLM generation failed with timeout, using static fallback plan for %s" % entity_name)

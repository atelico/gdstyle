@tool
extends EditorPlugin
## Editor plugin that integrates gdstyle into the Godot editor.
##
## Adds a bottom panel with lint results. Runs gdstyle on save or on demand,
## parses JSON output, and lets you click diagnostics to jump to the source.

const PANEL_SCENE := preload("res://addons/gdstyle/gdstyle_panel.gd")

var _panel: Control
var _editor_interface: EditorInterface


func _enter_tree() -> void:
	_editor_interface = get_editor_interface()
	_panel = PANEL_SCENE.new()
	_panel.editor_interface = _editor_interface
	add_control_to_bottom_panel(_panel, "gdstyle")

	# Auto-lint on save.
	resource_saved.connect(_on_resource_saved)


func _exit_tree() -> void:
	if resource_saved.is_connected(_on_resource_saved):
		resource_saved.disconnect(_on_resource_saved)
	if _panel:
		remove_control_from_bottom_panel(_panel)
		_panel.queue_free()
		_panel = null


func _on_resource_saved(resource: Resource) -> void:
	if not _panel:
		return
	var path := resource.resource_path
	if not path.ends_with(".gd"):
		return
	# Format on save: the file is already on disk, format it there.
	if _panel.auto_format_on_save:
		_panel.format_file_on_disk(path)
	if _panel.auto_lint_on_save:
		_panel.lint_project()

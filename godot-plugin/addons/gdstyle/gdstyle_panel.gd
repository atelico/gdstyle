@tool
extends VBoxContainer
## Bottom panel that displays gdstyle diagnostics.
##
## Supports two backends:
##   1. GDExtension (native): if the GdStyle class is available, calls Rust
##      directly with zero process overhead.
##   2. CLI fallback: spawns the gdstyle binary with check --format json.
##      Auto-downloads the binary from GitHub Releases if not found.

const GITHUB_REPO := "atelico/gdstyle"
const BIN_DIR := "res://addons/gdstyle/bin"
const SETTINGS_PATH := "res://addons/gdstyle/settings.json"

var editor_interface: EditorInterface
var auto_lint_on_save: bool = true
var auto_format_on_save: bool = false

var _tree: Tree
var _context_menu: PopupMenu
var _status_label: Label
var _mode_switch: OptionButton
var _cli_row: HBoxContainer
var _gdextension_available: bool = false
var _auto_lint_check: CheckBox
var _auto_format_check: CheckBox
var _binary_path_edit: LineEdit
var _download_button: Button
var _diagnostics: Array[Dictionary] = []

# Set while _load_settings() is populating the UI. Any toggle / text
# handler that fires from the assignment must not write settings.json
# back: on Windows the WRITE open from the toggle handler would collide
# with the still-open READ handle from this load and Godot's atomic
# save would leave an orphan settings.json#######.tmp behind every
# project startup. See issue #6.
var _loading_settings: bool = false

# CLI binary path (used when GDExtension is not available).
var _gdstyle_path: String = OS.get_environment("HOME").path_join(".local/bin/gdstyle")

# Which backend is active.
var _use_gdextension: bool = false
var _native_linter: RefCounted = null


func _ready() -> void:
	_detect_backend()
	_build_ui()
	_load_settings()


# --- Backend detection ---


func _detect_backend() -> void:
	# Try to instantiate the GDExtension class.
	if ClassDB.class_exists(&"GdStyle"):
		_native_linter = ClassDB.instantiate(&"GdStyle")
		if _native_linter:
			_gdextension_available = true
			_use_gdextension = true
			return
	_gdextension_available = false
	_use_gdextension = false


# --- UI construction ---


func _build_ui() -> void:
	# --- Toolbar ---
	var toolbar := HBoxContainer.new()
	toolbar.add_theme_constant_override("separation", 6)
	add_child(toolbar)

	# Lint actions.
	var lint_project_btn := Button.new()
	lint_project_btn.text = "Lint Project"
	lint_project_btn.tooltip_text = "Run gdstyle on all .gd files in the project"
	lint_project_btn.pressed.connect(lint_project)
	toolbar.add_child(lint_project_btn)

	var lint_file_btn := Button.new()
	lint_file_btn.text = "Lint File"
	lint_file_btn.tooltip_text = "Run gdstyle on the currently open script"
	lint_file_btn.pressed.connect(lint_current_file)
	toolbar.add_child(lint_file_btn)

	var fix_file_btn := Button.new()
	fix_file_btn.text = "Fix File"
	fix_file_btn.tooltip_text = "Apply all available fixes to the currently open script"
	fix_file_btn.pressed.connect(fix_current_file)
	toolbar.add_child(fix_file_btn)

	toolbar.add_child(VSeparator.new())

	# Format actions.
	var fmt_project_btn := Button.new()
	fmt_project_btn.text = "Format Project"
	fmt_project_btn.tooltip_text = "Format all .gd files in the project"
	fmt_project_btn.pressed.connect(format_project)
	toolbar.add_child(fmt_project_btn)

	var fmt_file_btn := Button.new()
	fmt_file_btn.text = "Format File"
	fmt_file_btn.tooltip_text = "Format the currently open script"
	fmt_file_btn.pressed.connect(format_current_file)
	toolbar.add_child(fmt_file_btn)

	toolbar.add_child(VSeparator.new())

	# On-save checkboxes.
	_auto_lint_check = CheckBox.new()
	_auto_lint_check.text = "Lint on Save"
	_auto_lint_check.button_pressed = auto_lint_on_save
	_auto_lint_check.toggled.connect(_on_auto_lint_toggled)
	toolbar.add_child(_auto_lint_check)

	_auto_format_check = CheckBox.new()
	_auto_format_check.text = "Format on Save"
	_auto_format_check.button_pressed = auto_format_on_save
	_auto_format_check.toggled.connect(_on_auto_format_toggled)
	toolbar.add_child(_auto_format_check)

	toolbar.add_child(VSeparator.new())

	# Mode switch.
	_mode_switch = OptionButton.new()
	_mode_switch.add_item("Native", 0)
	_mode_switch.add_item("CLI", 1)
	_mode_switch.tooltip_text = "Switch between native GDExtension and CLI backend"
	if not _gdextension_available:
		_mode_switch.set_item_disabled(0, true)
		_mode_switch.set_item_text(0, "Native (not available)")
	_mode_switch.selected = 0 if _use_gdextension else 1
	_mode_switch.item_selected.connect(_on_mode_switched)
	toolbar.add_child(_mode_switch)

	# Spacer to push status to the right.
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	toolbar.add_child(spacer)

	_status_label = Label.new()
	_status_label.text = "Ready"
	_status_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	toolbar.add_child(_status_label)

	# --- CLI settings row (shown when CLI backend is selected) ---
	_cli_row = HBoxContainer.new()
	_cli_row.add_theme_constant_override("separation", 6)
	_cli_row.visible = not _use_gdextension
	add_child(_cli_row)

	var path_label := Label.new()
	path_label.text = "CLI Binary:"
	_cli_row.add_child(path_label)

	_binary_path_edit = LineEdit.new()
	_binary_path_edit.text = _gdstyle_path
	_binary_path_edit.tooltip_text = "Path to the gdstyle CLI binary"
	_binary_path_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_binary_path_edit.text_submitted.connect(_on_binary_path_changed)
	_cli_row.add_child(_binary_path_edit)

	_download_button = Button.new()
	_download_button.text = "Download"
	_download_button.tooltip_text = "Download the gdstyle CLI binary from GitHub Releases"
	_download_button.pressed.connect(_download_binary)
	_cli_row.add_child(_download_button)

	# --- Results tree ---
	_tree = Tree.new()
	_tree.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_tree.columns = 4
	_tree.set_column_title(0, "File")
	_tree.set_column_title(1, "Line")
	_tree.set_column_title(2, "Rule")
	_tree.set_column_title(3, "Message")
	_tree.set_column_titles_visible(true)
	_tree.set_column_expand(0, false)
	_tree.set_column_expand(1, false)
	_tree.set_column_expand(2, false)
	_tree.set_column_expand(3, true)
	_tree.set_column_custom_minimum_width(0, 200)
	_tree.set_column_custom_minimum_width(1, 80)
	_tree.set_column_custom_minimum_width(2, 260)
	_tree.set_column_custom_minimum_width(3, 300)
	_tree.set_column_clip_content(0, true)
	_tree.set_column_clip_content(2, true)
	_tree.item_activated.connect(_on_item_activated)
	_tree.allow_rmb_select = true
	_tree.item_mouse_selected.connect(_on_item_rmb_selected)
	add_child(_tree)

	_context_menu = PopupMenu.new()
	_context_menu.id_pressed.connect(_on_context_menu_id_pressed)
	add_child(_context_menu)


func _on_mode_switched(index: int) -> void:
	if index == 0 and _gdextension_available:
		_use_gdextension = true
	else:
		_use_gdextension = false
	if _cli_row:
		_cli_row.visible = not _use_gdextension


# --- Editor buffer helpers ---


## Get the CodeEdit for the currently open script (or null).
func _get_code_edit() -> CodeEdit:
	if not editor_interface:
		return null
	var script_editor := editor_interface.get_script_editor()
	if not script_editor:
		return null
	var current_editor := script_editor.get_current_editor()
	if not current_editor:
		return null
	return current_editor.get_base_editor()


## Get the text currently shown in the editor for the open script.
func _get_editor_text() -> String:
	var code_edit := _get_code_edit()
	if code_edit:
		return code_edit.text
	return ""


## Collapse runs of 3+ blank lines down to 2.
func _collapse_blank_lines(text: String) -> String:
	var lines := text.split("\n")
	var result: PackedStringArray = []
	var consecutive_empty := 0
	for line in lines:
		if line.strip_edges().is_empty():
			consecutive_empty += 1
			if consecutive_empty <= 2:
				result.append(line)
		else:
			consecutive_empty = 0
			result.append(line)
	return "\n".join(result)


## Replace the editor text, preserving caret and scroll position.
func _set_editor_text(new_text: String) -> void:
	var code_edit := _get_code_edit()
	if not code_edit:
		return
	var saved_line := code_edit.get_caret_line()
	var saved_col := code_edit.get_caret_column()
	var saved_scroll := code_edit.scroll_vertical
	code_edit.text = new_text
	code_edit.set_caret_line(saved_line)
	code_edit.set_caret_column(saved_col)
	code_edit.scroll_vertical = saved_scroll


## Write source text to a temp file, run a callback, then clean up.
## Returns the CLI stdout text.
func _with_temp_file(source: String, file_name: String, args: Array) -> String:
	var tmp_dir := OS.get_cache_dir()
	var tmp_path := tmp_dir.path_join("gdstyle_tmp_" + file_name)
	var file := FileAccess.open(tmp_path, FileAccess.WRITE)
	if not file:
		return ""
	file.store_string(source)
	file.close()

	var full_args: Array = args.duplicate()
	full_args.append(tmp_path)
	var output: Array = []
	OS.execute(_gdstyle_path, full_args, output, true)

	# Read back (for fmt which modifies in place).
	var result_text := ""
	var result_file := FileAccess.open(tmp_path, FileAccess.READ)
	if result_file:
		result_text = result_file.get_as_text()
		result_file.close()

	DirAccess.remove_absolute(tmp_path)

	# Also return stdout for JSON parsing.
	var stdout_text := ""
	for line in output:
		stdout_text += str(line)
	# Store stdout in a way the caller can access: pack both into a dict.
	# Actually, just return stdout. Caller reads result_text from temp if needed.
	return stdout_text


# --- Public API ---


func lint_project() -> void:
	if _use_gdextension:
		_lint_project_native()
	else:
		_run_gdstyle_cli([ProjectSettings.globalize_path("res://")])


func lint_current_file() -> void:
	var path := _get_current_gd_path()
	if path.is_empty():
		return

	var source := _get_editor_text()
	if source.is_empty():
		return

	_load_nearest_config(path)

	_set_status("Linting...")
	_diagnostics.clear()
	_tree.clear()

	if _use_gdextension:
		var results: Array = _native_linter.lint_source(source, path.get_file())
		for d in results:
			_diagnostics.append(d)
	else:
		# Write buffer to temp file, lint it, parse JSON results.
		var tmp_dir := OS.get_cache_dir()
		var tmp_path := tmp_dir.path_join("gdstyle_tmp_" + path.get_file())
		var file := FileAccess.open(tmp_path, FileAccess.WRITE)
		if not file:
			_set_status("Failed to create temp file")
			return
		file.store_string(source)
		file.close()

		var output: Array = []
		var exit_code := OS.execute(_gdstyle_path, ["check", "--format", "json", tmp_path], output, true)
		DirAccess.remove_absolute(tmp_path)

		if exit_code == -1:
			_set_status("Binary not found. Click Download or set the path.")
			return

		var stdout_text := ""
		for line in output:
			stdout_text += str(line)

		if not stdout_text.strip_edges().is_empty():
			var json := JSON.new()
			if json.parse(stdout_text) == OK and json.data is Array:
				_diagnostics.assign(json.data)

	# Patch file paths: diagnostics reference the temp file or bare filename,
	# replace with the real res:// path so go-to-line and fix work.
	for d: Dictionary in _diagnostics:
		d["file"] = path

	_populate_tree()
	_update_status_counts()


func fix_current_file() -> void:
	var path := _get_current_gd_path()
	if path.is_empty():
		return

	var source := _get_editor_text()
	if source.is_empty():
		return

	_load_nearest_config(path)

	var fixed_source := ""

	if _use_gdextension:
		fixed_source = _native_linter.fix_source_unsafe(source, path.get_file())
	else:
		# Write to temp, run --unsafe-fix, read back.
		var tmp_dir := OS.get_cache_dir()
		var tmp_path := tmp_dir.path_join("gdstyle_tmp_" + path.get_file())
		var file := FileAccess.open(tmp_path, FileAccess.WRITE)
		if not file:
			_set_status("Failed to create temp file")
			return
		file.store_string(source)
		file.close()

		OS.execute(_gdstyle_path, ["check", "--unsafe-fix", tmp_path], [], true)

		var result_file := FileAccess.open(tmp_path, FileAccess.READ)
		if result_file:
			fixed_source = result_file.get_as_text()
			result_file.close()
		DirAccess.remove_absolute(tmp_path)

	if not fixed_source.is_empty() and fixed_source != source:
		_set_editor_text(fixed_source)
		_set_status("Applied all fixes: %s" % path.get_file())
	else:
		_set_status("No fixes to apply: %s" % path.get_file())

	lint_current_file()


func format_project() -> void:
	_set_status("Formatting...")
	var gd_files := _find_gd_files("res://")
	var changed_count := 0
	for file_path in gd_files:
		if _format_single_file(file_path):
			changed_count += 1
	_set_status("Formatted %d file%s" % [changed_count, "s" if changed_count != 1 else ""])


func format_current_file() -> void:
	var path := _get_current_gd_path()
	if path.is_empty():
		return

	var source := _get_editor_text()
	if source.is_empty():
		return

	_load_nearest_config(path)

	var formatted := ""

	if _use_gdextension:
		formatted = _native_linter.format_source(source)
	else:
		# Write to temp, run fmt, read back.
		var tmp_dir := OS.get_cache_dir()
		var tmp_path := tmp_dir.path_join("gdstyle_tmp_" + path.get_file())
		var file := FileAccess.open(tmp_path, FileAccess.WRITE)
		if not file:
			_set_status("Failed to create temp file")
			return
		file.store_string(source)
		file.close()

		OS.execute(_gdstyle_path, ["fmt", tmp_path], [], true)

		var result_file := FileAccess.open(tmp_path, FileAccess.READ)
		if result_file:
			formatted = result_file.get_as_text()
			result_file.close()
		DirAccess.remove_absolute(tmp_path)

	if not formatted.is_empty() and formatted != source:
		_set_editor_text(formatted)
		_set_status("Formatted: %s" % path.get_file())
	else:
		_set_status("No changes: %s" % path.get_file())


## Format on save operates on disk (Godot already wrote the file).
func format_file_on_disk(res_path: String) -> void:
	if _use_gdextension:
		_native_linter.format_res_file(res_path)
	else:
		var fs_path := ProjectSettings.globalize_path(res_path)
		OS.execute(_gdstyle_path, ["fmt", fs_path], [], true)

	# The formatter rewrote the file on disk, but if it is the script
	# currently open in the editor the in-memory buffer is now stale.
	# The next editor save would overwrite the formatted file, so re-sync
	# the editor buffer from the formatted disk content.
	if res_path == _open_script_path():
		var formatted := FileAccess.get_file_as_string(res_path)
		if not formatted.is_empty() and formatted != _get_editor_text():
			_set_editor_text(formatted)


## The resource path of the script currently open in the editor, or "" if
## none. Unlike `_get_current_gd_path`, this is side-effect-free (no status
## messages). Safe to call from the save hook.
func _open_script_path() -> String:
	if not editor_interface:
		return ""
	var script_editor := editor_interface.get_script_editor()
	if not script_editor:
		return ""
	var current_script := script_editor.get_current_script()
	if not current_script:
		return ""
	return current_script.resource_path


func _get_current_gd_path() -> String:
	if not editor_interface:
		_set_status("No editor interface available")
		return ""
	var script_editor := editor_interface.get_script_editor()
	if not script_editor:
		_set_status("No script editor open")
		return ""
	var current_script := script_editor.get_current_script()
	if not current_script:
		_set_status("No script open")
		return ""
	var path := current_script.resource_path
	if not path.ends_with(".gd"):
		_set_status("Current file is not a GDScript file")
		return ""
	return path


## Format a file on disk (for project-wide formatting).
func _format_single_file(res_path: String) -> bool:
	if _use_gdextension:
		return _native_linter.format_res_file(res_path)
	else:
		var fs_path := ProjectSettings.globalize_path(res_path)
		var output: Array = []
		OS.execute(_gdstyle_path, ["fmt", fs_path], output, true)
		return true


# --- GDExtension backend ---


func _lint_project_native() -> void:
	_set_status("Linting...")
	_diagnostics.clear()
	_tree.clear()

	var gd_files := _find_gd_files("res://")
	for file_path in gd_files:
		var results: Array = _native_linter.lint_res_file(file_path)
		for d in results:
			_diagnostics.append(d)

	_populate_tree()
	_update_status_counts()


func _find_gd_files(dir_path: String) -> Array[String]:
	var files: Array[String] = []
	var dir := DirAccess.open(dir_path)
	if not dir:
		return files
	dir.list_dir_begin()
	var file_name := dir.get_next()
	while file_name != "":
		var full_path := dir_path.path_join(file_name)
		if dir.current_is_dir():
			# Skip hidden dirs, .godot, and addons.
			if not file_name.begins_with(".") and file_name != "addons":
				files.append_array(_find_gd_files(full_path))
		elif file_name.ends_with(".gd"):
			files.append(full_path)
		file_name = dir.get_next()
	return files


# --- CLI backend ---


func _run_gdstyle_cli(args: Array) -> void:
	_set_status("Linting...")
	_diagnostics.clear()
	_tree.clear()

	var full_args: Array = ["check", "--format", "json"]
	full_args.append_array(args)

	var output: Array = []
	var exit_code := OS.execute(_gdstyle_path, full_args, output, true)

	if exit_code == -1:
		_set_status("Binary not found. Click Download or set the path.")
		push_error("gdstyle: Could not execute '%s'. Is it installed?" % _gdstyle_path)
		return

	var stdout_text: String = ""
	for line in output:
		stdout_text += str(line)

	if stdout_text.strip_edges().is_empty():
		_set_status("No issues found")
		return

	var json := JSON.new()
	if json.parse(stdout_text) != OK:
		_set_status("Failed to parse gdstyle output")
		push_error("gdstyle: JSON parse error: %s" % json.get_error_message())
		return

	var parsed = json.data
	if not parsed is Array:
		_set_status("Unexpected gdstyle output format")
		return

	_diagnostics.assign(parsed)
	_populate_tree()
	_update_status_counts()


# --- Auto-download ---


func _download_binary() -> void:
	_set_status("Downloading gdstyle...")
	_download_button.disabled = true

	# Determine platform.
	var os_name := OS.get_name().to_lower()
	var arch := Engine.get_architecture_name()
	var artifact_name := ""
	var binary_name := ""

	if os_name == "linux":
		artifact_name = "gdstyle-x86_64-unknown-linux-gnu.tar.gz"
		binary_name = "gdstyle"
	elif os_name == "macos":
		if "arm" in arch or "aarch64" in arch:
			artifact_name = "gdstyle-aarch64-apple-darwin.tar.gz"
		else:
			artifact_name = "gdstyle-x86_64-apple-darwin.tar.gz"
		binary_name = "gdstyle"
	elif os_name == "windows":
		artifact_name = "gdstyle-x86_64-pc-windows-msvc.zip"
		binary_name = "gdstyle.exe"
	else:
		_set_status("Unsupported platform: %s" % os_name)
		_download_button.disabled = false
		return

	# Get the latest release download URL.
	var url := "https://github.com/%s/releases/latest/download/%s" % [GITHUB_REPO, artifact_name]

	# Download using HTTPRequest.
	var http := HTTPRequest.new()
	add_child(http)

	var download_path := ProjectSettings.globalize_path(BIN_DIR).path_join(artifact_name)

	# Ensure bin directory exists.
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(BIN_DIR))

	http.download_file = download_path
	http.request_completed.connect(
		func(result: int, response_code: int, _headers: PackedStringArray, _body: PackedByteArray):
			http.queue_free()

			if result != HTTPRequest.RESULT_SUCCESS or response_code != 200:
				_set_status("Download failed (HTTP %d). Check releases." % response_code)
				_download_button.disabled = false
				push_error("gdstyle: Download failed from %s (result=%d, code=%d)" % [url, result, response_code])
				return

			# Extract the binary.
			var bin_dir := ProjectSettings.globalize_path(BIN_DIR)
			var extract_args: Array
			if artifact_name.ends_with(".tar.gz"):
				extract_args = ["tar", "xzf", download_path, "-C", bin_dir]
			else:
				extract_args = ["7z", "x", download_path, "-o" + bin_dir, "-y"]

			# Use tar/7z to extract.
			var extract_output: Array = []
			OS.execute(extract_args[0], extract_args.slice(1), extract_output, true)

			# Clean up the archive.
			DirAccess.remove_absolute(download_path)

			# Update the binary path.
			var final_path := bin_dir.path_join(binary_name)
			if os_name != "windows":
				# Make executable.
				OS.execute("chmod", ["+x", final_path], [], true)

			_gdstyle_path = final_path
			_binary_path_edit.text = _gdstyle_path
			_save_settings()
			_download_button.disabled = false
			_set_status("Downloaded successfully")
	)

	var err := http.request(url)
	if err != OK:
		_set_status("Failed to start download")
		_download_button.disabled = false
		http.queue_free()


# --- Shared UI logic ---


func _populate_tree() -> void:
	_tree.clear()
	var root := _tree.create_item()

	for diag: Dictionary in _diagnostics:
		var item := _tree.create_item(root)

		var file_path: String = diag.get("file", "")
		var line_num: int = diag.get("line", 0)
		# CLI backend uses nested "span" dict, GDExtension flattens it.
		if diag.has("span") and diag["span"] is Dictionary:
			line_num = diag["span"].get("line", 0)
		var rule: String = diag.get("rule", "")
		var message: String = diag.get("message", "")
		var severity: String = diag.get("severity", "warn")

		var file_name := file_path.get_file()
		item.set_text(0, file_name)
		item.set_tooltip_text(0, file_path)
		item.set_text(1, str(line_num))
		item.set_tooltip_text(1, str(line_num))
		item.set_text(2, rule)
		item.set_tooltip_text(2, rule)
		item.set_text(3, message)
		item.set_tooltip_text(3, message)

		if severity == "error":
			item.set_custom_color(3, Color(1.0, 0.4, 0.4))
		else:
			item.set_custom_color(3, Color(1.0, 0.85, 0.4))

		item.set_metadata(0, file_path)
		item.set_metadata(1, line_num)
		item.set_metadata(2, rule)
		# GDExtension backend provides flat "has_fix", CLI provides nested "fix" dict.
		var has_fix: bool = diag.get("has_fix", false) or diag.has("fix")
		item.set_metadata(3, has_fix)


func _update_status_counts() -> void:
	var warning_count := 0
	var error_count := 0
	for diag: Dictionary in _diagnostics:
		if diag.get("severity", "") == "error":
			error_count += 1
		else:
			warning_count += 1

	var parts: Array[String] = []
	if error_count > 0:
		parts.append("%d error%s" % [error_count, "s" if error_count != 1 else ""])
	if warning_count > 0:
		parts.append("%d warning%s" % [warning_count, "s" if warning_count != 1 else ""])
	if parts.is_empty():
		_set_status("No issues found")
	else:
		_set_status(", ".join(parts))


func _on_item_rmb_selected(position: Vector2, mouse_button_index: int) -> void:
	if mouse_button_index != MOUSE_BUTTON_RIGHT:
		return
	var selected := _tree.get_selected()
	if not selected:
		return

	var has_fix: bool = selected.get_metadata(3)

	_context_menu.clear()
	_context_menu.add_item("Go to line", 0)
	_context_menu.add_separator()
	_context_menu.add_item("Fix", 1)
	_context_menu.set_item_disabled(_context_menu.get_item_index(1), not has_fix)
	if has_fix:
		_context_menu.set_item_tooltip(_context_menu.get_item_index(1), "Apply auto-fix for this violation")
	else:
		_context_menu.set_item_tooltip(_context_menu.get_item_index(1), "No auto-fix available for this rule")

	_context_menu.position = Vector2i(_tree.get_screen_position() + position)
	_context_menu.reset_size()
	_context_menu.popup()


func _on_context_menu_id_pressed(id: int) -> void:
	var selected := _tree.get_selected()
	if not selected:
		return
	if id == 0:
		_on_item_activated()
	elif id == 1:
		_fix_selected_item(selected)


func _fix_selected_item(item: TreeItem) -> void:
	var file_path: String = item.get_metadata(0)
	var line_num: int = item.get_metadata(1)
	var rule: String = item.get_metadata(2)

	if file_path.is_empty():
		return

	var source := _get_editor_text()
	if source.is_empty():
		return

	var res_path := file_path
	if not res_path.begins_with("res://"):
		var project_dir := ProjectSettings.globalize_path("res://")
		if file_path.begins_with(project_dir):
			res_path = "res://" + file_path.substr(project_dir.length())
		else:
			push_warning("gdstyle: Cannot resolve '%s' to a res:// path" % file_path)
			return

	_load_nearest_config(res_path)

	var fixed := ""

	if _use_gdextension:
		# Native single-fix: lints in memory, applies the matching diagnostic's
		# replacements, returns the patched source. No temp file, no CLI hop.
		fixed = _native_linter.fix_one_in_source(source, res_path.get_file(), line_num, rule)
	else:
		# CLI fallback: write source to a temp file, lint with --select, parse
		# the JSON, apply the replacements in memory.
		var tmp_dir := OS.get_cache_dir()
		var tmp_path := tmp_dir.path_join("gdstyle_tmp_" + res_path.get_file())
		var file := FileAccess.open(tmp_path, FileAccess.WRITE)
		if not file:
			_set_status("Fix failed: cannot create temp file")
			return
		file.store_string(source)
		file.close()

		var output: Array = []
		OS.execute(_gdstyle_path, ["check", "--format", "json", "--select", rule, tmp_path], output, true)
		DirAccess.remove_absolute(tmp_path)

		var stdout_text := ""
		for line in output:
			stdout_text += str(line)

		var json := JSON.new()
		if json.parse(stdout_text) != OK or not json.data is Array:
			_set_status("Fix failed: could not parse diagnostics")
			return

		var match_diag: Dictionary = {}
		for d: Dictionary in json.data:
			var d_line: int = d.get("line", 0)
			if d.has("span") and d["span"] is Dictionary:
				d_line = d["span"].get("line", 0)
			if d_line == line_num and d.get("rule", "") == rule and d.has("fix"):
				match_diag = d
				break

		if match_diag.is_empty():
			_set_status("No fixable match at line %d" % line_num)
			return

		var fix_data: Dictionary = match_diag["fix"]
		var replacements: Array = fix_data.get("replacements", [])
		replacements.sort_custom(func(a, b): return a["offset"] > b["offset"])

		fixed = source
		for repl: Dictionary in replacements:
			var offset: int = repl["offset"]
			var length: int = repl["length"]
			var new_text: String = repl["new_text"]
			fixed = fixed.substr(0, offset) + new_text + fixed.substr(offset + length)

	if fixed.is_empty() or fixed == source:
		_set_status("No fixable match at line %d" % line_num)
		return

	_set_editor_text(fixed)
	_set_status("Fixed: %s at line %d" % [rule, line_num])

	lint_current_file()


func _on_item_activated() -> void:
	var selected := _tree.get_selected()
	if not selected:
		return

	var file_path: String = selected.get_metadata(0)
	var line_num: int = selected.get_metadata(1)

	if file_path.is_empty():
		return

	# Convert filesystem path to res:// path if needed.
	var res_path := file_path
	if not res_path.begins_with("res://"):
		var project_dir := ProjectSettings.globalize_path("res://")
		if file_path.begins_with(project_dir):
			res_path = "res://" + file_path.substr(project_dir.length())
		else:
			push_warning("gdstyle: Cannot resolve '%s' to a res:// path" % file_path)
			return

	var script := load(res_path)
	if script:
		editor_interface.edit_resource(script)
		await get_tree().process_frame
		var script_editor := editor_interface.get_script_editor()
		if script_editor:
			script_editor.goto_line(line_num - 1)


# --- Settings ---


func _on_auto_lint_toggled(pressed: bool) -> void:
	auto_lint_on_save = pressed
	_save_settings()


func _on_auto_format_toggled(pressed: bool) -> void:
	auto_format_on_save = pressed
	_save_settings()


func _on_binary_path_changed(new_path: String) -> void:
	_gdstyle_path = new_path.strip_edges()
	_save_settings()


func _set_status(text: String) -> void:
	if _status_label:
		_status_label.text = text


## Find the nearest `gdstyle.toml` / `.gdstyle.toml` by walking up from the
## file's directory, and load it into the native linter. Falls back to
## defaults when none is found. The CLI applies the same search itself; this
## method just makes the GDExtension backend consistent with it so subfolder
## overrides (e.g. inside a vendored addon) are respected.
func _load_nearest_config(res_path: String) -> void:
	if not _use_gdextension or _native_linter == null:
		return
	var dir := res_path.get_base_dir()
	while not dir.is_empty():
		for name in ["gdstyle.toml", ".gdstyle.toml"]:
			var candidate := dir.path_join(name)
			if FileAccess.file_exists(candidate):
				_native_linter.load_config_res(candidate)
				return
		if dir == "res://":
			break
		var parent := dir.get_base_dir()
		if parent == dir:
			break
		dir = parent
	_native_linter.reset_config()


func _save_settings() -> void:
	# Skip the write while we're populating the UI from disk. Without
	# this guard the toggled handlers fired from _load_settings would
	# call back into here and clash with the still-open READ handle.
	if _loading_settings:
		return
	var settings := {
		"binary_path": _gdstyle_path,
		"auto_lint_on_save": auto_lint_on_save,
		"auto_format_on_save": auto_format_on_save,
	}
	var file := FileAccess.open(SETTINGS_PATH, FileAccess.WRITE)
	if file == null:
		push_warning(
			"gdstyle: could not open %s for write (err=%d)" % [
				SETTINGS_PATH,
				FileAccess.get_open_error(),
			]
		)
		return
	file.store_string(JSON.stringify(settings, "\t"))
	file.close()


func _load_settings() -> void:
	if not FileAccess.file_exists(SETTINGS_PATH):
		return
	var file := FileAccess.open(SETTINGS_PATH, FileAccess.READ)
	if file == null:
		push_warning(
			"gdstyle: could not open %s for read (err=%d)" % [
				SETTINGS_PATH,
				FileAccess.get_open_error(),
			]
		)
		return
	var text := file.get_as_text()
	file.close()
	var json := JSON.new()
	if json.parse(text) != OK:
		return
	# Guard the UI writes so any signal handlers that fire from these
	# assignments don't try to rewrite settings.json before we return.
	# `set_pressed_no_signal` on the checkboxes also avoids the toggled
	# round-trip; the flag is belt-and-suspenders against future
	# handlers added on `_binary_path_edit` or similar.
	_loading_settings = true
	_apply_loaded_settings(json.data)
	_loading_settings = false


func _apply_loaded_settings(data: Dictionary) -> void:
	if data.has("binary_path"):
		_gdstyle_path = data["binary_path"]
		if _binary_path_edit:
			_binary_path_edit.text = _gdstyle_path
	if data.has("auto_lint_on_save"):
		auto_lint_on_save = data["auto_lint_on_save"]
		if _auto_lint_check:
			_auto_lint_check.set_pressed_no_signal(auto_lint_on_save)
	if data.has("auto_format_on_save"):
		auto_format_on_save = data["auto_format_on_save"]
		if _auto_format_check:
			_auto_format_check.set_pressed_no_signal(auto_format_on_save)

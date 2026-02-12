# Changelog

All notable changes to this project will be documented in this file.

Surfer is currently unstable and all 0.x releases are expected to contain
breaking changes. Releases are mainly symbolic and are done on a six-week
release cycle. Every six weeks, the current master branch is tagged and
released as a new version.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.6.0] - 2026-02-12

## Added

- Color translators that set the wave color based on RGB, YCbCr or Grayscale.
- Waveform events are now supported.
- Translator methods that directly return floats.
- Commands: `surver_select_file` and `surver_switch_file` to change files without and with keeping waveform variables.
- Keyboard shortcut to add a divider: `d`.
- Buttons to expand/collapse the hierarchy.
- It is possible to deselect a scope, leading to that the root scope is selected.
- New commands: `scope_select_root` and `stream_select_root` the selects the root scope/stream.
- Optional and configurable icons for variable and scope types.
- Many keyboard bindings are now configurable.
- Mapping translator that can convert values to text using a configuration file.
- WCP command `set_cursor` for positioning the cursor.
- Translator plugins can now access a configuration directory.

## Changed

- There is a separate crate, `surfer-wcp`, to assist in connecting to Surfer via WCP.
- A workaround for an egui issue has been removed since it was fixed in egui. This should lead to cursors ending up close to where you actually clicked and better aligned with transitions when drawing. If something looks worse, please open an issue.

## Fixed

* Row computation for transactions where several transactions start at the same time.
* Analog drawing is faster thanks to direct conversion to floats.
* It is again possible to start surfer with a surver URL.
* Logging is working again in the WASM version.
* Adding a scope corresponding to a compound variable (array, record, etc) as a group will set the variables in that group to have local names.
* More variable context menu entries operate on all selected variables.
* Real-valued parameters are now handled as parameters.
* Save state file dialog behaves better on macOS.
* Bug where viewports got inverted when loading a state file.
* String decoding in FST and VCD (due to improvements in wellen).
* Crash when file was changed multiple times on disk, caused by the UI not refreshed.
* The drawing of mouse gesture and measure widgets is now correctly positioned in y-direction even when the timeline is visible.
* Several issues related to vertical scroll of the waveform view, especially related to items with different heights.
* Bug where different array items were reloaded as the same.
* It is now possible to select a specific array item for a command.

## [0.5.0] - 2025-12-19

## Added

* Analog drawing.
* Surver supports multiple files. The file names can also be provided in a text file.
* Markers can be added through WCP.
* It is possible to select which value is displayed when the cursor is on a transition. Configuration parameter `transition_value` which can be "Next" (default), "Previous", or "Both". Corresponding Settings menu entry available.
* The variable filter textbox tooltip displays the error message in case of regex errors.
* Context menu for empty part of the variable name and variable value columns that can add dividers and timelines.
* Drawing mode inspired by Dinotrace: vectors with all zeros are drawn without the top line and with wide bottom line, vectors with all ones are drawn with wide top line. Enable in the menu or by using the configuration parameter `use_dinotrace_style` (bool). Line width for wide lines set by the theme parameter `thick_linewidth` (float).
* Reload in server mode will reload the server side file (if it is modified since last load).
* New configuration parameters:
  * `animation_enabled` (bool) can be used to turn off UI animations, also in the Settings menu.
  * `animation_time` (float) time for UI animation in seconds.
  * `max_url_length` (unsigned integer) max length of an url, only change if behind a proxy that limits it.

## Changed

* Design parameters can now be shown in the tooltip instead or not at all, in addition to Scopes or Variables. New configuration `parameter_display_location` which can be "Scopes", "Variables", "Tooltips", or "None".
* The marker information window renders a bit better (right-aligned times) and can be accessed through the context menu in the name column for markers.

## Fixed

* Surfer is more likely to deal with incorrect mouse/keyboard inputs in a graceful way.
* The North and South mouse gestures are no longer mixed up,
* The variable filter textbox didn't update the error indication when, e.g., deleting all text.
* Signal highlighting on reload with missing variables.
* It is possible to load an arbitrary number of variables from a server connection without getting an invalid URL.
* The waveform height can be changed for multiple selectable variables at once using the context menu.

## Removed

* Configuration parameter `show_parameters_in_scopes`. Use `parameter_display_location` instead.

## [0.4.0] - 2025-11-06

## Added

- New commands `goto_cursor`, `cursor_set`, `marker_set`, `marker_remove`. Markers can be accessed through their names or numbers as `#3` etc.
- It is possible to pan with the right/secondary mouse button.
- Change the relative size of the Scope and Variable lists.
- Fixed-point translators based on index value (fractional bits have negative indices), suitable for `sfixed` VHDL-types etc.
- Add grouping of variables (and groups).
- It is possible to add scopes as groups, possibly recursively.
- Surfer automatically select a corresponding translator for the VHDL types `signed`, `unsigned`, `sfixed`, and `ufixed`.
- It is now possible to drop state files onto Surfer to load them.
- A script, `surfer.sh`, that can be used to easily start Windows Surfer from WSL. Download from repo and follow the instructions in the script.
- It is now possible to copy the variable name of the focused variable by right-clicking on the name.
- Right-click menu for scopes to add variables in scopes.
- `Ctrl+s` now saves the state file.
- If there is a file ending with `.surf.ron` in the current directory, there will be a dialog asking if it should be loaded (configurable to either ask, always load, or never load).
- The variable filter can now filter based on port direction and optionally group based on port direction.
- Measure time by holding shift and left-click (primary mouse button). Configure color and line width using `measure` in theme. Can be changed to not need shift by setting the config `primary_button_drag_behavior` to `Measure` (dragging the cursor will now require shift) or by menu entry.
- The arrow between transactions can now be fully configured, including line width and arrow properties.
- The maximum size of the "X" between vector transitions can be configured using `vector_transition_width´ (default 6).
- It is possible to configure the mouse gestures.
- LoongArch64 (LA64) instruction translator.
- The height of the waves can be modified by right-clicking on the variable name and use the "Height" submenu. The available options can be configured using `waveforms_line_height_multiples`.
- There is now an app_id for use with Wayland to identify the Surfer window: `org.surfer-project.surfer`.
- It is now possible to run a command file/script using the menu, toolbar, or the new commands: `run_command_file` or `run_command_file_from_url`.
- New config of wide signal fill: `wide_opacity`
- Translators can now be written in WASM and loaded at runtime.
- Translators an now translate variable names into source code locations
- Right click now pans the view
- "Expand scope" in variable context menu selects the containing scope and expand the tree.
- Two accessible color themes, one light and one dark, based on Petroff.
- You can now scale the UI with `ctrl/+ and ctrl/-`
- Various changes to the Waveform Control Protocol
    - Add `add_scope_recursive`
    - Add `add_scope`
    - Fix greetings
    - Send waveforms_loaded on loads not triggered by WCP
    - Add sources to waveforms_loaded
    - Require greetings
    - Change the WCP injection API to be more similar to CXXRTL
    - Adds events for drivers and loads to a signal

## Changed

- `surver` now sends data in a compressed format.
- The scope list is sorted.
- `m` now adds a marker (next number), while toggling the menu is changed to `Alt+m`.
- Surfer state-files now ends with `.surf.ron`.
- To drag the cursor, you now need to hold shift.
- BREAKING: Mouse gesture line style moved from config to theme.
- BREAKING: Arrow between transactions can be fully configured, so `relation_arrow` is now a subsection and not a simple color.
- BREAKING: The config settings `autoreload_files` and `auto_load_sibling_state_files` have changed format from `Option<bool>` to an enum with the values `Always`, `Never`, and `Ask`.

## Fixed

- Crash when adding too many viewports/trying to draw zero-sized viewports.
- It is again possible to correctly build Surfer with or without selected features.
- Wellen backend updated with better support for GHW-files and incomplete FST-files among other things.
- Loading files with many variables should now be significantly faster.
- It is no longer possible to zoom in arbitrarily much, avoiding some issues related to that.
- Crash when using automatic time scale and underlying waveform file has very high resolution.
- The mouse gesture zoom in now starts at the exact click location in x-direction.
- Surfer should be more responsible when having an active scope with a large number of variables.
- Time scale is no longer reset after reload.
- Adding many variables (recursive scopes etc) in one go, should be significantly faster.
- Add background to multi-bit signals. (Enabled in themes, default in the light theme)
- The right-most clock edge line is now drawn.
- More settings are saved in the state.
- Crash when zooming before waveform file is fully loaded.

## Removed

## Other

- egui is updated to 0.33. This can lead to minor visual changes, including more clear text rendering. If something looks worse, please report!



## [0.3.0] - 2024-12-20

## Added

- Bumped backend to Wellen 0.13.6
- MIPS translator.
- RV32 and RV64 translators support all unprivileged instructions.
- Parameters now have an icon in the variable list and are drawn in a separate color, `variable_parameter` in the config.
- Custom instruction decoders can be loaded from the config directory.
- It is possible to press tab to expand text in the command prompt.
- Loading and saving states from within the ui has been added/improved.
- Separate server binary, `surver`.
- A number of color-blind friendly themes.
- [FTR](https://github.com/Minres/LWTR4SC) transaction streams are now supported.
- It is now possible to configure the possible UI zoom levels, `zoom_factors` and the default choice, `default_zoom_factor`.
- A link to a web page with all licenses to dependencies is added in the License information dialog.
- Initial [user](https://docs.surfer-project.org/book/) and [API](https://docs.surfer-project.org/surfer/) documentation.
- New configuration parameters `waveforms_line_height` and `waveforms_text_size` to control the height and text size of waveforms, respectively.
- It is now possible to add variables by dragging scopes and variables from the sidebar.
- Add `waves_loaded`, `index_of_name` and `spade_loaded`  to the wasm API
- Add `ViewportStrategy` which allows programmatic smooth scroll when scripting Surfer
- Add `ExpandItem` message which expands the fields of a viewed variable.
- Add `SetConfigFromString` which allows setting a configuration when Surfer is embedded in a webpage.
- `scope_add_recursive` command.
- Dialog will show up when a file is changed on disk, asking for reload. Not yet working on Windows.
- Translators for number of ones, leading/trailing ones/zeros and identical MSBs (sign-bits).
- The mouse gestures can be accessed through ctrl/cmd and primary mouse button (for use, e.g., when no middle mouse button is available).
- Show start and end time of mouse gesture zoom
- Allow mouse gesture zoom with ctrl+left click
- Add a timeline by default

## Changed

- Limit scrolling to always show some of the waveform
- Text color is (often) selected based on highest contrast vs background color. It is selected as one of the two config values `foreground` and `alt_text_color`.
- BREAKING: the `ticks` settings are moved from config to theme.
- Respect direction of arrows in `DrawTextArrow`
- Empty scopes are not shown by default, can be enabled by setting `show_empty_scopes` to `true` or using the menu entry.
- Parameters are shown in the scope list rather than among the variables, can be moved to variables by setting `show_parameters_in_scopes` to `false` or using the menu entry.
- The zoom in mouse gesture now shows the duration of the zoomed region.
- Variables are now sorted when added with `scope_add`

## Fixed

- Crash related to signals not being assigned values at time zero and snapping.
- Loading VCD files with errors more rarely crashes Surfer. In addition, the log window now pops up in case of an error.
- Empty scopes are no longer expandable in the tree hierarchy.
- The server can now be accessed in the web/WASM version.
- Translator selection is now deterministic. Earlier, different translators may be selected if more than one was preferred, notably this happened for single-bit enums.
- When variables are added using the `scope_add` command, they are sorted so that the result is identical to selecting the scope and pressing the `+` button.
- Variables with negative indices are now correctly sorted.
- Remove lingering marker line when deleting a marker

## Removed

## Other

- egui 0.30 is now used. This changes the shadowing in the UI and fixes an issue with scaling the UI in web browsers.

## [0.2.0] - 2024-05-31

## Added
- It is possible to disable the performance plot, by disabling the feature `performance_plot`, reducing the binary size with about 250kB.
- Clicking in the overview widget will now center the (primary, see multiple viewports) view to that position. It is also possible to drag the mouse while pressed.
- Allow injecting [Messages](https://gitlab.com/surfer-project/surfer/-/blob/main/src/message.rs?ref_type=heads#L27) into Surfer via `window.inject_message` in Javascript. Note that the Message enum, at least for now, may change at any time.
- Added some commands to the command prompt which were available only in the GUI before.
- Added a context menu where cursors can be added.
- Added an alternative tree-like hierarchy view to the left sidebar.
- Added an alternative text color config, currently used for marker boxes, `alt_text_color`.
- Multiple viewports are now supported. These can be added with `viewport_add`. The separator is configurable using the `viewport_separator` config value.
- Added jump to next and previous transition.
- The value of the selected variable at the cursor can now be copied to the clipboard, either using the toolbar, variable context menu, or standard keyboard short cut Ctrl/Cmd+c. Using the `copy_value` command, the value of any variable can be copied.
- There is now experimental support for GHW-files.
- Added a license window to display the license text for Surfer. Licenses for used crates are missing though.
- Added enum translator.
- The variable name filtering can now be case insensitive.
- Auto time scale that selects the largest time unit possible for each tick without having to reside to fractional numbers.
- Themes can be changed using the `view/theme` GUI options and with a command.
- It is now possible to drag-and-drop reorder variables in the waveform view.
- There is now a pre-built binary for macos-aarch64.
- Added an experimental client-server approach for running surfer at a remote location. Start with `surfer server --file=waveformfile.vcd/fst/ghw` where the file exists and follow the instructions in the output.
- Added undo/redo functionality
- The port direction is (optionally, but on by default) shown for variables in the variable list.
- New RISC-V instruction decoder with support for RV32IMAFD.

## Changed

- Renamed `cursors` to `markers` to differentiate the named and numbered *markers* from the *cursor* that moves with clicks.
- egui is updated to version 0.25.
- Icons are changed from Material Design to Remix Icons.
- Display scopes and variables while loading the variable change data and bring back progress bar when loading
- Translators that do not match the required word length for a variable are now not removed, but put in the "Not recommended" submenu. While there is often no reason to select a not recommended translator, the change leads to that variables that changes word length are not removed during a reload.
- The progress bar when loading waveforms has been moved to the status bar.

## Fixed
- Ticks do not longer disappear or become blurry at certain zoom levels.
- Files that do not initialize variables at time 0 now works.
- Transitions are no longer drawn for false transitions, e.g., from 0 to 0.
- Fixed anti-aliasing for variables which are mostly 1.
- The alternate background now fully covers the variable value column.
- Top-level variables that are not in a scope are now visible.
- The Cmd-key is now used on Mac (instead of Ctrl).
- Variable name filtering is faster (but adding lots of variables to the list view still takes a long time).
- Screen scaling has been improved so that it works on, e.g., HiDPI screens.
- Startup commands can now contain `load_waves` and friends.
- Copies of signals can now have different translators.
- Added rising edge markers to clock signals.

## Removed
- Buttons for adding divider and time at the bottom of the variable list is removed. Use the toolbar instead.

## Other
- There is now a VS Code [extension](https://marketplace.visualstudio.com/items?itemName=surfer-project.surfer) that loads the web-version of Surfer when opening a waveform file. [Repo](https://gitlab.com/surfer-project/vscode-extension).
- The minimum rustc version is determined and pinned to 1.75.


## [0.1.0] - 2023-03-07

Initial numbered version


[Unreleased]: https://gitlab.com/surfer-project/surfer/-/compare/v0.6.0...main
[0.6.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.6.0
[0.5.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.5.0
[0.4.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.4.0
[0.3.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.3.0
[0.2.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.2.0
[0.1.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.1.0

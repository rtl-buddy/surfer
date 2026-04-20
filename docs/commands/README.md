# Commands

To execute a command, press space and type the command. There is fuzzy match support, so it is enough to type parts of the command name and it will display options that matches.

It is also possible to create a command file, extension `.sucl`, and run that. Running a command file can be done from within Surfer using the menu option in the File menu, through the toolbar button, or by typing the command ``run_command_file``. It can also be done using the ``--command-file`` argument when starting Surfer.

## CLI flags for batch/headless use

* ``--command-file <FILE>`` (``-c <FILE>``)

    Load and execute a command file on startup.

* ``--exit-after-commands``

    Exit automatically after all commands from ``--command-file`` have been executed. Useful for scripted PNG export.

* ``--headless``

    Run without a GUI window. Processes ``--command-file`` batch commands and exits. Implies ``--exit-after-commands``. Typical usage:

    ```
    surfer --headless -c export.cmd dump.fst
    ```

Not all commands are available unless a file is loaded. Also, some commands are not available in the WASM-build (browser/VS Code extension).

## Waveform/transaction loading and reloading

* ``load_file <FILE_NAME>``

    Load a file. Note that it works to load a waveform file from a command file.

    <div class="warning">In WASM-builds (web browser/VS Code plugin) it is not possible to open a file due to file access restrictions. Use <tt>load_url</tt>.</div>


* ``switch_file <FILE_NAME>``

    Load file, but keep waveform view.

* ``load_url <URL>``

    Load a URL.

* ``reload``

    Reload the current file. Does not work in a web browser.

* ``remove_unavailable``

    Remove variables that are not longer present in the reloaded/switched file.

## State files

* ``load_state <FILE_NAME>``
* ``save_state``
* ``save_state_as``

## Command files

* ``run_command_file <FILE_NAME>`` (not on WASM)

    Run the commands in the given file.

    <div class="warning">In WASM-builds (web browser/VS Code plugin) it is not possible to run another command file from a command file due to file access restrictions.</div>

* ``run_command_file_from_url <URL>``

## Add variable/transaction items

* ``scope_add <SCOPE_NAME>``, ``stream_add``

    Add all variables in the specified scope to the waveform display.

* ``scope_add_recursive <SCOPE_NAME>``

    Add all variables in the specified scope and from all sub-scopes to the waveform display.

    <div class="warning">Adding large hierarchies with a large number of variables can freeze surfer for a significant amount of time.</div>

* ``scope_add_as_group <SCOPE_NAME>``

    Add all variables in the specified scope to the waveform display in a newly created group of the same name.

* ``scope_add_as_group_recursive <SCOPE_NAME>``

    Add all variables in the specified scope and all sub-scopes to the waveform display in a newly created groups nested.

    <div class="warning">Adding large hierarchies with a large number of variables can freeze surfer for a significant amount of time.</div>

* ``variable_add <FULL_VARIABLE_NAME>``, ``generator_add  <FULL_GENERATOR_NAME>``

    Add a variable/generator using the full path, including scopes/streams.

* ``scope_select <SCOPE_NAME>``, ``stream_select <STREAM_NAME>``

    Select a scope/stream to be active (shown in the side panel).

* ``variable_add_from_scope <VARIABLE_NAME>``, ``generator_add_from_stream <GENERATOR_NAME>``

    Add variable/generator from currently selected scope/stream.

## Add other items

* ``divider_add <NAME>``

  Add a divider with the given name.

* ``timeline_add``

## Groups

* ``group_marked``

    Add selected variables/items to a new group.

* ``group_dissolve``
* ``group_fold_recursive``
* ``group_unfold_recursive``
* ``group_fold_all``
* ``group_unfold_all``

## Controlling item appearance

* ``item_focus``
* ``item_set_color <COLOR_NAME>``
* ``item_set_background_color <COLOR_NAME>``
* ``item_set_format <FORMAT_NAME>``
* ``item_unset_color``

  Reset to default color.

* ``item_unset_background_color``

  Reset to default background color.

* ``item_unfocus``

  Remove focus from currently focused item.

* ``item_rename``
* ``theme_select <THEME_NAME>``

    Apply a named theme (e.g. ``light``, ``dark``, ``light+``, ``dark+``).

## Signal name and value columns

* ``name_col_width <PIXELS>``

    Set the signal-name column to a fixed pixel width.

* ``name_col_fit``

    Auto-size the signal-name column to fit the widest displayed name. Computed from font metrics at render time.

* ``value_col_width <PIXELS>``

    Set the signal-value column to a fixed pixel width.

* ``value_col_fit``

    Auto-size the signal-value column to fit the widest value currently shown at the cursor. Call after ``cursor_set`` so values are available.

## Navigation

* ``zoom_fit``

  Zoom to display the full simulation.

* ``zoom_in``
* ``zoom_out``
* ``zoom_range <START> <END>``

  Zoom the viewport to the given time range (in simulation time units).
* ``scroll_to_start``,  ``goto_start``
* ``scroll_to_end``, ``goto_end``
* ``transition_next``

  Move cursor to next transition of focused item. Scroll if not visible.

* ``transition_previous``

  Move cursor to previous transition of focused item. Scroll if not visible.

* ``transaction_next``
* ``transaction_prev``

## UI control

* ``show_controls``
* ``show_mouse_gestures``

  Show mouse gesture help window.

* ``show_quick_start``
* ``show_logs``

  Show log window.

* ``toggle_menu``

  Toggle visibility of menu. If not visible, there will be a burger menu in the toolbar.

* ``toggle_side_panel``

Toggle visibility of the side panel, i.e., where the scopes and variables are shown.

* ``toggle_fullscreen``

  Toggle fullscreen view.

* ``toggle_tick_lines``
* ``variable_set_name_type <Local | Unique | Global>``
* ``variable_force_name_type <Local | Unique | Global>``
* ``preference_set_clock_highlight <Line | Cycle | None>``


* ``preference_set_hierarchy_style <Separate | Tree>``

Set if the design hierarchy is shown with scopes and variables separated or as a tree.

* ``preference_set_arrow_key_bindings <Edge | Scroll>``

* ``config_reload``

## Cursor and markers

* ``goto_cursor``

  Go to the location of the main cursor. If off screen, scroll to it.

* ``goto_marker <MARKER_NAME> | #<MARKER_NUMBER>``

  Go to the location of the given marker. If off screen, scroll to it.

* ``cursor_set <TIME>``

  Move cursor to given time.

* ``marker_set  <MARKER_NAME> | #<MARKER_NUMBER>``

  Add/set marker to location of cursor.

* ``marker_remove <MARKER_NAME> | #<MARKER_NUMBER>``

  Remove marker.

* ``show_marker_window``

  Display window with markers and differences between markers

## Interactive simulation

* ``pause_simulation``
* ``unpause_simulation``

## Viewports

* ``viewport_add``
* ``viewport_remove``

## Waveform control protocol (WCP)

* ``wcp_server_start`` (not WASM)

Start the [WCP](https://gitlab.com/waveform-control-protocol/wcp/) server.
Typically, this is using port 54321 at address 127.0.0.1, but this can be changed
using the `address` setting in the `wcp` part of the config file.

* ``wcp_server_stop`` (not WASM)

Stop the WCP server.

## Other

* ``copy_value``

Copy the variable name and value at cursor to the clipboard.

* ``export_wave <PATH> <WIDTH> [HEIGHT]`` (not WASM)

  Export the waveform traces (cropped to the signal list area) as a PNG. ``WIDTH`` is in pixels. ``HEIGHT`` is optional — when omitted the canvas is auto-sized to fit all displayed rows without clipping.

  ```
  export_wave report/img/overview.png 1280
  export_wave report/img/detail.png 1280 400
  ```

* ``export_window <PATH> [WIDTH [HEIGHT]]`` (not WASM)

  Export the full Surfer window as a PNG.

* ``undo``
* ``redo``
* ``exit`` (not WASM)

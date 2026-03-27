//! Help texts and dialogs.
use egui::{Context, Grid, OpenUrl, RichText, ScrollArea, Ui, Window};
use egui_remixicon::icons;
use emath::{Align2, Pos2};

use crate::keyboard_shortcuts::{ShortcutAction, SurferShortcuts};
use crate::wave_source::LoadOptions;
use crate::{SystemState, message::Message};

impl SystemState {
    pub fn help_message(&self, ui: &mut Ui) {
        if self.user.waves.is_none() {
            let show_command_prompt = self
                .user
                .config
                .shortcuts
                .format_shortcut(ShortcutAction::ShowCommandPrompt);

            ui.label(RichText::new(
                "Drag and drop a VCD, FST, or GHW file here to open it",
            ));

            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new(format!(
                "Or press {show_command_prompt} and type load_url"
            )));
            #[cfg(not(target_arch = "wasm32"))]
            ui.label(RichText::new(format!(
                "Or press {show_command_prompt} and type load_file or load_url"
            )));
            #[cfg(target_arch = "wasm32")]
            ui.label(RichText::new(
                "Or use the file menu or toolbar to open a URL",
            ));
            #[cfg(not(target_arch = "wasm32"))]
            ui.label(RichText::new(
                "Or use the file menu or toolbar to open a file or a URL",
            ));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Or click"));
                if ui.link("here").clicked() {
                    self.channels
                        .msg_sender
                        .send(Message::LoadWaveformFileFromUrl(
                            "https://app.surfer-project.org/picorv32.vcd".to_string(),
                            LoadOptions::Clear,
                        ))
                        .ok();
                }
                ui.label("to open an example waveform");
            });

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(20.0);
        }

        controls_listing(ui, &self.user.config.shortcuts);

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(20.0);

        #[cfg(target_arch = "wasm32")]
        {
            ui.label(RichText::new(
            "Note that this web based version is a bit slower than a natively installed version. There may also be a long delay with unresponsiveness when loading large waveforms because the web assembly version does not currently support multi threading.",
        ));

            ui.hyperlink_to(
                "See https://gitlab.com/surfer-project/surfer for install instructions",
                "https://gitlab.com/surfer-project/surfer",
            );
        }
    }
}

pub fn draw_about_window(ctx: &Context, msgs: &mut Vec<Message>) {
    let mut open = true;
    Window::new("About Surfer")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                ui.add_space(20.);
                ui.label(format!(
                    "Cargo version: {ver}",
                    ver = env!("CARGO_PKG_VERSION")
                ));
                if ui
                    .small_button(format!(
                        "Git version: {ver}",
                        ver = env!("VERGEN_GIT_DESCRIBE")
                    ))
                    .on_hover_text("Click to copy git version")
                    .clicked()
                {
                    ctx.copy_text(env!("VERGEN_GIT_DESCRIBE").to_string());
                }
                ui.label(format!(
                    "Build date: {date}",
                    date = env!("VERGEN_BUILD_DATE")
                ));
                ui.hyperlink_to(
                    (icons::GITLAB_FILL).to_string() + " repository",
                    "https://gitlab.com/surfer-project/surfer",
                );
                ui.hyperlink_to("Homepage", "https://surfer-project.org/");
                ui.add_space(10.);
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetAboutVisible(false));
                }
            })
        });
    if !open {
        msgs.push(Message::SetAboutVisible(false));
    }
}

pub fn draw_quickstart_help_window(
    ctx: &Context,
    msgs: &mut Vec<Message>,
    shortcuts: &SurferShortcuts,
) {
    let mut open = true;
    let show_command_prompt = shortcuts.format_shortcut(ShortcutAction::ShowCommandPrompt);
    Window::new("🏄 Surfer quick start")
        .collapsible(true)
        .resizable(true)
        .pivot(Align2::CENTER_CENTER)
        .open(&mut open)
        .default_pos(Pos2::new(
            ctx.content_rect().size().x / 2.,
            ctx.content_rect().size().y / 2.,
        ))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(5.);

                ui.label(RichText::new("Controls").size(20.));
                ui.add_space(5.);
                ui.label("↔ Use scroll and ctrl+scroll to navigate the waveform");
                ui.label(format!(
                    "🚀 Press {show_command_prompt} to open the command palette"
                ));
                ui.label("✋ Click the middle mouse button for gestures");
                ui.label("❓ See the help menu for more controls");
                ui.add_space(10.);
                ui.label(RichText::new("Adding traces").size(20.));
                ui.add_space(5.);
                ui.label("Add more traces using the command palette or using the sidebar");
                ui.add_space(10.);
                ui.label(RichText::new("Opening files").size(20.));
                ui.add_space(5.);
                ui.label("Open a new file by");
                ui.label("- dragging a VCD, FST, or GHW file");
                #[cfg(target_arch = "wasm32")]
                ui.label("- typing load_url in the command palette");
                #[cfg(not(target_arch = "wasm32"))]
                ui.label("- typing load_url or load_file in the command palette");
                ui.label("- using the file menu");
                ui.label("- using the toolbar");
                ui.add_space(10.);
            });
            ui.vertical_centered(|ui| {
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetQuickStartVisible(false));
                }
            })
        });
    if !open {
        msgs.push(Message::SetQuickStartVisible(false));
    }
}

pub fn draw_control_help_window(
    ctx: &Context,
    msgs: &mut Vec<Message>,
    shortcuts: &SurferShortcuts,
) {
    let mut open = true;
    Window::new("🖮 Surfer controls")
        .collapsible(true)
        .resizable(true)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                key_listing(ui, shortcuts);
                ui.add_space(10.);
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetKeyHelpVisible(false));
                }
            });
        });
    if !open {
        msgs.push(Message::SetKeyHelpVisible(false));
    }
}

/// Long list of key binding for the dialog.
fn key_listing(ui: &mut Ui, shortcuts: &SurferShortcuts) {
    let save_state_file = shortcuts.format_shortcut(ShortcutAction::SaveStateFile);
    let toggle_hierarchy = shortcuts.format_shortcut(ShortcutAction::ToggleSidePanel);
    let toggle_toolbar = shortcuts.format_shortcut(ShortcutAction::ToggleToolbar);
    let reload_waveform = shortcuts.format_shortcut(ShortcutAction::ReloadWaveform);
    let focus_item = shortcuts.format_shortcut(ShortcutAction::ItemFocus);
    let goto_end = shortcuts.format_shortcut(ShortcutAction::GoToEnd);
    let goto_start = shortcuts.format_shortcut(ShortcutAction::GoToStart);
    let zoom_in = shortcuts.format_shortcut(ShortcutAction::ZoomIn);
    let zoom_out = shortcuts.format_shortcut(ShortcutAction::ZoomOut);
    let show_command_prompt = shortcuts.format_shortcut(ShortcutAction::ShowCommandPrompt);
    let selected_item_toggle = shortcuts.format_shortcut(ShortcutAction::SelectToggle);
    let undo = shortcuts.format_shortcut(ShortcutAction::Undo);
    let redo = shortcuts.format_shortcut(ShortcutAction::Redo);
    let add_marker = shortcuts.format_shortcut(ShortcutAction::MarkerAdd);
    let scroll_up = shortcuts.format_shortcut(ShortcutAction::ScrollUp);
    let scroll_down = shortcuts.format_shortcut(ShortcutAction::ScrollDown);
    let delete_selected = shortcuts.format_shortcut(ShortcutAction::DeleteSelected);
    let toggle_menu = shortcuts.format_shortcut(ShortcutAction::ToggleMenu);
    let divider_add = shortcuts.format_shortcut(ShortcutAction::DividerAdd);
    #[cfg(not(target_arch = "wasm32"))]
    let ui_zoom_in = shortcuts.format_shortcut(ShortcutAction::UiZoomIn);
    #[cfg(not(target_arch = "wasm32"))]
    let ui_zoom_out = shortcuts.format_shortcut(ShortcutAction::UiZoomOut);
    let keys = vec![
        ("🚀", show_command_prompt.as_str(), "Show command prompt"),
        ("↔", "Scroll", "Pan"),
        ("🔎", "Ctrl+Scroll", "Zoom"),
        (icons::SAVE_FILL, &save_state_file, "Save the state"),
        (
            icons::LAYOUT_LEFT_FILL,
            &toggle_hierarchy,
            "Show or hide the design hierarchy",
        ),
        (icons::MENU_FILL, &toggle_menu, "Show or hide menu"),
        (icons::TOOLS_FILL, &toggle_toolbar, "Show or hide toolbar"),
        (icons::ZOOM_IN_FILL, &zoom_in, "Zoom in"),
        (icons::ZOOM_OUT_FILL, &zoom_out, "Zoom out"),
        #[cfg(not(target_arch = "wasm32"))]
        ("", &ui_zoom_in, "UI Zoom in"),
        #[cfg(not(target_arch = "wasm32"))]
        ("", &ui_zoom_out, "UI Zoom out"),
        ("", "k/⬆", "Scroll up"),
        ("", "j/⬇", "Scroll down"),
        ("", "Ctrl+k/⬆", "Move focused item up"),
        ("", "Ctrl+j/⬇", "Move focused item down"),
        ("", "Alt+k/⬆", "Move focus up"),
        ("", "Alt+j/⬇", "Move focus down"),
        ("", &selected_item_toggle, "Add focused item to selection"),
        ("", "Ctrl+Alt+k/⬆", "Extend selection up"),
        ("", "Ctrl+Alt+j/⬇", "Extend selection down"),
        ("", &undo, "Undo last change"),
        ("", &redo, "Redo last change"),
        ("", &focus_item, "Fast focus a variable"),
        ("", &add_marker, "Add marker at current cursor"),
        ("", "Ctrl+0-9", "Add numbered marker"),
        ("", "0-9", "Center view at numbered marker"),
        ("", &divider_add, "Add divider"),
        (icons::REWIND_START_FILL, &goto_start, "Go to start"),
        (icons::FORWARD_END_FILL, &goto_end, "Go to end"),
        (icons::REFRESH_LINE, &reload_waveform, "Reload waveform"),
        (icons::SPEED_FILL, &scroll_up, "Go one page/screen right"),
        (icons::REWIND_FILL, &scroll_down, "Go one page/screen left"),
        (
            icons::PLAY_FILL,
            "➡/l",
            "Go to next transition of focused variable (changeable in config)",
        ),
        (
            icons::PLAY_REVERSE_FILL,
            "⬅/h",
            "Go to previous transition of focused variable (changeable in config)",
        ),
        (
            "",
            "Ctrl+➡/l",
            "Go to next non-zero transition of focused variable",
        ),
        (
            "",
            "Ctrl+⬅/h",
            "Go to previous non-zero transition of focused variable",
        ),
        (
            icons::DELETE_BIN_2_FILL,
            &delete_selected,
            "Delete focused item",
        ),
        #[cfg(not(target_arch = "wasm32"))]
        (icons::FULLSCREEN_LINE, "F11", "Toggle full screen"),
    ];

    Grid::new("keys")
        .num_columns(3)
        .spacing([5., 5.])
        .show(ui, |ui| {
            for (symbol, control, description) in keys {
                let control = ctrl_to_cmd(control);
                ui.label(symbol);
                ui.label(control);
                ui.label(description);
                ui.end_row();
            }
        });

    add_hint_text(ui);
}

/// Shorter list displayed at startup screen.
fn controls_listing(ui: &mut Ui, shortcuts: &SurferShortcuts) {
    let show_command_prompt = shortcuts.format_shortcut(ShortcutAction::ShowCommandPrompt);
    let toggle_hierarchy = shortcuts.format_shortcut(ShortcutAction::ToggleSidePanel);
    let toggle_toolbar = shortcuts.format_shortcut(ShortcutAction::ToggleToolbar);
    let toggle_menu = shortcuts.format_shortcut(ShortcutAction::ToggleMenu);

    let controls = vec![
        ("🚀", show_command_prompt.as_str(), "Show command prompt"),
        ("↔", "Horizontal Scroll", "Pan"),
        ("↕", "j, k, Up, Down", "Scroll down/up"),
        ("⌖", "Ctrl+j, k, Up, Down", "Move focus down/up"),
        ("🔃", "Alt+j, k, Up, Down", "Move focused item down/up"),
        ("🔎", "Ctrl+Scroll", "Zoom"),
        (
            icons::LAYOUT_LEFT_2_FILL,
            &toggle_hierarchy,
            "Show or hide the design hierarchy",
        ),
        (icons::MENU_FILL, &toggle_menu, "Show or hide menu"),
        (icons::TOOLS_FILL, &toggle_toolbar, "Show or hide toolbar"),
    ];

    Grid::new("controls")
        .num_columns(2)
        .spacing([20., 5.])
        .show(ui, |ui| {
            for (symbol, control, description) in controls {
                let control = ctrl_to_cmd(control);
                ui.label(format!("{symbol}  {control}"));
                ui.label(description);
                ui.end_row();
            }
        });
    add_hint_text(ui);
}

fn add_hint_text(ui: &mut Ui) {
    ui.add_space(20.);
    ui.label(RichText::new("Hint: You can repeat keybinds by typing Alt+0-9 before them. For example, Alt+1 Alt+0 k scrolls 10 steps up."));
}

// Display information about licenses for Surfer and used crates.
pub fn draw_license_window(ctx: &Context, msgs: &mut Vec<Message>) {
    let mut open = true;
    let text = include_str!("../../LICENSE-EUPL-1.2.txt");
    Window::new("Surfer License")
        .open(&mut open)
        .collapsible(false)
        .max_height(600.)
        .default_size((600., 600.))
        .show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.label(text);
            });
            ui.add_space(10.);
            ui.horizontal(|ui| {
                if ui.button("Dependency licenses").clicked() {
                    ctx.open_url(OpenUrl {
                        url: "https://docs.surfer-project.org/licenses.html".to_string(),
                        new_tab: true,
                    });
                }
                if ui.button("Close").clicked() {
                    msgs.push(Message::SetLicenseVisible(false));
                }
            });
        });
    if !open {
        msgs.push(Message::SetLicenseVisible(false));
    }
}

// Replace Ctrl with Cmd in case of macos, unless we are running tests
fn ctrl_to_cmd(instr: &str) -> String {
    #[cfg(all(target_os = "macos", not(test)))]
    let instring = instr.to_string().replace("Ctrl", "Cmd");
    #[cfg(any(not(target_os = "macos"), test))]
    let instring = instr.to_string();
    instring
}

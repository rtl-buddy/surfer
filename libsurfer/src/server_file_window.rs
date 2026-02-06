use egui::{Context, Key, ScrollArea, TextWrapMode, Window};

use crate::{SystemState, message::Message, wave_source::LoadOptions};

impl SystemState {
    pub fn draw_surver_file_window(&self, ctx: &Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        let mut selected_file_idx = *self.surver_selected_file.borrow();
        let mut load_options = *self.surver_load_options.borrow();
        let mut should_load = false;

        Window::new("Select wave file")
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ScrollArea::both().id_salt("file_list").show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        if let Some(file_infos) = self.user.surver_file_infos.as_ref() {
                            for (i, file_info) in file_infos.iter().enumerate() {
                                // Only make item selectable if last_load_ok is true
                                ui.add_enabled_ui(file_info.last_load_ok, |ui| {
                                    let response = ui
                                        .selectable_label(
                                            Some(i) == selected_file_idx && file_info.last_load_ok,
                                            &file_info.filename,
                                        )
                                        .on_hover_ui(|ui| {
                                            ui.set_max_width(ui.spacing().tooltip_width);
                                            if file_info.last_load_ok {
                                                ui.label(format!(
                                                    "Size: {} bytes",
                                                    file_info.bytes
                                                ));
                                            } else {
                                                ui.colored_label(
                                                    ecolor::Color32::RED,
                                                    "File cannot be loaded. See logs for details.",
                                                );
                                            }
                                        });

                                    // Handle single click to select
                                    if response.clicked() && file_info.last_load_ok {
                                        selected_file_idx = Some(i);
                                        *self.surver_selected_file.borrow_mut() = Some(i);
                                    }

                                    // Handle double-click to select and load
                                    if response.double_clicked() && file_info.last_load_ok {
                                        selected_file_idx = Some(i);
                                        *self.surver_selected_file.borrow_mut() = Some(i);
                                        should_load = true;
                                    }
                                });
                            }
                        }
                    });
                });

                // Handle keyboard navigation
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    msgs.push(Message::SetSurverFileWindowVisible(false));
                    return;
                }

                if ui.input(|i| i.key_pressed(Key::Enter)) && selected_file_idx.is_some() {
                    should_load = true;
                }

                if self.user.selected_server_file_index.is_some() {
                    ui.separator();
                    ui.radio_value(
                        &mut load_options,
                        LoadOptions::Clear,
                        "Clean load (do not keep any variables)",
                    );
                    ui.radio_value(
                        &mut load_options,
                        LoadOptions::KeepAvailable,
                        "Reload and keep available variables",
                    );
                    ui.radio_value(
                        &mut load_options,
                        LoadOptions::KeepAll,
                        "Reload and keep all variables",
                    );
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        msgs.push(Message::SetSurverFileWindowVisible(false));
                    }

                    // Disable Select button when nothing is selected
                    ui.add_enabled_ui(selected_file_idx.is_some(), |ui| {
                        if ui.button("Select").clicked() {
                            should_load = true;
                        }
                    });
                });
            });

        // Handle file loading
        if should_load && let Some(file_idx) = selected_file_idx {
            msgs.push(Message::SetSurverFileWindowVisible(false));
            msgs.push(Message::LoadSurverFileByIndex(
                Some(file_idx),
                if self.user.selected_server_file_index.is_some() {
                    load_options
                } else {
                    LoadOptions::Clear
                },
            ));
        }

        if !open {
            msgs.push(Message::SetSurverFileWindowVisible(false));
        }
        *self.surver_load_options.borrow_mut() = load_options;
    }
}

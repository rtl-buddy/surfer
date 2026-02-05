use crate::{
    config::{ThemeColorPair, TransitionValue},
    dialog::{draw_open_sibling_state_file_dialog, draw_reload_waveform_dialog},
    displayed_item::DisplayedVariable,
    fzcmd::expand_command,
    menus::generic_context_menu,
    tooltips::variable_tooltip_text,
    wave_container::{ScopeId, VarId, VariableMeta},
};
use ecolor::Color32;
#[cfg(not(target_arch = "wasm32"))]
use egui::ViewportCommand;
use egui::{
    CentralPanel, FontSelection, Frame, Layout, Painter, RichText, ScrollArea, Sense, SidePanel,
    TextStyle, Ui, UiBuilder, WidgetText,
};
use emath::{Align, GuiRounding, Pos2, Rect, RectTransform, Vec2};
use epaint::{
    CornerRadiusF32, Margin, Shape, Stroke,
    text::{FontId, LayoutJob, TextFormat, TextWrapMode},
};
use itertools::Itertools;
use num::{BigUint, One, Zero};
use tracing::info;

use surfer_translation_types::{
    TranslatedValue, Translator, VariableInfo, VariableValue,
    translator::{TrueName, VariableNameInfo},
};

use crate::OUTSTANDING_TRANSACTIONS;
#[cfg(feature = "performance_plot")]
use crate::benchmark::NUM_PERF_SAMPLES;
use crate::command_parser::get_parser;
use crate::config::SurferTheme;
use crate::displayed_item::{DisplayedFieldRef, DisplayedItem, DisplayedItemRef};
use crate::displayed_item_tree::{ItemIndex, VisibleItemIndex};
use crate::help::{
    draw_about_window, draw_control_help_window, draw_license_window, draw_quickstart_help_window,
};
use crate::time::time_string;
use crate::transaction_container::TransactionStreamRef;
use crate::translation::TranslationResultExt;
use crate::util::get_alpha_focus_id;
use crate::wave_container::{FieldRef, FieldRefExt, VariableRef};
use crate::{
    Message, MoveDir, SystemState, command_prompt::show_command_prompt, hierarchy::HierarchyStyle,
    wave_data::WaveData,
};

pub struct DrawingContext<'a> {
    pub painter: &'a mut Painter,
    pub cfg: &'a DrawConfig,
    pub to_screen: &'a dyn Fn(f32, f32) -> Pos2,
    pub theme: &'a SurferTheme,
}

#[derive(Debug)]
pub struct DrawConfig {
    pub canvas_height: f32,
    pub line_height: f32,
    pub text_size: f32,
    pub extra_draw_width: i32,
}

impl DrawConfig {
    #[must_use]
    pub fn new(canvas_height: f32, line_height: f32, text_size: f32) -> Self {
        Self {
            canvas_height,
            line_height,
            text_size,
            extra_draw_width: 6,
        }
    }
}

#[derive(Debug)]
pub struct VariableDrawingInfo {
    pub field_ref: FieldRef,
    pub displayed_field_ref: DisplayedFieldRef,
    pub vidx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct DividerDrawingInfo {
    pub vidx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct MarkerDrawingInfo {
    pub vidx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
    pub idx: u8,
}

#[derive(Debug)]
pub struct TimeLineDrawingInfo {
    pub vidx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct StreamDrawingInfo {
    pub transaction_stream_ref: TransactionStreamRef,
    pub vidx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct GroupDrawingInfo {
    pub vidx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug)]
pub struct PlaceholderDrawingInfo {
    pub vidx: VisibleItemIndex,
    pub top: f32,
    pub bottom: f32,
}

pub enum ItemDrawingInfo {
    Variable(VariableDrawingInfo),
    Divider(DividerDrawingInfo),
    Marker(MarkerDrawingInfo),
    TimeLine(TimeLineDrawingInfo),
    Stream(StreamDrawingInfo),
    Group(GroupDrawingInfo),
    Placeholder(PlaceholderDrawingInfo),
}

impl ItemDrawingInfo {
    #[must_use]
    pub fn top(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.top,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Group(drawing_info) => drawing_info.top,
            ItemDrawingInfo::Placeholder(drawing_info) => drawing_info.top,
        }
    }
    #[must_use]
    pub fn bottom(&self) -> f32 {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Group(drawing_info) => drawing_info.bottom,
            ItemDrawingInfo::Placeholder(drawing_info) => drawing_info.bottom,
        }
    }
    #[must_use]
    pub fn vidx(&self) -> VisibleItemIndex {
        match self {
            ItemDrawingInfo::Variable(drawing_info) => drawing_info.vidx,
            ItemDrawingInfo::Divider(drawing_info) => drawing_info.vidx,
            ItemDrawingInfo::Marker(drawing_info) => drawing_info.vidx,
            ItemDrawingInfo::TimeLine(drawing_info) => drawing_info.vidx,
            ItemDrawingInfo::Stream(drawing_info) => drawing_info.vidx,
            ItemDrawingInfo::Group(drawing_info) => drawing_info.vidx,
            ItemDrawingInfo::Placeholder(drawing_info) => drawing_info.vidx,
        }
    }
}

impl eframe::App for SystemState {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start_frame();

        if self.continuous_redraw {
            self.invalidate_draw_commands();
        }

        let (fullscreen, window_size) = ctx.input(|i| {
            (
                i.viewport().fullscreen.unwrap_or_default(),
                Some(i.viewport_rect().size()),
            )
        });
        #[cfg(target_arch = "wasm32")]
        let _ = fullscreen;

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start("draw");
        let mut msgs = self.draw(ctx, window_size);
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end("draw");

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start("push_async_messages");
        self.push_async_messages(&mut msgs);
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end("push_async_messages");

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start("update");
        let ui_zoom_factor = self.ui_zoom_factor();
        if ctx.zoom_factor() != ui_zoom_factor {
            ctx.set_zoom_factor(ui_zoom_factor);
        }

        self.items_to_expand.borrow_mut().clear();

        while let Some(msg) = msgs.pop() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::Exit = msg {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Message::ToggleFullscreen = msg {
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!fullscreen));
            }
            self.update(msg);
        }
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end("update");

        self.handle_batch_commands();
        #[cfg(target_arch = "wasm32")]
        self.handle_wasm_external_messages();

        let viewport_is_moving = if let Some(waves) = &mut self.user.waves {
            let mut is_moving = false;
            for vp in &mut waves.viewports {
                if vp.is_moving() {
                    vp.move_viewport(ctx.input(|i| i.stable_dt));
                    is_moving = true;
                }
            }
            is_moving
        } else {
            false
        };

        if let Some(waves) = self.user.waves.as_ref().and_then(|w| w.inner.as_waves()) {
            waves.tick();
        }

        if viewport_is_moving {
            self.invalidate_draw_commands();
            ctx.request_repaint();
        }

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().start("handle_wcp_commands");
        self.handle_wcp_commands();
        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end("handle_wcp_commands");

        // We can save some user battery life by not redrawing unless needed. At the moment,
        // we only need to continuously redraw to make surfer interactive during loading, otherwise
        // we'll let egui manage repainting. In practice
        if self.continuous_redraw
            || self.progress_tracker.is_some()
            || self.user.show_performance
            || OUTSTANDING_TRANSACTIONS.load(std::sync::atomic::Ordering::SeqCst) != 0
        {
            ctx.request_repaint();
        }

        #[cfg(feature = "performance_plot")]
        if let Some(prev_cpu) = frame.info().cpu_usage {
            self.rendering_cpu_times.push_back(prev_cpu);
            if self.rendering_cpu_times.len() > NUM_PERF_SAMPLES {
                self.rendering_cpu_times.pop_front();
            }
        }

        #[cfg(feature = "performance_plot")]
        self.timing.borrow_mut().end_frame();
    }
}

impl SystemState {
    pub(crate) fn draw(&mut self, ctx: &egui::Context, window_size: Option<Vec2>) -> Vec<Message> {
        let max_width = ctx.available_rect().width();
        let max_height = ctx.available_rect().height();

        let mut msgs = vec![];

        if self.user.show_about {
            draw_about_window(ctx, &mut msgs);
        }

        if self.user.show_license {
            draw_license_window(ctx, &mut msgs);
        }

        if self.user.show_keys {
            draw_control_help_window(ctx, &mut msgs, &self.user.config.shortcuts);
        }

        if self.user.show_quick_start {
            draw_quickstart_help_window(ctx, &mut msgs, &self.user.config.shortcuts);
        }

        if self.user.show_gestures {
            self.mouse_gesture_help(ctx, &mut msgs);
        }

        if self.user.show_logs {
            self.draw_log_window(ctx, &mut msgs);
        }

        if let Some(dialog) = self.user.show_reload_suggestion {
            draw_reload_waveform_dialog(ctx, dialog, &mut msgs);
        }

        if let Some(dialog) = self.user.show_open_sibling_state_file_suggestion {
            draw_open_sibling_state_file_dialog(ctx, dialog, &mut msgs);
        }

        if self.user.show_performance {
            #[cfg(feature = "performance_plot")]
            self.draw_performance_graph(ctx, &mut msgs);
        }

        if self.user.show_cursor_window
            && let Some(waves) = &self.user.waves
        {
            self.draw_marker_window(waves, ctx, &mut msgs);
        }

        if self
            .user
            .show_menu
            .unwrap_or_else(|| self.user.config.layout.show_menu())
        {
            self.add_menu_panel(ctx, &mut msgs);
        }

        if self.show_toolbar() {
            self.add_toolbar_panel(ctx, &mut msgs);
        }

        if self.user.show_url_entry {
            self.draw_load_url(ctx, &mut msgs);
        }

        if self.user.show_server_file_window {
            self.draw_surver_file_window(ctx, &mut msgs);
        }

        if self.show_statusbar() {
            self.add_statusbar_panel(ctx, self.user.waves.as_ref(), &mut msgs);
        }
        if let Some(waves) = &self.user.waves
            && self.show_overview()
            && !waves.items_tree.is_empty()
        {
            self.add_overview_panel(ctx, waves, &mut msgs);
        }

        if self.show_hierarchy() {
            SidePanel::left("variable select left panel")
                .default_width(300.)
                .width_range(100.0..=max_width)
                .frame(Frame {
                    fill: self.user.config.theme.primary_ui_color.background,
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    self.user.sidepanel_width = Some(ui.clip_rect().width());
                    match self.hierarchy_style() {
                        HierarchyStyle::Separate => self.separate(ui, &mut msgs),
                        HierarchyStyle::Tree => self.tree(ui, &mut msgs),
                        HierarchyStyle::Variables => self.variable_list(ui, &mut msgs),
                    }
                });
        }

        if self.command_prompt.visible {
            show_command_prompt(self, ctx, window_size, &mut msgs);
            if let Some(new_idx) = self.command_prompt.new_selection {
                self.command_prompt.selected = new_idx;
                self.command_prompt.new_selection = None;
            }
        }

        if self.user.waves.is_some() {
            let scroll_offset = self.user.waves.as_ref().unwrap().scroll_offset;
            if self.user.waves.as_ref().unwrap().any_displayed() {
                let draw_focus_ids = self.command_prompt.visible
                    && expand_command(&self.command_prompt_text.borrow(), get_parser(self))
                        .expanded
                        .starts_with("item_focus");
                if draw_focus_ids {
                    SidePanel::left("focus id list")
                        .default_width(40.)
                        .width_range(40.0..=max_width)
                        .show(ctx, |ui| {
                            self.handle_pointer_in_ui(ui, &mut msgs);
                            let response = ScrollArea::both()
                                .vertical_scroll_offset(scroll_offset)
                                .show(ui, |ui| {
                                    self.draw_item_focus_list(ui);
                                });
                            self.user.waves.as_mut().unwrap().top_item_draw_offset =
                                response.inner_rect.min.y;
                            self.user.waves.as_mut().unwrap().total_height =
                                response.inner_rect.height();
                            if (scroll_offset - response.state.offset.y).abs() > 5. {
                                msgs.push(Message::SetScrollOffset(response.state.offset.y));
                            }
                        });
                }

                SidePanel::left("variable list")
                    .default_width(100.)
                    .width_range(100.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        self.handle_pointer_in_ui(ui, &mut msgs);
                        if self.show_default_timeline() {
                            ui.label(RichText::new("Time").italics());
                        }

                        let response = ScrollArea::both()
                            .auto_shrink([false; 2])
                            .vertical_scroll_offset(scroll_offset)
                            .show(ui, |ui| {
                                self.draw_item_list(&mut msgs, ui, ctx);
                            });
                        self.user.waves.as_mut().unwrap().top_item_draw_offset =
                            response.inner_rect.min.y;
                        self.user.waves.as_mut().unwrap().total_height =
                            response.inner_rect.height();
                        if (scroll_offset - response.state.offset.y).abs() > 5. {
                            msgs.push(Message::SetScrollOffset(response.state.offset.y));
                        }
                    });

                // Will only draw if a transaction is focused
                self.draw_transaction_detail_panel(ctx, max_width, &mut msgs);

                SidePanel::left("variable values")
                    .frame(
                        egui::Frame::default()
                            .inner_margin(0)
                            .outer_margin(0)
                            .fill(self.user.config.theme.secondary_ui_color.background),
                    )
                    .default_width(100.)
                    .width_range(10.0..=max_width)
                    .show(ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        self.handle_pointer_in_ui(ui, &mut msgs);
                        let response = ScrollArea::both()
                            .auto_shrink([false; 2])
                            .vertical_scroll_offset(scroll_offset)
                            .show(ui, |ui| self.draw_var_values(ui, &mut msgs));
                        if (scroll_offset - response.state.offset.y).abs() > 5. {
                            msgs.push(Message::SetScrollOffset(response.state.offset.y));
                        }
                    });
                let std_stroke = ctx.style().visuals.widgets.noninteractive.bg_stroke;
                ctx.style_mut(|style| {
                    style.visuals.widgets.noninteractive.bg_stroke =
                        Stroke::from(&self.user.config.theme.viewport_separator);
                });
                let number_of_viewports = self.user.waves.as_ref().unwrap().viewports.len();
                if number_of_viewports > 1 {
                    // Draw additional viewports
                    let max_width = ctx.available_rect().width();
                    let default_width = max_width / (number_of_viewports as f32);
                    for viewport_idx in 1..number_of_viewports {
                        SidePanel::right(format! {"view port {viewport_idx}"})
                            .default_width(default_width)
                            .width_range(30.0..=max_width)
                            .frame(Frame {
                                inner_margin: Margin::ZERO,
                                outer_margin: Margin::ZERO,
                                ..Default::default()
                            })
                            .show(ctx, |ui| self.draw_items(ctx, &mut msgs, ui, viewport_idx));
                    }
                }

                CentralPanel::default()
                    .frame(Frame {
                        inner_margin: Margin::ZERO,
                        outer_margin: Margin::ZERO,
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        self.draw_items(ctx, &mut msgs, ui, 0);
                    });
                ctx.style_mut(|style| {
                    style.visuals.widgets.noninteractive.bg_stroke = std_stroke;
                });
            }
        }

        if self.user.waves.is_none()
            || self
                .user
                .waves
                .as_ref()
                .is_some_and(|waves| !waves.any_displayed())
        {
            CentralPanel::default()
                .frame(Frame::NONE.fill(self.user.config.theme.canvas_colors.background))
                .show(ctx, |ui| {
                    ui.add_space(max_height * 0.1);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🏄 Surfer").monospace().size(24.));
                        ui.add_space(20.);
                        let layout = Layout::top_down(Align::LEFT);
                        ui.allocate_ui_with_layout(
                            Vec2 {
                                x: max_width * 0.35,
                                y: max_height * 0.5,
                            },
                            layout,
                            |ui| self.help_message(ui),
                        );
                    });
                });
        }

        ctx.input(|i| {
            i.raw.dropped_files.iter().for_each(|file| {
                info!("Got dropped file");
                msgs.push(Message::FileDropped(file.clone()));
            });
        });

        // If some dialogs are open, skip decoding keypresses
        if !self.user.show_url_entry && self.user.show_reload_suggestion.is_none() {
            self.handle_pressed_keys(ctx, &mut msgs);
        }
        msgs
    }

    fn draw_load_url(&self, ctx: &egui::Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        egui::Window::new("Load URL")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let url = &mut *self.url.borrow_mut();
                    let response = ui.text_edit_singleline(url);
                    ui.horizontal(|ui| {
                        if ui.button("Load URL").clicked()
                            || (response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            if let Some(callback) = &self.url_callback {
                                msgs.push(callback(url.clone()));
                            }
                            msgs.push(Message::SetUrlEntryVisible(false, None));
                        }
                        if ui.button("Cancel").clicked() {
                            msgs.push(Message::SetUrlEntryVisible(false, None));
                        }
                    });
                });
            });
        if !open {
            msgs.push(Message::SetUrlEntryVisible(false, None));
        }
    }

    pub fn handle_pointer_in_ui(&self, ui: &mut Ui, msgs: &mut Vec<Message>) {
        if ui.ui_contains_pointer() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            if scroll_delta.y > 0.0 {
                msgs.push(Message::InvalidateCount);
                msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
            } else if scroll_delta.y < 0.0 {
                msgs.push(Message::InvalidateCount);
                msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
            }
        }
    }

    fn draw_item_focus_list(&self, ui: &mut Ui) {
        let Some(waves) = self.user.waves.as_ref() else {
            return;
        };
        let alignment = self.get_name_alignment();
        ui.with_layout(
            Layout::top_down(alignment).with_cross_justify(false),
            |ui| {
                if self.show_default_timeline() {
                    ui.add_space(ui.text_style_height(&TextStyle::Body) + 2.0);
                }
                for (vidx, _) in waves.items_tree.iter_visible().enumerate() {
                    let vidx = VisibleItemIndex(vidx);
                    ui.scope(|ui| {
                        ui.style_mut().visuals.selection.bg_fill =
                            self.user.config.theme.accent_warn.background;
                        ui.style_mut().visuals.override_text_color =
                            Some(self.user.config.theme.accent_warn.foreground);
                        let _ = ui.selectable_label(true, get_alpha_focus_id(vidx, waves));
                    });
                }
            },
        );
    }

    fn hierarchy_icon(
        &self,
        ui: &mut Ui,
        has_children: bool,
        unfolded: bool,
        alignment: Align,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::splat(self.user.config.layout.waveforms_text_size),
            Sense::click(),
        );
        if !has_children {
            return response;
        }

        // fixme: use the much nicer remixicon arrow? do a layout here and paint the galley into the rect?
        // or alternatively: change how the tree iterator works and use the egui facilities (cross widget?)
        let icon_rect = Rect::from_center_size(
            rect.center(),
            emath::vec2(rect.width(), rect.height()) * 0.75,
        );
        let mut points = vec![
            icon_rect.left_top(),
            icon_rect.right_top(),
            icon_rect.center_bottom(),
        ];
        let rotation = emath::Rot2::from_angle(if unfolded {
            0.0
        } else if alignment == Align::LEFT {
            -std::f32::consts::TAU / 4.0
        } else {
            std::f32::consts::TAU / 4.0
        });
        for p in &mut points {
            *p = icon_rect.center() + rotation * (*p - icon_rect.center());
        }

        let style = ui.style().interact(&response);
        ui.painter().add(Shape::convex_polygon(
            points,
            style.fg_stroke.color,
            Stroke::NONE,
        ));
        response
    }

    fn draw_item_list(&mut self, msgs: &mut Vec<Message>, ui: &mut Ui, ctx: &egui::Context) {
        let mut item_offsets = Vec::new();

        let any_groups = self
            .user
            .waves
            .as_ref()
            .unwrap()
            .items_tree
            .iter()
            .any(|node| node.level > 0);
        let alignment = self.get_name_alignment();
        ui.with_layout(Layout::top_down(alignment).with_cross_justify(true), |ui| {
            let available_rect = ui.available_rect_before_wrap();
            for crate::displayed_item_tree::Info {
                node:
                    crate::displayed_item_tree::Node {
                        item_ref,
                        level,
                        unfolded,
                        ..
                    },
                vidx,
                has_children,
                last,
                ..
            } in self
                .user
                .waves
                .as_ref()
                .unwrap()
                .items_tree
                .iter_visible_extra()
            {
                let Some(displayed_item) = self
                    .user
                    .waves
                    .as_ref()
                    .unwrap()
                    .displayed_items
                    .get(item_ref)
                else {
                    continue;
                };

                ui.with_layout(
                    if alignment == Align::LEFT {
                        Layout::left_to_right(Align::TOP)
                    } else {
                        Layout::right_to_left(Align::TOP)
                    },
                    |ui| {
                        ui.add_space(10.0 * f32::from(*level));
                        if any_groups {
                            let response =
                                self.hierarchy_icon(ui, has_children, *unfolded, alignment);
                            if response.clicked() {
                                if *unfolded {
                                    msgs.push(Message::GroupFold(Some(*item_ref)));
                                } else {
                                    msgs.push(Message::GroupUnfold(Some(*item_ref)));
                                }
                            }
                        }

                        let item_rect = match displayed_item {
                            DisplayedItem::Variable(displayed_variable) => {
                                let levels_to_force_expand =
                                    self.items_to_expand.borrow().iter().find_map(
                                        |(id, levels)| {
                                            if item_ref == id { Some(*levels) } else { None }
                                        },
                                    );

                                self.draw_variable(
                                    msgs,
                                    vidx,
                                    displayed_item,
                                    *item_ref,
                                    FieldRef::without_fields(
                                        displayed_variable.variable_ref.clone(),
                                    ),
                                    &mut item_offsets,
                                    &displayed_variable.info,
                                    ui,
                                    ctx,
                                    levels_to_force_expand,
                                    alignment,
                                )
                            }
                            DisplayedItem::Divider(_)
                            | DisplayedItem::Marker(_)
                            | DisplayedItem::Placeholder(_)
                            | DisplayedItem::TimeLine(_)
                            | DisplayedItem::Stream(_)
                            | DisplayedItem::Group(_) => {
                                ui.with_layout(
                                    ui.layout()
                                        .with_main_justify(true)
                                        .with_main_align(alignment),
                                    |ui| {
                                        self.draw_plain_item(
                                            msgs,
                                            vidx,
                                            *item_ref,
                                            displayed_item,
                                            &mut item_offsets,
                                            ui,
                                            ctx,
                                        )
                                    },
                                )
                                .inner
                            }
                        };
                        // expand to the left, but not over the icon size
                        let mut expanded_rect = item_rect;
                        expanded_rect.set_left(
                            available_rect.left()
                                + self.user.config.layout.waveforms_text_size
                                + ui.spacing().item_spacing.x,
                        );
                        expanded_rect.set_right(available_rect.right());
                        self.draw_drag_target(msgs, vidx, expanded_rect, available_rect, ui, last);
                    },
                );
            }
        });

        self.user.waves.as_mut().unwrap().drawing_infos = item_offsets;

        // Context menu for the unused part
        let response = ui.allocate_response(ui.available_size(), Sense::click());
        generic_context_menu(msgs, &response);
    }

    fn get_name_alignment(&self) -> Align {
        if self.align_names_right() {
            Align::RIGHT
        } else {
            Align::LEFT
        }
    }

    fn draw_drag_source(
        &self,
        msgs: &mut Vec<Message>,
        vidx: VisibleItemIndex,
        item_response: &egui::Response,
        modifiers: egui::Modifiers,
    ) {
        if item_response.dragged_by(egui::PointerButton::Primary)
            && item_response.drag_delta().length() > self.user.config.theme.drag_threshold
        {
            if !modifiers.ctrl
                && !(self.user.waves.as_ref())
                    .and_then(|w| w.items_tree.get_visible(vidx))
                    .is_some_and(|i| i.selected)
            {
                msgs.push(Message::FocusItem(vidx));
                msgs.push(Message::ItemSelectionClear);
            }
            msgs.push(Message::SetItemSelected(vidx, true));
            msgs.push(Message::VariableDragStarted(vidx));
        }

        if item_response.drag_stopped()
            && self
                .user
                .drag_source_idx
                .is_some_and(|source_idx| source_idx == vidx)
        {
            msgs.push(Message::VariableDragFinished);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_variable_label(
        &self,
        vidx: VisibleItemIndex,
        displayed_item: &DisplayedItem,
        displayed_id: DisplayedItemRef,
        field: FieldRef,
        msgs: &mut Vec<Message>,
        ui: &mut Ui,
        ctx: &egui::Context,
        meta: Option<&VariableMeta>,
    ) -> egui::Response {
        let mut variable_label = self.draw_item_label(
            vidx,
            displayed_id,
            displayed_item,
            Some(&field),
            msgs,
            ui,
            ctx,
            meta,
        );

        if self.show_tooltip() {
            variable_label = variable_label.on_hover_ui(|ui| {
                let tooltip = if self.user.waves.is_some() {
                    if field.field.is_empty() {
                        if let Some(meta) = meta {
                            variable_tooltip_text(Some(meta), &field.root)
                        } else {
                            let wave_container =
                                self.user.waves.as_ref().unwrap().inner.as_waves().unwrap();
                            let meta = wave_container.variable_meta(&field.root).ok();
                            variable_tooltip_text(meta.as_ref(), &field.root)
                        }
                    } else {
                        "From translator".to_string()
                    }
                } else {
                    "No waveform loaded".to_string()
                };
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.add(egui::Label::new(tooltip));
            });
        }

        variable_label
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_variable(
        &self,
        msgs: &mut Vec<Message>,
        vidx: VisibleItemIndex,
        displayed_item: &DisplayedItem,
        displayed_id: DisplayedItemRef,
        field: FieldRef,
        drawing_infos: &mut Vec<ItemDrawingInfo>,
        info: &VariableInfo,
        ui: &mut Ui,
        ctx: &egui::Context,
        levels_to_force_expand: Option<usize>,
        alignment: Align,
    ) -> Rect {
        let displayed_field_ref = DisplayedFieldRef {
            item: displayed_id,
            field: field.field.clone(),
        };
        match info {
            VariableInfo::Compound { subfields } => {
                let mut header = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    egui::Id::new(&field),
                    false,
                );

                if let Some(level) = levels_to_force_expand {
                    header.set_open(level > 0);
                }

                let response = ui
                    .with_layout(Layout::top_down(alignment).with_cross_justify(true), |ui| {
                        header
                            .show_header(ui, |ui| {
                                ui.with_layout(
                                    Layout::top_down(alignment).with_cross_justify(true),
                                    |ui| {
                                        self.draw_variable_label(
                                            vidx,
                                            displayed_item,
                                            displayed_id,
                                            field.clone(),
                                            msgs,
                                            ui,
                                            ctx,
                                            None,
                                        )
                                    },
                                );
                            })
                            .body(|ui| {
                                for (name, info) in subfields {
                                    let mut new_path = field.clone();
                                    new_path.field.push(name.clone());
                                    ui.with_layout(
                                        Layout::top_down(alignment).with_cross_justify(true),
                                        |ui| {
                                            self.draw_variable(
                                                msgs,
                                                vidx,
                                                displayed_item,
                                                displayed_id,
                                                new_path,
                                                drawing_infos,
                                                info,
                                                ui,
                                                ctx,
                                                levels_to_force_expand.map(|l| l.saturating_sub(1)),
                                                alignment,
                                            );
                                        },
                                    );
                                }
                            })
                    })
                    .inner;
                drawing_infos.push(ItemDrawingInfo::Variable(VariableDrawingInfo {
                    displayed_field_ref,
                    field_ref: field.clone(),
                    vidx,
                    top: response.0.rect.top(),
                    bottom: response.0.rect.bottom(),
                }));
                response.0.rect
            }
            VariableInfo::Bool
            | VariableInfo::Bits
            | VariableInfo::Clock
            | VariableInfo::String
            | VariableInfo::Event
            | VariableInfo::Real => {
                let label = ui
                    .with_layout(Layout::top_down(alignment).with_cross_justify(true), |ui| {
                        self.draw_variable_label(
                            vidx,
                            displayed_item,
                            displayed_id,
                            field.clone(),
                            msgs,
                            ui,
                            ctx,
                            None,
                        )
                    })
                    .inner;
                self.draw_drag_source(msgs, vidx, &label, ctx.input(|e| e.modifiers));
                drawing_infos.push(ItemDrawingInfo::Variable(VariableDrawingInfo {
                    displayed_field_ref,
                    field_ref: field.clone(),
                    vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
                label.rect
            }
        }
    }

    fn draw_drag_target(
        &self,
        msgs: &mut Vec<Message>,
        vidx: VisibleItemIndex,
        expanded_rect: Rect,
        available_rect: Rect,
        ui: &mut Ui,
        last: bool,
    ) {
        if !self.user.drag_started || self.user.drag_source_idx.is_none() {
            return;
        }

        let waves = self
            .user
            .waves
            .as_ref()
            .expect("waves not available, but expected");

        // expanded_rect is just for the label, leaving us with gaps between lines
        // expand to counter that
        let rect_with_margin = expanded_rect.expand2(ui.spacing().item_spacing / 2f32);

        // collision check rect need to be
        // - limited to half the height of the item text
        // - extended to cover the empty space to the left
        // - for the last element, expanded till the bottom
        let before_rect = rect_with_margin
            .with_max_y(rect_with_margin.left_center().y)
            .with_min_x(available_rect.left())
            .round_to_pixels(ui.painter().pixels_per_point());
        let after_rect = if last {
            rect_with_margin.with_max_y(ui.max_rect().max.y)
        } else {
            rect_with_margin
        }
        .with_min_y(rect_with_margin.left_center().y)
        .with_min_x(available_rect.left())
        .round_to_pixels(ui.painter().pixels_per_point());

        let (insert_vidx, line_y) = if ui.rect_contains_pointer(before_rect) {
            (vidx, rect_with_margin.top())
        } else if ui.rect_contains_pointer(after_rect) {
            (VisibleItemIndex(vidx.0 + 1), rect_with_margin.bottom())
        } else {
            return;
        };

        let level_range = waves.items_tree.valid_levels_visible(insert_vidx, |node| {
            matches!(
                waves.displayed_items.get(&node.item_ref),
                Some(DisplayedItem::Group(..))
            )
        });

        let left_x = |level: u8| -> f32 { rect_with_margin.left() + f32::from(level) * 10.0 };
        let Some(insert_level) = level_range.find_or_last(|&level| {
            let mut rect = expanded_rect.with_min_x(left_x(level));
            rect.set_width(10.0);
            if level == 0 {
                rect.set_left(available_rect.left());
            }
            ui.rect_contains_pointer(rect)
        }) else {
            return;
        };

        ui.painter().line_segment(
            [
                Pos2::new(left_x(insert_level), line_y),
                Pos2::new(rect_with_margin.right(), line_y),
            ],
            Stroke::new(
                self.user.config.theme.linewidth,
                self.user.config.theme.drag_hint_color,
            ),
        );
        msgs.push(Message::VariableDragTargetChanged(
            crate::displayed_item_tree::TargetPosition {
                before: ItemIndex(
                    waves
                        .items_tree
                        .to_displayed(insert_vidx)
                        .map_or_else(|| waves.items_tree.len(), |index| index.0),
                ),
                level: insert_level,
            },
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_item_label(
        &self,
        vidx: VisibleItemIndex,
        displayed_id: DisplayedItemRef,
        displayed_item: &DisplayedItem,
        field: Option<&FieldRef>,
        msgs: &mut Vec<Message>,
        ui: &mut Ui,
        ctx: &egui::Context,
        meta: Option<&VariableMeta>,
    ) -> egui::Response {
        let color_pair = {
            if self.item_is_focused(vidx) {
                &self.user.config.theme.accent_info
            } else if self.item_is_selected(displayed_id) {
                &self.user.config.theme.selected_elements_colors
            } else if matches!(
                displayed_item,
                DisplayedItem::Variable(_) | DisplayedItem::Placeholder(_)
            ) {
                &self.user.config.theme.primary_ui_color
            } else {
                &ThemeColorPair {
                    background: self.user.config.theme.primary_ui_color.background,
                    foreground: self.get_item_text_color(displayed_item),
                }
            }
        };
        {
            let style = ui.style_mut();
            style.visuals.selection.bg_fill = color_pair.background;
        }

        let mut layout_job = LayoutJob::default();
        match displayed_item {
            DisplayedItem::Variable(var) if field.is_some() => {
                let field = field.unwrap();
                if field.field.is_empty() {
                    let name_info = self.get_variable_name_info(&var.variable_ref, meta);

                    if let Some(true_name) = name_info.and_then(|info| info.true_name) {
                        let monospace_font =
                            ui.style().text_styles.get(&TextStyle::Monospace).unwrap();
                        let monospace_width = {
                            ui.fonts_mut(|fonts| {
                                fonts
                                    .layout_no_wrap(
                                        " ".to_string(),
                                        monospace_font.clone(),
                                        Color32::BLACK,
                                    )
                                    .size()
                                    .x
                            })
                        };
                        let available_width = ui.available_width();

                        draw_true_name(
                            &true_name,
                            &mut layout_job,
                            monospace_font.clone(),
                            color_pair.foreground,
                            monospace_width,
                            available_width,
                        );
                    } else {
                        displayed_item.add_to_layout_job(
                            color_pair.foreground,
                            ui.style(),
                            &mut layout_job,
                            Some(field),
                            &self.user.config,
                        );
                    }
                } else {
                    RichText::new(field.field.last().unwrap().clone())
                        .color(color_pair.foreground)
                        .line_height(Some(self.user.config.layout.waveforms_line_height))
                        .append_to(
                            &mut layout_job,
                            ui.style(),
                            FontSelection::Default,
                            Align::Center,
                        );
                }
            }
            _ => displayed_item.add_to_layout_job(
                color_pair.foreground,
                ui.style(),
                &mut layout_job,
                field,
                &self.user.config,
            ),
        }

        let item_label = ui
            .selectable_label(
                self.item_is_selected(displayed_id) || self.item_is_focused(vidx),
                WidgetText::LayoutJob(layout_job.into()),
            )
            .interact(Sense::drag());

        // click can select and deselect, depending on previous selection state & modifiers
        // with the rules:
        // - a primary click on the single selected item will deselect it (so that there is a
        //   way to deselect and get rid of the selection highlight)
        // - a primary/secondary click otherwise will select just the clicked item
        // - a secondary click on the selection will not change the selection
        // - a click with shift added will select all items between focused and clicked
        // - a click with control added will toggle the selection of the item
        // - shift + control does not have special meaning
        //
        // We do not implement more complex behavior like the selection toggling
        // that the windows explorer had in the past (with combined ctrl+shift)
        if item_label.clicked() || item_label.secondary_clicked() {
            let focused_item = self.user.waves.as_ref().and_then(|w| w.focused_item);
            let is_focused = focused_item == Some(vidx);
            let is_selected = self.item_is_selected(displayed_id);
            let single_selected = self
                .user
                .waves
                .as_ref()
                .map(|w| {
                    // FIXME check if this is fast
                    let it = w.items_tree.iter_visible_selected();
                    it.count() == 1
                })
                .unwrap();

            let modifiers = ctx.input(|i| i.modifiers);
            tracing::trace!(focused_item=?focused_item, is_focused=?is_focused, is_selected=?is_selected, single_selected=?single_selected, modifiers=?modifiers);

            // allow us to deselect, but only do so if this is the only selected item
            if item_label.clicked() && is_selected && single_selected {
                msgs.push(Message::Batch(vec![
                    Message::ItemSelectionClear,
                    Message::UnfocusItem,
                ]));
                return item_label;
            }

            match (item_label.clicked(), modifiers.command, modifiers.shift) {
                (false, false, false) if is_selected => {}
                (_, false, false) => {
                    msgs.push(Message::Batch(vec![
                        Message::ItemSelectionClear,
                        Message::SetItemSelected(vidx, true),
                        Message::FocusItem(vidx),
                    ]));
                }
                (_, _, true) => msgs.push(Message::Batch(vec![
                    Message::ItemSelectRange(vidx),
                    Message::FocusItem(vidx),
                ])),
                (_, true, false) => {
                    if !is_selected {
                        msgs.push(Message::Batch(vec![
                            Message::SetItemSelected(vidx, true),
                            Message::FocusItem(vidx),
                        ]));
                    } else if item_label.clicked() {
                        msgs.push(Message::Batch(vec![
                            Message::SetItemSelected(vidx, false),
                            Message::UnfocusItem,
                        ]))
                    }
                }
            }
        }

        item_label.context_menu(|ui| {
            self.item_context_menu(
                field,
                msgs,
                ui,
                vidx,
                true,
                crate::message::MessageTarget::CurrentSelection,
            );
        });

        item_label
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_plain_item(
        &self,
        msgs: &mut Vec<Message>,
        vidx: VisibleItemIndex,
        displayed_id: DisplayedItemRef,
        displayed_item: &DisplayedItem,
        drawing_infos: &mut Vec<ItemDrawingInfo>,
        ui: &mut Ui,
        ctx: &egui::Context,
    ) -> Rect {
        let label = self.draw_item_label(
            vidx,
            displayed_id,
            displayed_item,
            None,
            msgs,
            ui,
            ctx,
            None,
        );

        self.draw_drag_source(msgs, vidx, &label, ui.ctx().input(|e| e.modifiers));
        match displayed_item {
            DisplayedItem::Divider(_) => {
                drawing_infos.push(ItemDrawingInfo::Divider(DividerDrawingInfo {
                    vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            DisplayedItem::Marker(cursor) => {
                drawing_infos.push(ItemDrawingInfo::Marker(MarkerDrawingInfo {
                    vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                    idx: cursor.idx,
                }));
            }
            DisplayedItem::TimeLine(_) => {
                drawing_infos.push(ItemDrawingInfo::TimeLine(TimeLineDrawingInfo {
                    vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            DisplayedItem::Stream(stream) => {
                drawing_infos.push(ItemDrawingInfo::Stream(StreamDrawingInfo {
                    transaction_stream_ref: stream.transaction_stream_ref.clone(),
                    vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            DisplayedItem::Group(_) => {
                drawing_infos.push(ItemDrawingInfo::Group(GroupDrawingInfo {
                    vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            &DisplayedItem::Placeholder(_) => {
                drawing_infos.push(ItemDrawingInfo::Placeholder(PlaceholderDrawingInfo {
                    vidx,
                    top: label.rect.top(),
                    bottom: label.rect.bottom(),
                }));
            }
            &DisplayedItem::Variable(_) => {
                panic!(
                    "draw_plain_item must not be called with a Variable - use draw_variable instead"
                )
            }
        }
        label.rect
    }

    fn item_is_focused(&self, vidx: VisibleItemIndex) -> bool {
        if let Some(waves) = &self.user.waves {
            waves.focused_item == Some(vidx)
        } else {
            false
        }
    }

    fn item_is_selected(&self, id: DisplayedItemRef) -> bool {
        if let Some(waves) = &self.user.waves {
            waves
                .items_tree
                .iter_visible_selected()
                .any(|node| node.item_ref == id)
        } else {
            false
        }
    }

    fn draw_var_values(&self, ui: &mut Ui, msgs: &mut Vec<Message>) {
        let Some(waves) = &self.user.waves else {
            return;
        };
        let response = ui.allocate_response(ui.available_size(), Sense::click());
        generic_context_menu(msgs, &response);

        let mut painter = ui.painter().clone();
        let rect = response.rect;
        let container_rect = Rect::from_min_size(Pos2::ZERO, rect.size());
        let to_screen = RectTransform::from_to(container_rect, rect);
        let cfg = DrawConfig::new(
            rect.height(),
            self.user.config.layout.waveforms_line_height,
            self.user.config.layout.waveforms_text_size,
        );
        let frame_width = rect.width();

        let ctx = DrawingContext {
            painter: &mut painter,
            cfg: &cfg,
            to_screen: &|x, y| to_screen.transform_pos(Pos2::new(x, y)),
            theme: &self.user.config.theme,
        };

        let gap = ui.spacing().item_spacing.y * 0.5;
        let y_zero = to_screen.transform_pos(Pos2::ZERO).y;
        let ucursor = waves.cursor.as_ref().and_then(num::BigInt::to_biguint);

        // Add default margin as it was removed when creating the frame
        let rect_with_margin = Rect {
            min: rect.min + ui.spacing().item_spacing,
            max: rect.max + Vec2::new(0.0, 40.0),
        };

        let builder = UiBuilder::new().max_rect(rect_with_margin);
        ui.scope_builder(builder, |ui| {
            let text_style = TextStyle::Monospace;
            ui.style_mut().override_text_style = Some(text_style);
            for (item_count, drawing_info) in waves
                .drawing_infos
                .iter()
                .sorted_by_key(|o| o.top() as i32)
                .enumerate()
            {
                let next_y = ui.cursor().top();
                // In order to align the text in this view with the variable tree,
                // we need to keep track of how far away from the expected offset we are,
                // and compensate for it
                if next_y < drawing_info.top() {
                    ui.add_space(drawing_info.top() - next_y);
                }

                let backgroundcolor =
                    self.get_background_color(waves, drawing_info.vidx(), item_count);
                self.draw_background(
                    drawing_info,
                    y_zero,
                    &ctx,
                    gap,
                    frame_width,
                    backgroundcolor,
                );
                match drawing_info {
                    ItemDrawingInfo::Variable(drawing_info) => {
                        if ucursor.as_ref().is_none() {
                            ui.label("");
                            continue;
                        }

                        let v = self.get_variable_value(
                            waves,
                            &drawing_info.displayed_field_ref,
                            ucursor.as_ref(),
                        );
                        if let Some(v) = v {
                            ui.label(
                                RichText::new(v)
                                    .color(
                                        self.user.config.theme.get_best_text_color(backgroundcolor),
                                    )
                                    .line_height(Some(
                                        self.user.config.layout.waveforms_line_height,
                                    )),
                            )
                            .context_menu(|ui| {
                                self.item_context_menu(
                                    Some(&FieldRef::without_fields(
                                        drawing_info.field_ref.root.clone(),
                                    )),
                                    msgs,
                                    ui,
                                    drawing_info.vidx,
                                    true,
                                    crate::message::MessageTarget::CurrentSelection,
                                );
                            });
                        }
                    }

                    ItemDrawingInfo::Marker(numbered_cursor) => {
                        if let Some(cursor) = &waves.cursor {
                            let delta = time_string(
                                &(waves.numbered_marker_time(numbered_cursor.idx) - cursor),
                                &waves.inner.metadata().timescale,
                                &self.user.wanted_timeunit,
                                &self.get_time_format(),
                            );

                            ui.label(RichText::new(format!("Δ: {delta}",)).color(
                                self.user.config.theme.get_best_text_color(backgroundcolor),
                            ))
                            .context_menu(|ui| {
                                self.item_context_menu(
                                    None,
                                    msgs,
                                    ui,
                                    drawing_info.vidx(),
                                    true,
                                    crate::message::MessageTarget::CurrentSelection,
                                );
                            });
                        } else {
                            ui.label("");
                        }
                    }
                    ItemDrawingInfo::Divider(_)
                    | ItemDrawingInfo::TimeLine(_)
                    | ItemDrawingInfo::Stream(_)
                    | ItemDrawingInfo::Group(_)
                    | ItemDrawingInfo::Placeholder(_) => {
                        ui.label("");
                    }
                }
            }
        });
    }

    pub fn get_variable_value(
        &self,
        waves: &WaveData,
        displayed_field_ref: &DisplayedFieldRef,
        ucursor: Option<&num::BigUint>,
    ) -> Option<String> {
        let ucursor = ucursor?;

        let DisplayedItem::Variable(displayed_variable) =
            waves.displayed_items.get(&displayed_field_ref.item)?
        else {
            return None;
        };

        let variable = &displayed_variable.variable_ref;
        let meta = waves
            .inner
            .as_waves()
            .unwrap()
            .variable_meta(variable)
            .ok()?;
        let translator = waves.variable_translator_with_meta(
            &displayed_field_ref.without_field(),
            &self.translators,
            &meta,
        );

        let wave_container = waves.inner.as_waves().unwrap();
        let query_result = wave_container
            .query_variable(variable, ucursor)
            .ok()
            .flatten()?;

        let (time, val) = query_result.current?;
        let curr = self.translate_query_result(
            displayed_field_ref,
            displayed_variable,
            translator,
            meta.clone(),
            val,
        );

        // If time doesn't match cursor, i.e., we are not at a transition or the cursor is at zero
        // or we want the next value after the transition, return current
        if time != *ucursor
            || (*ucursor).is_zero()
            || self.transition_value() == TransitionValue::Next
        {
            return curr;
        }

        // Otherwise, we need to check the previous value for transition display
        let prev_query_result = wave_container
            .query_variable(variable, &(ucursor - BigUint::one()))
            .ok()
            .flatten()?;

        let (_, prev_val) = prev_query_result.current?;
        let prev = self.translate_query_result(
            displayed_field_ref,
            displayed_variable,
            translator,
            meta,
            prev_val,
        );

        match self.transition_value() {
            TransitionValue::Previous => Some(format!("←{}", prev.unwrap_or_default())),
            TransitionValue::Both => match (curr, prev) {
                (Some(curr_val), Some(prev_val)) => Some(format!("{prev_val} → {curr_val}")),
                (None, Some(prev_val)) => Some(format!("{prev_val} →")),
                (Some(curr_val), None) => Some(format!("→ {curr_val}")),
                _ => None,
            },
            TransitionValue::Next => curr, // This will never happen due to the earlier check
        }
    }

    fn translate_query_result(
        &self,
        displayed_field_ref: &DisplayedFieldRef,
        displayed_variable: &DisplayedVariable,
        translator: &dyn Translator<VarId, ScopeId, Message>,
        meta: VariableMeta,
        val: VariableValue,
    ) -> Option<String> {
        let translated = translator.translate(&meta, &val).ok()?;
        let fields = translated.format_flat(
            &displayed_variable.format,
            &displayed_variable.field_formats,
            &self.translators,
        );

        let subfield = fields
            .iter()
            .find(|res| res.names == displayed_field_ref.field)?;

        match &subfield.value {
            Some(TranslatedValue { value, .. }) => Some(value.clone()),
            None => Some("-".to_string()),
        }
    }

    pub fn get_variable_name_info(
        &self,
        var: &VariableRef,
        meta: Option<&VariableMeta>,
    ) -> Option<VariableNameInfo> {
        self.variable_name_info_cache
            .borrow_mut()
            .entry(var.clone())
            .or_insert_with(|| {
                meta.as_ref().and_then(|meta| {
                    self.translators
                        .all_translators()
                        .iter()
                        .find_map(|t| t.variable_name_info(meta))
                })
            })
            .clone()
    }

    pub fn draw_background(
        &self,
        drawing_info: &ItemDrawingInfo,
        y_zero: f32,
        ctx: &DrawingContext<'_>,
        gap: f32,
        frame_width: f32,
        background_color: Color32,
    ) {
        // Draw background
        let min = (ctx.to_screen)(0.0, drawing_info.top() - y_zero - gap);
        let max = (ctx.to_screen)(frame_width, drawing_info.bottom() - y_zero + gap);
        ctx.painter
            .rect_filled(Rect { min, max }, CornerRadiusF32::ZERO, background_color);
    }

    pub fn get_background_color(
        &self,
        waves: &WaveData,
        vidx: VisibleItemIndex,
        item_count: usize,
    ) -> Color32 {
        if let Some(focused) = waves.focused_item
            && self.highlight_focused()
            && focused == vidx
        {
            return self.user.config.theme.highlight_background;
        }
        waves
            .displayed_items
            .get(&waves.items_tree.get_visible(vidx).unwrap().item_ref)
            .and_then(super::displayed_item::DisplayedItem::background_color)
            .and_then(|color| self.user.config.theme.get_color(color))
            .unwrap_or_else(|| self.get_default_alternating_background_color(item_count))
    }

    fn get_default_alternating_background_color(&self, item_count: usize) -> Color32 {
        // Set background color
        if self.user.config.theme.alt_frequency != 0
            && (item_count / self.user.config.theme.alt_frequency) % 2 == 1
        {
            self.user.config.theme.canvas_colors.alt_background
        } else {
            Color32::TRANSPARENT
        }
    }

    /// Draw the default timeline at the top of the canvas
    pub fn draw_default_timeline(
        &self,
        waves: &WaveData,
        ctx: &DrawingContext,
        viewport_idx: usize,
        frame_width: f32,
    ) {
        let ticks =
            self.get_ticks_for_viewport_idx(waves, viewport_idx, frame_width, ctx.cfg.text_size);

        waves.draw_ticks(
            self.user.config.theme.foreground,
            &ticks,
            ctx,
            0.0,
            emath::Align2::CENTER_TOP,
        );
    }
}

pub fn draw_true_name(
    true_name: &TrueName,
    layout_job: &mut LayoutJob,
    font: FontId,
    foreground: Color32,
    char_width: f32,
    allowed_space: f32,
) {
    let char_budget = (allowed_space / char_width) as usize;

    match true_name {
        TrueName::SourceCode {
            line_number,
            before,
            this,
            after,
        } => {
            let before_chars = before.chars().collect::<Vec<_>>();
            let this_chars = this.chars().collect::<Vec<_>>();
            let after_chars = after.chars().collect::<Vec<_>>();
            let line_num = format!("{line_number} ");
            let important_chars = line_num.len() + this_chars.len();
            let required_extra_chars = before_chars.len() + after_chars.len();

            // If everything fits, things are very easy
            let (line_num, before, this, after) =
                if char_budget >= important_chars + required_extra_chars {
                    (line_num, before.clone(), this.clone(), after.clone())
                } else if char_budget > important_chars {
                    // How many extra chars we have available
                    let extra_chars = char_budget - important_chars;

                    let max_from_before = (extra_chars as f32 / 2.).ceil() as usize;
                    let max_from_after = (extra_chars as f32 / 2.).floor() as usize;

                    let (chars_from_before, chars_from_after) =
                        if max_from_before > before_chars.len() {
                            (before_chars.len(), extra_chars - before_chars.len())
                        } else if max_from_after > after_chars.len() {
                            (extra_chars - after_chars.len(), before_chars.len())
                        } else {
                            (max_from_before, max_from_after)
                        };

                    let mut before = before_chars
                        .into_iter()
                        .rev()
                        .take(chars_from_before)
                        .rev()
                        .collect::<Vec<_>>();
                    if !before.is_empty() {
                        before[0] = '…';
                    }
                    let mut after = after_chars
                        .into_iter()
                        .take(chars_from_after)
                        .collect::<Vec<_>>();
                    if !after.is_empty() {
                        let last_elem = after.len() - 1;
                        after[last_elem] = '…';
                    }

                    (
                        line_num,
                        before.into_iter().collect(),
                        this.clone(),
                        after.into_iter().collect(),
                    )
                } else {
                    // If we can't even fit the whole important part,
                    // we'll prefer the line number
                    let from_line_num = line_num.len();
                    let from_this = char_budget.saturating_sub(from_line_num);
                    let this = this
                        .chars()
                        .take(from_this)
                        .enumerate()
                        .map(|(i, c)| if i == from_this - 1 { '…' } else { c })
                        .collect();
                    (line_num, String::new(), this, String::new())
                };

            layout_job.append(
                &line_num,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: foreground.gamma_multiply(0.75),
                    ..Default::default()
                },
            );
            layout_job.append(
                &before,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: foreground.gamma_multiply(0.5),
                    ..Default::default()
                },
            );
            layout_job.append(
                &this,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: foreground,
                    ..Default::default()
                },
            );
            layout_job.append(
                after.trim_end(),
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: foreground.gamma_multiply(0.5),
                    ..Default::default()
                },
            );
        }
    }
}

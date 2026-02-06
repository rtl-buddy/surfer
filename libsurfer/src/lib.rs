#![deny(unused_crate_dependencies)]

pub mod analog_renderer;
pub mod analog_signal_cache;
pub mod async_util;
pub mod batch_commands;
#[cfg(feature = "performance_plot")]
pub mod benchmark;
mod channels;
pub mod clock_highlighting;
pub mod command_parser;
pub mod command_prompt;
pub mod config;
pub mod cxxrtl;
pub mod cxxrtl_container;
pub mod data_container;
pub mod dialog;
pub mod displayed_item;
pub mod displayed_item_tree;
pub mod drawing_canvas;
pub mod file_dialog;
pub mod file_watcher;
pub mod fzcmd;
pub mod graphics;
pub mod help;
pub mod hierarchy;
pub mod keyboard_shortcuts;
pub mod keys;
pub mod logs;
pub mod marker;
pub mod menus;
pub mod message;
pub mod mousegestures;
pub mod overview;
pub mod remote;
pub mod server_file_window;
pub mod state;
pub mod state_file_io;
pub mod state_util;
pub mod statusbar;
pub mod system_state;
#[cfg(test)]
pub mod tests;
pub mod tiles;
pub mod time;
pub mod toolbar;
pub mod tooltips;
pub mod transaction_container;
pub mod transactions;
pub mod translation;
pub mod util;
pub mod variable_direction;
pub mod variable_filter;
mod variable_index;
pub mod variable_meta;
pub mod variable_name_type;
pub mod view;
pub mod viewport;
#[cfg(target_arch = "wasm32")]
pub mod wasm_api;
#[cfg(target_arch = "wasm32")]
pub mod wasm_panic;
pub mod wave_container;
pub mod wave_data;
pub mod wave_source;
mod waveform_tile;
pub mod wcp;
pub mod wellen;

use crate::config::AutoLoad;
use crate::displayed_item_tree::ItemIndex;
use crate::displayed_item_tree::TargetPosition;
use crate::remote::get_time_table_from_server;
use crate::variable_name_type::VariableNameType;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, LazyLock, RwLock};

use batch_commands::read_command_bytes;
use batch_commands::read_command_file;
#[cfg(target_arch = "wasm32")]
use channels::{GlobalChannelTx, IngressHandler, IngressReceiver};
use derive_more::Display;
use displayed_item::DisplayedVariable;
use displayed_item_tree::DisplayedItemTree;
use eframe::{App, CreationContext};
use egui::{FontData, FontDefinitions, FontFamily};
use eyre::Context;
use eyre::Result;
use ftr_parser::types::Transaction;
use futures::executor::block_on;
use itertools::Itertools;
use message::MessageTarget;
use num::BigInt;
use serde::Deserialize;
use surfer_translation_types::Translator;
use surfer_wcp::{WcpCSMessage, WcpEvent, WcpSCMessage};
pub use system_state::SystemState;
#[cfg(target_arch = "wasm32")]
use tokio_stream as _;
use tracing::{error, info, warn};
#[cfg(all(not(target_arch = "wasm32"), feature = "wasm_plugins"))]
use translation::wasm_translator::PluginTranslator;
use wave_container::ScopeRef;

#[cfg(all(not(target_arch = "wasm32"), feature = "wasm_plugins"))]
use crate::async_util::perform_work;
use crate::config::{SurferConfig, SurferTheme};
use crate::dialog::{OpenSiblingStateFileDialog, ReloadWaveformDialog};
use crate::displayed_item::{
    AnalogVarState, DisplayedFieldRef, DisplayedItem, DisplayedItemRef, FieldFormat,
};
use crate::displayed_item_tree::VisibleItemIndex;
use crate::drawing_canvas::TxDrawingCommands;
use crate::message::Message;
use crate::transaction_container::{StreamScopeRef, TransactionRef, TransactionStreamRef};
use crate::translation::{AnyTranslator, all_translators};
use crate::variable_filter::{VariableIOFilterType, VariableNameFilterType};
use crate::viewport::Viewport;
use crate::wave_container::VariableRefExt;
use crate::wave_container::{ScopeRefExt, WaveContainer};
use crate::wave_data::{ScopeType, WaveData};
use crate::wave_source::{LoadOptions, WaveFormat, WaveSource};
use crate::wellen::{HeaderResult, convert_format};

/// A number that is non-zero if there are asynchronously triggered operations that
/// have been triggered but not successfully completed yet. In practice, if this is
/// non-zero, we will re-run the egui update function in order to ensure that we deal
/// with the outstanding transactions eventually.
/// When incrementing this, it is important to make sure that it gets decremented
/// whenever the asynchronous transaction is completed, otherwise we will re-render
/// things until program exit
pub(crate) static OUTSTANDING_TRANSACTIONS: AtomicUsize = AtomicUsize::new(0);

pub static EGUI_CONTEXT: LazyLock<RwLock<Option<Arc<egui::Context>>>> =
    LazyLock::new(|| RwLock::new(None));

#[cfg(target_arch = "wasm32")]
pub(crate) static WCP_CS_HANDLER: LazyLock<IngressHandler<WcpCSMessage>> =
    LazyLock::new(IngressHandler::new);

#[cfg(target_arch = "wasm32")]
pub(crate) static WCP_SC_HANDLER: LazyLock<GlobalChannelTx<WcpSCMessage>> =
    LazyLock::new(GlobalChannelTx::new);

#[derive(Default)]
pub struct StartupParams {
    pub waves: Option<WaveSource>,
    pub wcp_initiate: Option<u16>,
    pub startup_commands: Vec<String>,
}

fn setup_custom_font(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "remix_icons".to_owned(),
        FontData::from_static(egui_remixicon::FONT).into(),
    );

    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .push("remix_icons".to_owned());

    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .push("remix_icons".to_owned());

    ctx.set_fonts(fonts);
}

pub fn run_egui(cc: &CreationContext, mut state: SystemState) -> Result<Box<dyn App>> {
    let ctx_arc = Arc::new(cc.egui_ctx.clone());
    *EGUI_CONTEXT.write().unwrap() = Some(ctx_arc.clone());
    state.context = Some(ctx_arc.clone());
    cc.egui_ctx
        .set_visuals_of(egui::Theme::Dark, state.get_visuals());
    cc.egui_ctx
        .set_visuals_of(egui::Theme::Light, state.get_visuals());
    cc.egui_ctx.all_styles_mut(|style| {
        if state.user.config.animation_time == 0.0 {
            info!("With animation_time set to 0.0, animations cannot be enabled.");
        }
        style.animation_time = if state.user.config.animation_enabled() {
            state.user.config.animation_time
        } else {
            0.0
        };
    });
    #[cfg(not(target_arch = "wasm32"))]
    if state.user.config.wcp.autostart {
        state.start_wcp_server(Some(state.user.config.wcp.address.clone()), false);
    }
    setup_custom_font(&cc.egui_ctx);
    Ok(Box::new(state))
}

#[derive(Debug, Clone, Copy, Deserialize, Display, PartialEq, Eq)]
pub enum MoveDir {
    #[display("up")]
    Up,

    #[display("down")]
    Down,
}

pub enum ColorSpecifier {
    Index(usize),
    Name(String),
}

enum CachedDrawData {
    WaveDrawData(CachedWaveDrawData),
    TransactionDrawData(CachedTransactionDrawData),
}

struct CachedWaveDrawData {
    pub draw_commands: HashMap<DisplayedFieldRef, drawing_canvas::DrawingCommands>,
    pub clock_edges: Vec<f32>,
    pub ticks: Vec<(String, f32)>,
}

struct CachedTransactionDrawData {
    pub draw_commands: HashMap<TransactionRef, TxDrawingCommands>,
    pub stream_to_displayed_txs: HashMap<TransactionStreamRef, Vec<TransactionRef>>,
    pub inc_relation_tx_ids: Vec<TransactionRef>,
    pub out_relation_tx_ids: Vec<TransactionRef>,
}

pub struct Channels {
    pub msg_sender: Sender<Message>,
    pub msg_receiver: Receiver<Message>,
    #[cfg(target_arch = "wasm32")]
    wcp_c2s_receiver: Option<IngressReceiver<WcpCSMessage>>,
    #[cfg(not(target_arch = "wasm32"))]
    wcp_c2s_receiver: Option<tokio::sync::mpsc::Receiver<WcpCSMessage>>,
    wcp_s2c_sender: Option<tokio::sync::mpsc::Sender<WcpSCMessage>>,
}
impl Channels {
    fn new() -> Self {
        let (msg_sender, msg_receiver) = mpsc::channel();
        Self {
            msg_sender,
            msg_receiver,
            wcp_c2s_receiver: None,
            wcp_s2c_sender: None,
        }
    }
}

pub struct WcpClientCapabilities {
    pub waveforms_loaded: bool,
    pub goto_declaration: bool,
    pub add_drivers: bool,
    pub add_loads: bool,
}
impl WcpClientCapabilities {
    fn new() -> Self {
        Self {
            waveforms_loaded: false,
            goto_declaration: false,
            add_drivers: false,
            add_loads: false,
        }
    }
}

/// Stores the current canvas state to enable undo/redo operations
struct CanvasState {
    message: String,
    focused_item: Option<VisibleItemIndex>,
    focused_transaction: (Option<TransactionRef>, Option<Transaction>),
    items_tree: DisplayedItemTree,
    displayed_items: HashMap<DisplayedItemRef, DisplayedItem>,
    markers: HashMap<u8, BigInt>,
}

impl SystemState {
    pub fn update(&mut self, message: Message) -> Option<()> {
        if tracing::enabled!(tracing::Level::TRACE)
            && !matches!(message, Message::CommandPromptUpdate { .. })
        {
            tracing::trace!("{message:?}");
        }
        match message {
            Message::SetActiveScope(scope) => {
                let waves = self.user.waves.as_mut()?;
                if let Some(scope) = scope {
                    let scope = if let ScopeType::StreamScope(StreamScopeRef::Empty(name)) = scope {
                        ScopeType::StreamScope(StreamScopeRef::new_stream_from_name(
                            waves.inner.as_transactions().unwrap(),
                            name,
                        ))
                    } else {
                        scope
                    };

                    if waves.inner.scope_exists(&scope) {
                        waves.active_scope = Some(scope);
                    } else {
                        warn!("Setting active scope to {scope} which does not exist");
                    }
                } else {
                    // Set to top-level scope
                    waves.active_scope = None;
                }
            }
            Message::ExpandScope(scope_ref) => {
                *self.scope_ref_to_expand.borrow_mut() = Some(scope_ref);
            }
            Message::AddVariables(vars) => {
                if !vars.is_empty() {
                    let undo_msg = if vars.len() == 1 {
                        format!("Add variable {}", vars[0].name)
                    } else {
                        format!("Add {} variables", vars.len())
                    };
                    self.save_current_canvas(undo_msg);
                    if let Some(waves) = self.user.waves.as_mut() {
                        if let (Some(cmd), _) =
                            waves.add_variables(&self.translators, vars, None, true, false, None)
                        {
                            self.load_variables(cmd);
                        }
                        self.invalidate_draw_commands();
                    } else {
                        error!("Could not load signals, no waveform loaded");
                    }
                }
            }
            Message::AddDivider(name, vidx) => {
                self.save_current_canvas("Add divider".into());
                let waves = self.user.waves.as_mut()?;
                waves.add_divider(name, vidx);
            }
            Message::AddTimeLine(vidx) => {
                self.save_current_canvas("Add timeline".into());
                let waves = self.user.waves.as_mut()?;
                waves.add_timeline(vidx);
            }
            Message::AddScope(scope, recursive) => {
                self.save_current_canvas(format!("Add scope {}", scope.name()));

                let vars = self.get_scope(scope, recursive);
                let waves = self.user.waves.as_mut()?;

                // TODO add parameter to add_variables, insert to (self.drag_target_idx, self.drag_source_idx)
                if let (Some(cmd), _) =
                    waves.add_variables(&self.translators, vars, None, true, false, None)
                {
                    self.load_variables(cmd);
                }

                self.invalidate_draw_commands();
            }
            Message::AddScopeAsGroup(scope, recursive) => {
                self.save_current_canvas(format!("Add scope {} as group", scope.name()));
                let waves = self.user.waves.as_mut()?;
                let passed_or_focused = waves.insert_position(waves.focused_item);
                let target = passed_or_focused.unwrap_or_else(|| waves.end_insert_position());

                self.add_scope_as_group(&scope, target, recursive, None);
                self.invalidate_draw_commands();

                self.user.waves.as_mut()?.compute_variable_display_names();
            }
            Message::AddCount(digit) => {
                if let Some(count) = &mut self.user.count {
                    count.push(digit);
                } else {
                    self.user.count = Some(digit.to_string());
                }
            }
            Message::AddStreamOrGenerator(s) => {
                let undo_msg = if let Some(gen_id) = s.gen_id {
                    format!("Add generator(id: {gen_id})")
                } else {
                    format!("Add stream(id: {})", s.stream_id)
                };
                self.save_current_canvas(undo_msg);

                let waves = self.user.waves.as_mut()?;
                if s.gen_id.is_some() {
                    waves.add_generator(s);
                } else {
                    waves.add_stream(s);
                }
                self.invalidate_draw_commands();
            }
            Message::AddStreamOrGeneratorFromName(scope, name) => {
                self.save_current_canvas(format!("Add Stream/Generator from name: {}", &name));
                let waves = self.user.waves.as_mut()?;
                let inner = waves.inner.as_transactions()?;
                match scope {
                    Some(StreamScopeRef::Root) => {
                        let (stream_id, name) = inner
                            .get_stream_from_name(name)
                            .map(|s| (s.id, s.name.clone()))
                            .unwrap();

                        waves.add_stream(TransactionStreamRef::new_stream(stream_id, name));
                    }
                    Some(StreamScopeRef::Stream(stream)) => {
                        let (stream_id, id, name) = inner
                            .get_generator_from_name(Some(stream.stream_id), name)
                            .map(|g| (g.stream_id, g.id, g.name.clone()))
                            .unwrap();

                        waves.add_generator(TransactionStreamRef::new_gen(stream_id, id, name));
                    }
                    Some(StreamScopeRef::Empty(_)) => {}
                    None => {
                        let (stream_id, id, name) = inner
                            .get_generator_from_name(None, name)
                            .map(|g| (g.stream_id, g.id, g.name.clone()))
                            .unwrap();

                        waves.add_generator(TransactionStreamRef::new_gen(stream_id, id, name));
                    }
                }
                self.invalidate_draw_commands();
            }
            Message::AddAllFromStreamScope(scope_name) => {
                self.save_current_canvas(format!("Add all from scope {}", scope_name.clone()));
                let waves = self.user.waves.as_mut()?;
                if scope_name == "tr" {
                    waves.add_all_streams();
                } else {
                    let inner = waves.inner.as_transactions()?;
                    let stream = inner.get_stream_from_name(scope_name)?;
                    let gens = stream
                        .generators
                        .iter()
                        .map(|gen_id| inner.get_generator(*gen_id).unwrap())
                        .map(|g| (g.stream_id, g.id, g.name.clone()))
                        .collect_vec();

                    for (stream_id, id, name) in gens {
                        waves.add_generator(TransactionStreamRef::new_gen(
                            stream_id,
                            id,
                            name.clone(),
                        ));
                    }
                }
                self.invalidate_draw_commands();
            }
            Message::InvalidateCount => self.user.count = None,
            Message::SetNameAlignRight(align_right) => {
                self.user.align_names_right = Some(align_right);
            }
            Message::FocusItem(idx) => {
                let waves = self.user.waves.as_mut()?;

                let visible_items_len = waves.displayed_items.len();
                if idx.0 < visible_items_len {
                    waves.focused_item = Some(idx);
                } else {
                    error!(
                        "Can not focus variable {} because only {visible_items_len} variables are visible.",
                        idx.0
                    );
                }
            }
            Message::ItemSelectRange(select_to) => {
                let waves = self.user.waves.as_mut()?;
                let select_from = waves.focused_item?;
                waves
                    .items_tree
                    .xselect_visible_range(select_from, select_to, true);
            }
            Message::ItemSelectAll => {
                let waves = self.user.waves.as_mut()?;
                waves.items_tree.xselect_all_visible(true);
            }
            Message::SetItemSelected(vidx, selected) => {
                let waves = self.user.waves.as_mut()?;
                waves.items_tree.xselect(vidx, selected);
            }
            Message::ToggleItemSelected(vidx) => {
                let waves = self.user.waves.as_mut()?;
                let node = vidx
                    .or(waves.focused_item)
                    .and_then(|vidx| waves.items_tree.to_displayed(vidx))
                    .and_then(|item| waves.items_tree.get_mut(item))?;
                node.selected = !node.selected;
            }
            Message::SetDefaultTimeline(v) => {
                self.user.show_default_timeline = Some(v);
            }
            Message::UnfocusItem => {
                let waves = self.user.waves.as_mut()?;
                waves.focused_item = None;
            }
            Message::MoveFocus(direction, count, select) => {
                let waves = self.user.waves.as_mut()?;
                let visible_item_cnt = waves.items_tree.iter_visible().count();
                if visible_item_cnt == 0 {
                    return None;
                }

                let new_focus_vidx = VisibleItemIndex(match direction {
                    MoveDir::Up => waves
                        .focused_item
                        .map_or(visible_item_cnt, |vidx| vidx.0)
                        .saturating_sub(count),
                    MoveDir::Down => waves
                        .focused_item
                        .map_or(usize::MAX, |vidx| vidx.0)
                        .wrapping_add(count)
                        .clamp(0, visible_item_cnt - 1),
                });

                if select {
                    if let Some(vidx) = waves.focused_item {
                        waves.items_tree.xselect(vidx, true);
                    }
                    waves.items_tree.xselect(new_focus_vidx, true);
                }
                waves.focused_item = Some(new_focus_vidx);
            }
            Message::FocusTransaction(tx_ref, tx) => {
                if let Some(tx_ref) = tx_ref.as_ref()
                    && tx.is_none()
                {
                    self.save_current_canvas(format!("Focus Transaction id: {}", tx_ref.id));
                }
                let waves = self.user.waves.as_mut()?;
                let invalidate = tx.is_none();
                waves.focused_transaction =
                    (tx_ref, tx.or_else(|| waves.focused_transaction.1.clone()));
                if invalidate {
                    self.invalidate_draw_commands();
                }
            }
            Message::ScrollToItem(position) => {
                let waves = self.user.waves.as_mut()?;
                waves.scroll_to_item(position);
            }
            Message::SetScrollOffset(offset) => {
                let waves = self.user.waves.as_mut()?;
                waves.scroll_offset = offset;
            }
            Message::SetLogsVisible(visibility) => self.user.show_logs = visibility,
            Message::SetCursorWindowVisible(visibility) => {
                self.user.show_cursor_window = visibility;
            }
            Message::VerticalScroll(direction, count) => {
                let waves = self.user.waves.as_mut()?;
                let current_item = waves.get_top_item();
                match direction {
                    MoveDir::Down => {
                        waves.scroll_to_item(current_item + count);
                    }
                    MoveDir::Up => {
                        if current_item > count {
                            waves.scroll_to_item(current_item - count);
                        } else {
                            waves.scroll_to_item(0);
                        }
                    }
                }
            }
            Message::SetSurverFileWindowVisible(visibility) => {
                self.user.show_server_file_window = visibility;
            }
            Message::LoadSurverFileByIndex(file_index, load_options) => {
                // Disable file window in case executing from command/test
                self.user.show_server_file_window = false;
                let force_switch = self.user.selected_server_file_index != file_index;
                self.user.selected_server_file_index = file_index;
                *self.surver_selected_file.borrow_mut() = file_index;
                if let Some(url) = self.user.surver_url.as_ref() {
                    self.load_wave_from_url(url.to_string(), load_options, force_switch);
                }
            }
            Message::LoadSurverFileByName(file_name, load_options) => {
                // Disable file window in case executing from command/test
                self.user.show_server_file_window = false;
                let file_index = self
                    .user
                    .surver_file_infos
                    .as_ref()?
                    .iter()
                    .position(|fi| fi.filename == file_name);
                let force_switch = self.user.selected_server_file_index != file_index;

                self.user.selected_server_file_index = file_index;
                *self.surver_selected_file.borrow_mut() = file_index;
                if let Some(url) = self.user.surver_url.as_ref() {
                    self.load_wave_from_url(url.to_string(), load_options, force_switch);
                }
            }
            Message::RemoveVisibleItems(target) => match target {
                MessageTarget::Explicit(vidx) => {
                    let waves = self.user.waves.as_ref();
                    let item_ref = waves
                        .and_then(|waves| waves.items_tree.get_visible(vidx))
                        .map(|node| node.item_ref);
                    let undo_msg = item_ref
                        .and_then(|item_ref| {
                            waves.and_then(|waves| waves.displayed_items.get(&item_ref))
                        })
                        .map(displayed_item::DisplayedItem::name)
                        .map_or("Remove one item".to_string(), |name| {
                            format!("Remove item {name}")
                        });
                    self.save_current_canvas(undo_msg);

                    if let Some(waves) = self.user.waves.as_mut()
                        && let Some(item_ref) = item_ref
                    {
                        waves.remove_displayed_item(item_ref);
                    }
                }
                MessageTarget::CurrentSelection => {
                    self.save_current_canvas("Remove selected items".to_owned());
                    let waves = self.user.waves.as_mut()?;

                    let mut remove_ids: Vec<_> = waves
                        .items_tree
                        .iter_visible_selected()
                        .map(|node| node.item_ref)
                        .collect();
                    if let Some(node) = waves
                        .focused_item
                        .and_then(|focus| waves.items_tree.get_visible(focus))
                    {
                        remove_ids.push(node.item_ref);
                    }
                    for &item_ref in remove_ids.iter() {
                        waves.remove_displayed_item(item_ref);
                    }
                }
            },
            Message::RemoveItems(items) => {
                let undo_msg = self
                    .user
                    .waves
                    .as_ref()
                    .and_then(|waves| {
                        if items.len() == 1 {
                            items.first().and_then(|item_ref| {
                                waves
                                    .displayed_items
                                    .get(item_ref)
                                    .map(|item| format!("Remove item {}", item.name()))
                            })
                        } else {
                            Some(format!("Remove {} items", items.len()))
                        }
                    })
                    .unwrap_or_default();
                self.save_current_canvas(undo_msg);

                let waves = self.user.waves.as_mut()?;
                for id in items.iter().sorted_unstable_by(|a, b| Ord::cmp(b, a)) {
                    waves.remove_displayed_item(*id);
                }
            }
            Message::MoveFocusedItem(direction, count) => {
                self.save_current_canvas(format!("Move item {direction}, {count}"));
                self.invalidate_draw_commands();
                let waves = self.user.waves.as_mut()?;
                let mut vidx = waves.focused_item?;
                for _ in 0..count {
                    vidx = waves
                        .items_tree
                        .move_item(vidx, direction, |node| {
                            matches!(
                                waves.displayed_items.get(&node.item_ref),
                                Some(DisplayedItem::Group(..))
                            )
                        })
                        .expect("move failed for unknown reason");
                }
                waves.focused_item = waves.focused_item.and(Some(vidx));
            }
            Message::CanvasScroll {
                delta,
                viewport_idx,
            } => {
                let waves = self.user.waves.as_mut()?;
                waves.viewports[viewport_idx]
                    .handle_canvas_scroll(f64::from(delta.y) + f64::from(delta.x));
                self.invalidate_draw_commands();
            }
            Message::CanvasZoom {
                delta,
                mouse_ptr,
                viewport_idx,
            } => {
                let waves = self.user.waves.as_mut()?;
                if let Some(num_timestamps) = waves.num_timestamps() {
                    waves.viewports[viewport_idx].handle_canvas_zoom(
                        mouse_ptr,
                        f64::from(delta),
                        &num_timestamps,
                    );
                    self.invalidate_draw_commands();
                } else {
                    warn!(
                        "Canvas zoom: No timestamps count, even though waveforms should be loaded"
                    );
                }
            }
            Message::ZoomToFit { viewport_idx } => {
                let waves = self.user.waves.as_mut()?;
                waves.viewports[viewport_idx].zoom_to_fit();
                self.invalidate_draw_commands();
            }
            Message::GoToEnd { viewport_idx } => {
                let waves = self.user.waves.as_mut()?;
                waves.viewports[viewport_idx].go_to_end();
                self.invalidate_draw_commands();
            }
            Message::GoToStart { viewport_idx } => {
                let waves = self.user.waves.as_mut()?;
                waves.viewports[viewport_idx].go_to_start();
                self.invalidate_draw_commands();
            }
            Message::GoToTime(time, viewport_idx) => {
                let waves = self.user.waves.as_mut()?;
                // If there are no timestamps, the file is not fully loaded
                if let Some(num_timestamps) = waves.num_timestamps() {
                    let time = time?;
                    waves.viewports[viewport_idx].go_to_time(&time.clone(), &num_timestamps);
                    self.invalidate_draw_commands();
                } else {
                    warn!(
                        "Go to time: No timestamps count, even though waveforms should be loaded"
                    );
                }
            }
            Message::SetTimeUnit(timeunit) => {
                self.user.wanted_timeunit = timeunit;
                self.invalidate_draw_commands();
            }
            Message::SetTimeStringFormatting(format) => {
                self.user.time_string_format = format;
                self.invalidate_draw_commands();
            }
            Message::ZoomToRange {
                start,
                end,
                viewport_idx,
            } => {
                let waves = self.user.waves.as_mut()?;
                // If there are no timestamps, the file is not fully loaded
                if let Some(num_timestamps) = waves.num_timestamps() {
                    waves.viewports[viewport_idx].zoom_to_range(&start, &end, &num_timestamps);
                    self.invalidate_draw_commands();
                } else {
                    warn!(
                        "Zoom to range: No timestamps count, even though waveforms should be loaded"
                    );
                }
            }
            Message::VariableFormatChange(displayed_field_ref, format) => {
                let waves = self.user.waves.as_mut()?;
                if !self
                    .translators
                    .all_translator_names()
                    .contains(&format.as_str())
                {
                    warn!("No translator {format}");
                    return None;
                }

                let update_format =
                    |variable: &mut DisplayedVariable, field_ref: DisplayedFieldRef| {
                        if field_ref.field.is_empty() {
                            let Ok(meta) = waves
                                .inner
                                .as_waves()
                                .unwrap()
                                .variable_meta(&variable.variable_ref)
                                .map_err(|e| {
                                    warn!("Error trying to get variable metadata: {e:#?}");
                                })
                            else {
                                return;
                            };
                            let translator = self.translators.get_translator(&format);
                            let new_info = translator.variable_info(&meta).unwrap();

                            variable.format = Some(format.clone());
                            variable.info = new_info;
                        } else {
                            variable
                                .field_formats
                                .retain(|ff| ff.field != field_ref.field);
                            variable.field_formats.push(FieldFormat {
                                field: field_ref.field,
                                format: format.clone(),
                            });
                        }
                    };

                // convert focused item index to item ref
                let focused = waves
                    .focused_item
                    .and_then(|vidx| waves.items_tree.get_visible(vidx))
                    .map(|node| node.item_ref);

                let mut redraw = false;

                match displayed_field_ref {
                    MessageTarget::Explicit(field_ref) => {
                        if let Some(DisplayedItem::Variable(displayed_variable)) =
                            waves.displayed_items.get_mut(&field_ref.item)
                        {
                            update_format(displayed_variable, field_ref);
                            redraw = true;
                        }
                    }
                    MessageTarget::CurrentSelection => {
                        //If an item is focused, update its format too
                        if let Some(focused) = focused
                            && let Some(DisplayedItem::Variable(displayed_variable)) =
                                waves.displayed_items.get_mut(&focused)
                        {
                            update_format(displayed_variable, DisplayedFieldRef::from(focused));
                            redraw = true;
                        }
                        for item in waves
                            .items_tree
                            .iter_visible_selected()
                            .map(|node| node.item_ref)
                        {
                            //Update format for all selected
                            let field_ref = DisplayedFieldRef::from(item);
                            if let Some(DisplayedItem::Variable(variable)) =
                                waves.displayed_items.get_mut(&item)
                            {
                                update_format(variable, field_ref);
                            }
                            redraw = true;
                        }
                    }
                }

                if redraw {
                    self.invalidate_draw_commands();
                }
            }
            Message::ItemSelectionClear => {
                let waves = self.user.waves.as_mut()?;
                waves.items_tree.xselect_all_visible(false);
            }
            Message::ItemColorChange(vidx, color_name) => {
                self.save_current_canvas(format!(
                    "Change item color to {}",
                    color_name.clone().unwrap_or("default".into())
                ));
                self.invalidate_draw_commands();
                let waves = self.user.waves.as_mut()?;

                match vidx {
                    MessageTarget::Explicit(vidx) => {
                        let node = waves.items_tree.get_visible(vidx)?;
                        waves
                            .displayed_items
                            .entry(node.item_ref)
                            .and_modify(|item| item.set_color(&color_name));
                    }
                    MessageTarget::CurrentSelection => {
                        if let Some(focused) = waves.focused_item {
                            let node = waves.items_tree.get_visible(focused)?;
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(|item| item.set_color(&color_name));
                        }

                        for node in waves.items_tree.iter_visible_selected() {
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(|item| item.set_color(&color_name));
                        }
                    }
                }
            }
            Message::ItemNameChange(vidx, name) => {
                self.save_current_canvas(format!(
                    "Change item name to {}",
                    name.clone().unwrap_or("default".into())
                ));
                let waves = self.user.waves.as_mut()?;
                let vidx = vidx.or(waves.focused_item)?;
                let node = waves.items_tree.get_visible(vidx)?;
                waves
                    .displayed_items
                    .entry(node.item_ref)
                    .and_modify(|item| item.set_name(name));
            }
            Message::ItemNameReset(target) => {
                self.save_current_canvas("Resetting item name(s)".to_owned());
                let waves = self.user.waves.as_mut()?;
                match target {
                    MessageTarget::Explicit(vidx) => {
                        let node = waves.items_tree.get_visible(vidx)?;
                        waves
                            .displayed_items
                            .entry(node.item_ref)
                            .and_modify(|item| item.set_name(None));
                    }
                    MessageTarget::CurrentSelection => {
                        for node in waves.items_tree.iter_visible_selected() {
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(|item| item.set_name(None));
                        }
                    }
                }
            }
            Message::ItemBackgroundColorChange(vidx, color_name) => {
                self.save_current_canvas(format!(
                    "Change item background color to {}",
                    color_name.clone().unwrap_or("default".into())
                ));
                let waves = self.user.waves.as_mut()?;

                match vidx {
                    MessageTarget::Explicit(vidx) => {
                        let node = waves.items_tree.get_visible(vidx)?;
                        waves
                            .displayed_items
                            .entry(node.item_ref)
                            .and_modify(|item| item.set_background_color(&color_name));
                    }
                    MessageTarget::CurrentSelection => {
                        if let Some(focused) = waves.focused_item {
                            let node = waves.items_tree.get_visible(focused)?;
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(|item| item.set_background_color(&color_name));
                        }

                        for node in waves.items_tree.iter_visible_selected() {
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(|item| item.set_background_color(&color_name));
                        }
                    }
                }
            }
            Message::ItemHeightScalingFactorChange(vidx, scale) => {
                self.save_current_canvas(format!("Change item height scaling factor to {scale}"));
                let waves = self.user.waves.as_mut()?;

                match vidx {
                    MessageTarget::Explicit(vidx) => {
                        let node = waves.items_tree.get_visible(vidx)?;
                        waves
                            .displayed_items
                            .entry(node.item_ref)
                            .and_modify(|item| item.set_height_scaling_factor(scale));
                    }
                    MessageTarget::CurrentSelection => {
                        if let Some(focused) = waves.focused_item {
                            let node = waves.items_tree.get_visible(focused)?;
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(|item| item.set_height_scaling_factor(scale));
                        }

                        for node in waves.items_tree.iter_visible_selected() {
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(|item| item.set_height_scaling_factor(scale));
                        }
                    }
                }
            }
            Message::SetAnalogSettings(vidx, new_settings) => {
                self.save_current_canvas("Set analog state".into());
                self.invalidate_draw_commands();
                let waves = self.user.waves.as_mut()?;

                // Update settings while preserving existing cache
                let update = |item: &mut DisplayedItem| {
                    if let DisplayedItem::Variable(var) = item {
                        match (&mut var.analog, new_settings) {
                            (Some(s), Some(new)) => s.settings = new,
                            (None, Some(new)) => var.analog = Some(AnalogVarState::new(new)),
                            (_, None) => var.analog = None,
                        }
                    }
                };

                match vidx {
                    MessageTarget::Explicit(vidx) => {
                        let node = waves.items_tree.get_visible(vidx)?;
                        waves
                            .displayed_items
                            .entry(node.item_ref)
                            .and_modify(update);
                    }
                    MessageTarget::CurrentSelection => {
                        if let Some(focused) = waves.focused_item {
                            let node = waves.items_tree.get_visible(focused)?;
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(update);
                        }
                        for node in waves.items_tree.iter_visible_selected() {
                            waves
                                .displayed_items
                                .entry(node.item_ref)
                                .and_modify(update);
                        }
                    }
                }
            }
            Message::MoveCursorToTransition {
                next,
                variable,
                skip_zero,
            } => {
                let waves = self.user.waves.as_mut()?;
                // If there are no timestamps, the file is not fully loaded
                if let Some(num_timestamps) = waves.num_timestamps() {
                    // if no cursor is set, move it to
                    // start of visible area transition for next transition
                    // end of visible area for previous transition
                    if waves.cursor.is_none()
                        && waves.focused_item.is_some()
                        && let Some(vp) = waves.viewports.first()
                    {
                        waves.cursor = if next {
                            Some(vp.left_edge_time(&num_timestamps))
                        } else {
                            Some(vp.right_edge_time(&num_timestamps))
                        };
                    }
                    waves.set_cursor_at_transition(next, variable, skip_zero);
                    let moved = waves.go_to_cursor_if_not_in_view();
                    if moved {
                        self.invalidate_draw_commands();
                    }
                } else {
                    warn!(
                        "Move cursor to transition: No timestamps count, even though waveforms should be loaded"
                    );
                }
            }
            Message::MoveTransaction { next } => {
                let undo_msg = if next {
                    "Move to next transaction"
                } else {
                    "Move to previous transaction"
                };
                self.save_current_canvas(undo_msg.to_string());
                let waves = self.user.waves.as_mut()?;
                let inner = waves.inner.as_transactions()?;
                let mut transactions = waves
                    .items_tree
                    .iter_visible()
                    .flat_map(|node| {
                        let item = &waves.displayed_items[&node.item_ref];
                        match item {
                            DisplayedItem::Stream(s) => {
                                let stream_ref = &s.transaction_stream_ref;
                                let stream_id = stream_ref.stream_id;
                                if let Some(gen_id) = stream_ref.gen_id {
                                    inner.get_transactions_from_generator(gen_id)
                                } else {
                                    inner.get_transactions_from_stream(stream_id)
                                }
                            }
                            _ => vec![],
                        }
                    })
                    .collect_vec();

                transactions.sort_unstable();
                let tx = if let Some(focused_tx) = &waves.focused_transaction.0 {
                    let next_id = transactions
                        .iter()
                        .enumerate()
                        .find(|(_, tx)| **tx == focused_tx.id)
                        .map_or(
                            if next { transactions.len() - 1 } else { 0 },
                            |(vec_idx, _)| {
                                if next {
                                    if vec_idx + 1 < transactions.len() {
                                        vec_idx + 1
                                    } else {
                                        transactions.len() - 1
                                    }
                                } else {
                                    vec_idx.saturating_sub(1)
                                }
                            },
                        );
                    Some(TransactionRef {
                        id: *transactions.get(next_id).unwrap(),
                    })
                } else if !transactions.is_empty() {
                    Some(TransactionRef {
                        id: *transactions.first().unwrap(),
                    })
                } else {
                    None
                };
                waves.focused_transaction = (tx, waves.focused_transaction.1.clone());

                self.invalidate_draw_commands();
            }
            Message::ResetVariableFormat(displayed_field_ref) => {
                let waves = self.user.waves.as_mut()?;
                if let Some(DisplayedItem::Variable(displayed_variable)) =
                    waves.displayed_items.get_mut(&displayed_field_ref.item)
                {
                    if displayed_field_ref.field.is_empty() {
                        displayed_variable.format = None;
                    } else {
                        displayed_variable
                            .field_formats
                            .retain(|ff| ff.field != displayed_field_ref.field);
                    }
                    self.invalidate_draw_commands();
                }
            }
            Message::CursorSet(time) => {
                let waves = self.user.waves.as_mut()?;
                waves.cursor = Some(time);
            }
            Message::ExpandParameterSection => {
                self.expand_parameter_section = true;
            }
            Message::LoadFile(filename, load_options) => {
                self.user.selected_server_file_index = None;
                *self.surver_selected_file.borrow_mut() = None;
                #[cfg(not(target_arch = "wasm32"))]
                self.load_from_file(filename, load_options).ok();
                #[cfg(target_arch = "wasm32")]
                error!("Cannot load file from path in WASM");
            }
            Message::LoadWaveformFileFromUrl(url, load_options) => {
                self.user.selected_server_file_index = None;
                *self.surver_selected_file.borrow_mut() = None;
                // If we provide URL at command line and it is a Surver URL, we want to force a switch
                self.load_wave_from_url(url, load_options, true);
            }
            Message::LoadFromData(data, load_options) => {
                self.user.selected_server_file_index = None;
                *self.surver_selected_file.borrow_mut() = None;
                self.load_from_data(data, load_options).ok();
            }
            #[cfg(feature = "python")]
            Message::LoadPythonTranslator(filename) => {
                try_log_error!(
                    self.translators.load_python_translator(filename),
                    "Error loading Python translator",
                )
            }
            #[cfg(all(not(target_arch = "wasm32"), feature = "wasm_plugins"))]
            Message::LoadWasmTranslator(path) => {
                let sender = self.channels.msg_sender.clone();
                perform_work(
                    move || match PluginTranslator::new(path.into_std_path_buf()) {
                        Ok(t) => {
                            if let Err(e) = sender.send(Message::TranslatorLoaded(Arc::new(t))) {
                                error!("Failed to send message: {e}");
                            }
                        }
                        Err(e) => {
                            error!("Failed to load wasm translator {e:#}");
                        }
                    },
                );
            }
            Message::LoadCommandFile(path) => {
                self.add_batch_commands(read_command_file(&path));
            }
            Message::LoadCommandFileFromUrl(url) => {
                self.load_commands_from_url(url);
            }
            Message::LoadCommandFromData(bytes) => {
                self.add_batch_commands(read_command_bytes(bytes));
            }
            Message::SetupCxxrtl(kind) => self.connect_to_cxxrtl(kind, false),
            Message::SetSurverStatus(_start, server, status) => {
                self.user.surver_file_infos = Some(status.file_infos.clone());
                info!(
                    "Received surfer server status from {server}. {} files available.",
                    status.file_infos.len()
                );
                self.user.surver_url = Some(server.clone());
                if status.file_infos.is_empty() {
                    warn!("Received surfer server status with no file infos");
                    return None;
                }
                if self.user.selected_server_file_index.is_none() {
                    if status.file_infos.len() == 1 {
                        // if only one file is available, select it automatically
                        self.user.selected_server_file_index = Some(0);
                        *self.surver_selected_file.borrow_mut() = Some(0);
                        info!(
                            "Only one file available on server {}, loading it automatically",
                            server
                        );
                        self.load_wave_from_url(server.clone(), LoadOptions::Clear, false);
                    } else {
                        // if no file is selected, show the server file selection window
                        self.user.show_server_file_window = true;
                        self.progress_tracker = None;
                    }
                }

                if let Some(file_index) = self.user.selected_server_file_index {
                    if file_index >= status.file_infos.len() {
                        warn!(
                            "Selected server file index {file_index} is out of bounds ({} files available)",
                            status.file_infos.len()
                        );
                        return None;
                    }
                    self.server_status_to_progress(&server, &status.file_infos[file_index]);
                }
            }
            Message::FileDropped(dropped_file) => {
                self.load_from_dropped(dropped_file)
                    .map_err(|e| error!("{e:#?}"))
                    .ok();
            }
            Message::StopProgressTracker => {
                self.progress_tracker = None;
            }
            Message::WaveHeaderLoaded(start, source, load_options, header) => {
                // for files using the `wellen` backend, we load the header before parsing the body
                info!(
                    "Loaded the hierarchy and meta-data of {source} in {:?}",
                    start.elapsed()
                );
                match header {
                    HeaderResult::LocalFile(header) => {
                        // register waveform as loaded (but with no variable info yet!)
                        let shared_hierarchy = Arc::new(header.hierarchy);
                        let new_waves =
                            Box::new(WaveContainer::new_waveform(shared_hierarchy.clone()));
                        self.on_waves_loaded(
                            source.clone(),
                            convert_format(header.file_format),
                            new_waves,
                            load_options,
                        );
                        // start parsing of the body
                        self.load_wave_body(source, header.body, header.body_len, shared_hierarchy);
                    }
                    HeaderResult::LocalBytes(header) => {
                        // register waveform as loaded (but with no variable info yet!)
                        let shared_hierarchy = Arc::new(header.hierarchy);
                        let new_waves =
                            Box::new(WaveContainer::new_waveform(shared_hierarchy.clone()));
                        self.on_waves_loaded(
                            source.clone(),
                            convert_format(header.file_format),
                            new_waves,
                            load_options,
                        );
                        // start parsing of the body
                        self.load_wave_body(source, header.body, header.body_len, shared_hierarchy);
                    }
                    HeaderResult::Remote(hierarchy, file_format, server, file_index) => {
                        // register waveform as loaded (but with no variable info yet!)
                        let new_waves = Box::new(WaveContainer::new_remote_waveform(
                            &server,
                            hierarchy.clone(),
                        ));
                        self.on_waves_loaded(
                            source.clone(),
                            convert_format(file_format),
                            new_waves,
                            load_options,
                        );
                        // body is already being parsed on the server, we need to request the time table though
                        get_time_table_from_server(
                            self.channels.msg_sender.clone(),
                            server,
                            file_index,
                        );
                    }
                }
            }
            Message::WaveBodyLoaded(start, source, body) => {
                // for files using the `wellen` backend, parse the body in a second step
                info!("Loaded the body of {source} in {:?}", start.elapsed());
                self.progress_tracker = None;
                let waves = self
                    .user
                    .waves
                    .as_mut()
                    .expect("Waves should be loaded at this point!");
                // add source and time table
                let maybe_cmd = waves // TODO
                    .inner
                    .as_waves_mut()
                    .unwrap()
                    .wellen_add_body(body)
                    .map_err(|err| {
                        error!("While getting commands to lazy-load signals: {err:?}");
                    })
                    .ok()
                    .flatten();
                // Pre-load parameters
                let param_cmd = waves
                    .inner
                    .as_waves_mut()
                    .unwrap()
                    .load_parameters()
                    .map_err(|err| {
                        error!("While getting commands to lazy-load parameters: {err:?}");
                    })
                    .ok()
                    .flatten();

                if self.wcp_greeted_signal.load(Ordering::Relaxed)
                    && self.wcp_client_capabilities.waveforms_loaded
                {
                    let source = match source {
                        WaveSource::File(path) => path.to_string(),
                        WaveSource::Url(url) => url,
                        _ => String::new(),
                    };
                    self.channels.wcp_s2c_sender.as_ref().map(|ch| {
                        block_on(
                            ch.send(WcpSCMessage::event(WcpEvent::waveforms_loaded { source })),
                        )
                    });
                }

                // update viewports, now that we have the time table
                waves.update_viewports();
                // make sure we redraw
                self.invalidate_draw_commands();
                // start loading parameters
                if let Some(cmd) = param_cmd {
                    self.load_variables(cmd);
                }
                // start loading variables
                if let Some(cmd) = maybe_cmd {
                    self.load_variables(cmd);
                }
            }
            Message::SignalsLoaded(start, res) => {
                info!("Loaded {} variables in {:?}", res.len(), start.elapsed());
                self.progress_tracker = None;
                let waves = self
                    .user
                    .waves
                    .as_mut()
                    .expect("Waves should be loaded at this point!");
                match waves.inner.as_waves_mut().unwrap().on_signals_loaded(res) {
                    Err(err) => error!("{err:?}"),
                    Ok(Some(cmd)) => self.load_variables(cmd),
                    _ => {}
                }
                // make sure we redraw since now more variable data is available
                self.invalidate_draw_commands();
            }
            Message::WavesLoaded(filename, format, new_waves, load_options) => {
                self.on_waves_loaded(filename, format, new_waves, load_options);
                // here, the body and thus the number of timestamps is already loaded!
                self.user
                    .waves
                    .as_mut()
                    .expect("Waves should be loaded at this point!")
                    .update_viewports();
                self.progress_tracker = None;
            }
            Message::TransactionStreamsLoaded(filename, format, new_ftr, loaded_options) => {
                self.on_transaction_streams_loaded(filename, format, new_ftr, loaded_options);
                self.user
                    .waves
                    .as_mut()
                    .expect("Waves should be loaded at this point!")
                    .update_viewports();
            }
            Message::BlacklistTranslator(idx, translator) => {
                self.user.blacklisted_translators.insert((idx, translator));
            }
            Message::Error(e) => {
                error!("{e:?}");
                self.user.show_logs = true;
            }
            Message::TranslatorLoaded(t) => {
                info!("Translator {} loaded", t.name());
                t.set_wave_source(
                    self.user
                        .waves
                        .as_ref()
                        .map(|waves| waves.source.into_translation_type()),
                );

                self.translators.add_or_replace(AnyTranslator::Full(t));
            }
            Message::SetSidePanelVisible(v) => self.user.show_hierarchy = Some(v),
            Message::SetMenuVisible(v) => self.user.show_menu = Some(v),
            Message::ToggleMenu => {
                self.user.show_menu = Some(!self.show_menu());
            }
            Message::SetToolbarVisible(v) => self.user.show_toolbar = Some(v),
            Message::SetShowEmptyScopes(v) => self.user.show_empty_scopes = Some(v),
            Message::SetShowHierarchyIcons(v) => self.user.show_hierarchy_icons = Some(v),
            Message::SetParameterDisplayLocation(location) => {
                self.user.parameter_display_location = Some(location);
            }
            Message::SetStatusbarVisible(v) => self.user.show_statusbar = Some(v),
            Message::SetTickLines(v) => self.user.show_ticks = Some(v),
            Message::SetVariableTooltip(v) => self.user.show_tooltip = Some(v),
            Message::SetScopeTooltip(v) => self.user.show_scope_tooltip = Some(v),
            Message::SetOverviewVisible(v) => self.user.show_overview = Some(v),
            Message::SetShowVariableDirection(v) => self.user.show_variable_direction = Some(v),
            Message::SetTransitionValue(v) => self.user.transition_value = Some(v),
            Message::SetShowIndices(v) => {
                let new = v;
                self.user.show_variable_indices = Some(new);
                let waves = self.user.waves.as_mut()?;
                waves.display_variable_indices = new;
                waves.compute_variable_display_names();
            }
            Message::SetHighlightFocused(highlight) => {
                self.user.highlight_focused = Some(highlight);
            }
            Message::HideCommandPrompt => {
                *self.command_prompt_text.borrow_mut() = String::new();
                self.command_prompt.suggestions = vec![];
                self.command_prompt.selected = self.command_prompt.previous_commands.len();
                self.command_prompt.visible = false;
            }
            Message::ShowCommandPrompt(text, selected) => {
                self.command_prompt.new_text = Some((text, selected.unwrap_or(String::new())));
                self.command_prompt.visible = true;
            }
            Message::FileDownloaded(url, bytes, load_options) => {
                self.load_from_bytes(WaveSource::Url(url), bytes.to_vec(), load_options);
            }
            Message::CommandFileDownloaded(_url, bytes) => {
                self.add_batch_commands(read_command_bytes(bytes.to_vec()));
                self.progress_tracker = None;
            }
            Message::SetConfigFromString(s) => {
                // FIXME think about a structured way to collect errors
                let config = SurferConfig::new_from_toml(&s)
                    .with_context(|| "Failed to load config file")
                    .ok()?;

                self.user.config = config;

                let ctx = &self.context.as_ref()?;
                ctx.set_visuals(self.get_visuals());
            }
            Message::ReloadConfig => {
                // FIXME think about a structured way to collect errors
                let config = SurferConfig::new(false)
                    .with_context(|| "Failed to load config file")
                    .ok()?;
                self.translators = all_translators();
                self.user.config = config;

                let ctx = &self.context.as_ref()?;
                ctx.set_visuals(self.get_visuals());
            }
            Message::ReloadWaveform(keep_unavailable) => {
                let waves = self.user.waves.as_ref()?;
                let options = if keep_unavailable {
                    LoadOptions::KeepAll
                } else {
                    LoadOptions::KeepAvailable
                };
                match &waves.source {
                    WaveSource::File(filename) => {
                        self.load_from_file(filename.clone(), options).ok();
                    }
                    WaveSource::Data => {}       // can't reload
                    WaveSource::Cxxrtl(..) => {} // can't reload
                    WaveSource::DragAndDrop(filename) => {
                        filename
                            .clone()
                            .and_then(|filename| self.load_from_file(filename, options).ok());
                    }
                    WaveSource::Url(url) => {
                        self.load_wave_from_url(url.clone(), options, false);
                    }
                }

                for translator in self.translators.all_translators() {
                    translator.reload(self.channels.msg_sender.clone());
                }
                self.variable_name_info_cache.borrow_mut().clear();
            }
            Message::SuggestReloadWaveform => match self.autoreload_files() {
                AutoLoad::Always => self.update(Message::ReloadWaveform(true))?,
                AutoLoad::Never => (),
                AutoLoad::Ask => {
                    self.user.show_reload_suggestion = Some(ReloadWaveformDialog::default());
                }
            },
            Message::CloseReloadWaveformDialog {
                reload_file,
                do_not_show_again,
            } => {
                if do_not_show_again {
                    // FIXME: This is currently saved in state, but could be persisted in
                    // some setting.
                    self.user.autoreload_files = Some(AutoLoad::from_bool(reload_file));
                }
                self.user.show_reload_suggestion = None;
                if reload_file {
                    self.update(Message::ReloadWaveform(true));
                }
            }
            Message::UpdateReloadWaveformDialog(dialog) => {
                self.user.show_reload_suggestion = Some(dialog);
            }
            Message::OpenSiblingStateFile(open) => {
                if !open {
                    return None;
                }
                let waves = self.user.waves.as_ref()?;
                let state_file_path = waves.source.sibling_state_file()?;
                self.load_state_file(Some(state_file_path.clone().into_std_path_buf()));
            }
            Message::SuggestOpenSiblingStateFile => match self.autoload_sibling_state_files() {
                AutoLoad::Always => {
                    self.update(Message::OpenSiblingStateFile(true));
                }
                AutoLoad::Never => {}
                AutoLoad::Ask => {
                    self.user.show_open_sibling_state_file_suggestion =
                        Some(OpenSiblingStateFileDialog::default());
                }
            },
            Message::CloseOpenSiblingStateFileDialog {
                load_state,
                do_not_show_again,
            } => {
                if do_not_show_again {
                    self.user.autoload_sibling_state_files = Some(AutoLoad::from_bool(load_state));
                }
                self.user.show_open_sibling_state_file_suggestion = None;
                if load_state {
                    self.update(Message::OpenSiblingStateFile(true));
                }
            }
            Message::UpdateOpenSiblingStateFileDialog(dialog) => {
                self.user.show_open_sibling_state_file_suggestion = Some(dialog);
            }
            Message::RemovePlaceholders => {
                let waves = self.user.waves.as_mut()?;
                waves.remove_placeholders();
            }
            Message::SetClockHighlightType(new_type) => {
                self.user.clock_highlight_type = Some(new_type);
            }
            Message::SetFillHighValues(fill) => self.user.fill_high_values = Some(fill),
            Message::SetDinotraceStyle(dino_style) => {
                self.user.use_dinotrace_style = Some(dino_style);
                self.invalidate_draw_commands();
            }
            Message::AddMarker {
                time,
                name,
                move_focus,
            } => {
                if let Some(name) = &name {
                    self.save_current_canvas(format!("Add marker {name} at {time}"));
                } else {
                    self.save_current_canvas(format!("Add marker at {time}"));
                }
                let waves = self.user.waves.as_mut()?;
                waves.add_marker(&time, name, move_focus);
            }
            Message::SetMarker { id, time } => {
                self.save_current_canvas(format!("Set marker {id} to {time}"));
                let waves = self.user.waves.as_mut()?;
                waves.set_marker_position(id, &time);
            }
            Message::RemoveMarker(id) => {
                let waves = self.user.waves.as_mut()?;
                waves.remove_marker(id);
            }
            Message::MoveMarkerToCursor(idx) => {
                self.save_current_canvas("Move marker".into());
                let waves = self.user.waves.as_mut()?;
                waves.move_marker_to_cursor(idx);
            }
            Message::GoToCursorIfNotInView => {
                let waves = self.user.waves.as_mut()?;
                if waves.go_to_cursor_if_not_in_view() {
                    self.invalidate_draw_commands();
                }
            }
            Message::GoToMarkerPosition(idx, viewport_idx) => {
                let waves = self.user.waves.as_mut()?;
                // If there are no timestamps, the file is not fully loaded
                if let Some(num_timestamps) = waves.num_timestamps() {
                    let cursor = waves.markers.get(&idx)?;
                    waves.viewports[viewport_idx].go_to_time(cursor, &num_timestamps);
                    self.invalidate_draw_commands();
                } else {
                    warn!(
                        "Go to marker position: No timestamps count, even though waveforms should be loaded"
                    );
                }
            }
            Message::ChangeVariableNameType(target, name_type) => {
                let waves = self.user.waves.as_mut()?;
                let mut recompute_names = false;
                let mut change_type = |item_ref: DisplayedItemRef| {
                    waves.displayed_items.entry(item_ref).and_modify(|item| {
                        if let DisplayedItem::Variable(variable) = item {
                            variable.display_name_type = name_type;
                            recompute_names = true;
                        }
                    });
                };

                match target {
                    MessageTarget::Explicit(vidx) => {
                        if let Some(item_ref) =
                            waves.items_tree.get_visible(vidx).map(|node| node.item_ref)
                        {
                            change_type(item_ref);
                        }
                    }
                    MessageTarget::CurrentSelection => {
                        waves
                            .items_tree
                            .iter_visible_selected()
                            .for_each(|node| change_type(node.item_ref));
                        waves
                            .focused_item
                            .and_then(|vidx| waves.items_tree.get_visible(vidx))
                            .map(|node| node.item_ref)
                            .map(change_type);
                    }
                }

                if recompute_names {
                    waves.compute_variable_display_names();
                }
            }
            Message::ForceVariableNameTypes(name_type) => {
                let waves = self.user.waves.as_mut()?;
                waves.force_variable_name_type(name_type);
            }
            Message::CommandPromptClear => {
                *self.command_prompt_text.borrow_mut() = String::new();
                self.command_prompt.suggestions = vec![];
                // self.command_prompt.selected = self.command_prompt.previous_commands.len();
                self.command_prompt.selected = if self.command_prompt_text.borrow().is_empty() {
                    self.command_prompt.previous_commands.len().clamp(0, 3)
                } else {
                    0
                };
            }
            Message::CommandPromptUpdate { suggestions } => {
                self.command_prompt.suggestions = suggestions;
                self.command_prompt.selected = if self.command_prompt_text.borrow().is_empty() {
                    self.command_prompt.previous_commands.len().clamp(0, 3)
                } else {
                    0
                };
                self.command_prompt.new_selection =
                    Some(if self.command_prompt_text.borrow().is_empty() {
                        self.command_prompt.previous_commands.len().clamp(0, 3)
                    } else {
                        0
                    });
            }
            Message::CommandPromptPushPrevious(cmd) => {
                let len = cmd.len();
                self.command_prompt
                    .previous_commands
                    .insert(0, (cmd, vec![false; len]));
            }
            Message::OpenFileDialog(mode) => {
                self.open_file_dialog(mode);
            }
            Message::OpenCommandFileDialog => {
                self.open_command_file_dialog();
            }
            #[cfg(feature = "python")]
            Message::OpenPythonPluginDialog => {
                self.open_python_file_dialog();
            }
            #[cfg(feature = "python")]
            Message::ReloadPythonPlugin => {
                try_log_error!(
                    self.translators.reload_python_translator(),
                    "Error reloading Python translator"
                );
                self.invalidate_draw_commands();
            }
            Message::SaveStateFile(path) => self.save_state_file(path),
            Message::LoadStateFromData(bytes) => self.load_state_from_bytes(bytes),
            Message::LoadStateFile(path) => self.load_state_file(path),
            Message::LoadState(state, path) => self.load_state(state, path),
            Message::SetStateFile(path) => {
                // since in wasm we can't support "save", only "save as" - never set the `state_file`
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.user.state_file = Some(path);
                }
                #[cfg(target_arch = "wasm32")]
                {
                    error!("Failed to load {path:?}. Loading state files is unsupported on wasm")
                }
            }
            Message::SetAboutVisible(s) => self.user.show_about = s,
            Message::SetKeyHelpVisible(s) => self.user.show_keys = s,
            Message::SetGestureHelpVisible(s) => self.user.show_gestures = s,
            Message::SetUrlEntryVisible(s, f) => {
                self.user.show_url_entry = s;
                self.url_callback = f;
            }
            Message::SetLicenseVisible(s) => self.user.show_license = s,
            Message::SetQuickStartVisible(s) => self.user.show_quick_start = s,
            Message::SetPerformanceVisible(s) => {
                if !s {
                    self.continuous_redraw = false;
                }
                self.user.show_performance = s;
            }
            Message::SetContinuousRedraw(s) => self.continuous_redraw = s,
            Message::SetMouseGestureDragStart(pos) => self.gesture_start_location = pos,
            Message::SetMeasureDragStart(pos) => self.measure_start_location = pos,
            Message::SetFilterFocused(s) => self.user.variable_name_filter_focused = s,
            Message::SetVariableNameFilterType(variable_name_filter_type) => {
                self.user.variable_filter.name_filter_type = variable_name_filter_type;
            }
            Message::SetVariableNameFilterCaseInsensitive(s) => {
                self.user.variable_filter.name_filter_case_insensitive = s;
            }
            Message::SetVariableIOFilter(t, b) => match t {
                VariableIOFilterType::Output => self.user.variable_filter.include_outputs = b,
                VariableIOFilterType::Input => self.user.variable_filter.include_inputs = b,
                VariableIOFilterType::InOut => self.user.variable_filter.include_inouts = b,
                VariableIOFilterType::Other => self.user.variable_filter.include_others = b,
            },
            Message::SetVariableGroupByDirection(b) => {
                self.user.variable_filter.group_by_direction = b;
            }
            Message::SetUIZoomFactor(scale) => {
                if let Some(ctx) = &mut self.context.as_ref() {
                    ctx.set_zoom_factor(scale);
                }
                self.user.ui_zoom_factor = Some(scale);
            }
            Message::SelectPrevCommand => {
                self.command_prompt.new_selection = Some(
                    self.command_prompt
                        .new_selection
                        .unwrap_or(self.command_prompt.selected)
                        .saturating_sub(1)
                        .max(0),
                );
            }
            Message::SelectNextCommand => {
                self.command_prompt.new_selection = Some(
                    self.command_prompt
                        .new_selection
                        .unwrap_or(self.command_prompt.selected)
                        .saturating_add(1)
                        .min(self.command_prompt.suggestions.len().saturating_sub(1)),
                );
            }
            Message::SetHierarchyStyle(style) => self.user.hierarchy_style = Some(style),
            Message::SetArrowKeyBindings(bindings) => {
                self.user.arrow_key_bindings = Some(bindings);
            }
            Message::SetPrimaryMouseDragBehavior(behavior) => {
                self.user.primary_button_drag_behavior = Some(behavior);
            }
            Message::InvalidateDrawCommands => self.invalidate_draw_commands(),
            Message::UnpauseSimulation => {
                let waves = self.user.waves.as_ref()?;
                waves.inner.as_waves().unwrap().unpause_simulation();
            }
            Message::PauseSimulation => {
                let waves = self.user.waves.as_ref()?;
                waves.inner.as_waves().unwrap().pause_simulation();
            }
            Message::Batch(messages) => {
                for message in messages {
                    self.update(message);
                }
            }
            Message::AddDraggedVariables(variables) => {
                let waves = self.user.waves.as_mut()?;

                waves.focused_item = None;
                self.user.drag_source_idx = None;
                let target = self.user.drag_target_idx.take();

                if let (Some(cmd), _) =
                    waves.add_variables(&self.translators, variables, target, true, false, None)
                {
                    self.load_variables(cmd);
                }
                self.invalidate_draw_commands();
            }
            Message::VariableDragStarted(vidx) => {
                self.user.drag_started = true;
                self.user.drag_source_idx = Some(vidx);
                self.user.drag_target_idx = None;
            }
            Message::VariableDragTargetChanged(position) => {
                self.user.drag_target_idx = Some(position);
            }
            Message::VariableDragFinished => {
                self.user.drag_started = false;

                let source_vidx = self.user.drag_source_idx.take()?;
                let target_position = self.user.drag_target_idx.take()?;

                // reordering
                self.save_current_canvas("Drag item".to_string());
                self.invalidate_draw_commands();
                let waves = self.user.waves.as_mut()?;

                let focused_index = waves
                    .focused_item
                    .and_then(|vidx| waves.items_tree.to_displayed(vidx));
                let focused_item_ref = focused_index
                    .and_then(|idx| waves.items_tree.get(idx))
                    .map(|node| node.item_ref);

                let mut to_move = waves
                    .items_tree
                    .iter_visible_extra()
                    .filter_map(|info| info.node.selected.then_some(info.idx))
                    .collect::<Vec<_>>();
                if let Some(idx) = focused_index {
                    to_move.push(idx);
                }
                if let Some(vidx) = waves.items_tree.to_displayed(source_vidx) {
                    to_move.push(vidx);
                }

                let _ = waves.items_tree.move_items(to_move, target_position);

                waves.focused_item = focused_item_ref
                    .and_then(|item_ref| {
                        waves
                            .items_tree
                            .iter_visible()
                            .position(|node| node.item_ref == item_ref)
                    })
                    .map(VisibleItemIndex);
            }
            Message::VariableValueToClipbord(vidx) => {
                self.handle_variable_clipboard_operation(
                    vidx,
                    |waves, item_ref: DisplayedItemRef| {
                        if let Some(DisplayedItem::Variable(_)) =
                            waves.displayed_items.get(&item_ref)
                        {
                            let field_ref = item_ref.into();
                            self.get_variable_value(
                                waves,
                                &field_ref,
                                waves
                                    .cursor
                                    .as_ref()
                                    .and_then(num::BigInt::to_biguint)
                                    .as_ref(),
                            )
                        } else {
                            None
                        }
                    },
                );
            }
            Message::VariableNameToClipboard(vidx) => {
                self.handle_variable_clipboard_operation(
                    vidx,
                    |waves, item_ref: DisplayedItemRef| {
                        if let Some(DisplayedItem::Variable(variable)) =
                            waves.displayed_items.get(&item_ref)
                        {
                            Some(variable.variable_ref.name.clone())
                        } else {
                            None
                        }
                    },
                );
            }
            Message::VariableFullNameToClipboard(vidx) => {
                self.handle_variable_clipboard_operation(
                    vidx,
                    |waves, item_ref: DisplayedItemRef| {
                        if let Some(DisplayedItem::Variable(variable)) =
                            waves.displayed_items.get(&item_ref)
                        {
                            Some(variable.variable_ref.full_path_string())
                        } else {
                            None
                        }
                    },
                );
            }
            Message::SetViewportStrategy(s) => {
                if let Some(waves) = &mut self.user.waves {
                    for vp in &mut waves.viewports {
                        vp.move_strategy = s;
                    }
                }
            }
            Message::Undo(count) => {
                let waves = self.user.waves.as_mut()?;
                for _ in 0..count {
                    if let Some(prev_state) = self.undo_stack.pop() {
                        self.redo_stack
                            .push(SystemState::current_canvas_state(waves, prev_state.message));
                        waves.focused_item = prev_state.focused_item;
                        waves.focused_transaction = prev_state.focused_transaction;
                        waves.items_tree = prev_state.items_tree;
                        waves.displayed_items = prev_state.displayed_items;
                        waves.markers = prev_state.markers;
                    } else {
                        break;
                    }
                }
                self.invalidate_draw_commands();
            }
            Message::Redo(count) => {
                let waves = self.user.waves.as_mut()?;
                for _ in 0..count {
                    if let Some(prev_state) = self.redo_stack.pop() {
                        self.undo_stack
                            .push(SystemState::current_canvas_state(waves, prev_state.message));
                        waves.focused_item = prev_state.focused_item;
                        waves.focused_transaction = prev_state.focused_transaction;
                        waves.items_tree = prev_state.items_tree;
                        waves.displayed_items = prev_state.displayed_items;
                        waves.markers = prev_state.markers;
                    } else {
                        break;
                    }
                }
                self.invalidate_draw_commands();
            }
            Message::DumpTree => {
                let waves = self.user.waves.as_ref()?;
                dump_tree(waves);
            }
            Message::GroupNew {
                name,
                before,
                items,
            } => {
                self.save_current_canvas(format!(
                    "Create group {}",
                    name.clone().unwrap_or(String::new())
                ));
                self.invalidate_draw_commands();
                let waves = self.user.waves.as_mut()?;

                let passed_or_focused = before
                    .and_then(|before| {
                        waves
                            .items_tree
                            .get(before)
                            .map(|node| node.level)
                            .map(|level| TargetPosition { before, level })
                    })
                    .or_else(|| waves.insert_position(waves.focused_item));
                let final_target = passed_or_focused.unwrap_or_else(|| waves.end_insert_position());

                let mut item_refs = items.unwrap_or_else(|| {
                    waves
                        .items_tree
                        .iter_visible_selected()
                        .map(|node| node.item_ref)
                        .collect::<Vec<_>>()
                });

                // if we are using the focus as the insert anchor, then move that as well
                let item_refs = if before.is_none() & passed_or_focused.is_some() {
                    let focus_index = waves
                        .items_tree
                        .to_displayed(waves.focused_item.expect("Inconsistent state"))
                        .expect("Inconsistent state");
                    item_refs.push(
                        waves
                            .items_tree
                            .get(focus_index)
                            .expect("Inconsistent state")
                            .item_ref,
                    );
                    item_refs
                } else {
                    item_refs
                };

                if item_refs.is_empty() {
                    return None;
                }

                let group_ref =
                    waves.add_group(name.unwrap_or("Group".to_owned()), Some(final_target));

                let item_idxs = waves
                    .items_tree
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, node)| {
                        item_refs
                            .contains(&node.item_ref)
                            .then_some(crate::displayed_item_tree::ItemIndex(idx))
                    })
                    .collect::<Vec<_>>();

                if let Err(e) = waves.items_tree.move_items(
                    item_idxs,
                    crate::displayed_item_tree::TargetPosition {
                        before: ItemIndex(final_target.before.0 + 1),
                        level: final_target.level.saturating_add(1),
                    },
                ) {
                    dump_tree(waves);
                    error!("failed to move items into group: {e:?}");
                }
                waves.items_tree.xselect_all_visible(false);
                waves.focused_item = waves
                    .items_tree
                    .iter_visible_extra()
                    .find_map(|info| (info.node.item_ref == group_ref).then_some(info.vidx));
            }
            Message::GroupDissolve(item_ref) => {
                self.save_current_canvas("Dissolve group".to_owned());
                self.invalidate_draw_commands();
                let waves = self.user.waves.as_mut()?;
                let item_index = waves.index_for_ref_or_focus(item_ref)?;

                waves.items_tree.remove_dissolve(item_index);
            }
            Message::GroupFold(item_ref)
            | Message::GroupUnfold(item_ref)
            | Message::GroupFoldRecursive(item_ref)
            | Message::GroupUnfoldRecursive(item_ref) => {
                let unfold = matches!(
                    message,
                    Message::GroupUnfold(..) | Message::GroupUnfoldRecursive(..)
                );
                let recursive = matches!(
                    message,
                    Message::GroupFoldRecursive(..) | Message::GroupUnfoldRecursive(..)
                );

                let undo_msg = if unfold {
                    "Unfold group".to_owned()
                } else {
                    "Fold group".to_owned()
                } + &(if recursive {
                    " recursive".to_owned()
                } else {
                    String::new()
                });
                // TODO add group name? would have to break the pattern that we insert an
                // undo message even if no waves are available
                self.save_current_canvas(undo_msg);
                self.invalidate_draw_commands();

                let waves = self.user.waves.as_mut()?;
                let item = waves.index_for_ref_or_focus(item_ref)?;

                if let Some(focused_item) = waves.focused_item {
                    let info = waves
                        .items_tree
                        .get_visible_extra(focused_item)
                        .expect("Inconsistent state");
                    if waves.items_tree.subtree_contains(item, info.idx) {
                        waves.focused_item = None;
                    }
                }
                if recursive {
                    waves.items_tree.xfold_recursive(item, unfold);
                } else {
                    waves.items_tree.xfold(item, unfold);
                }
            }
            Message::GroupFoldAll | Message::GroupUnfoldAll => {
                let unfold = matches!(message, Message::GroupUnfoldAll);
                let undo_msg = if unfold {
                    "Fold all groups".to_owned()
                } else {
                    "Unfold all groups".to_owned()
                };
                self.save_current_canvas(undo_msg);
                self.invalidate_draw_commands();

                let waves = self.user.waves.as_mut()?;

                // remove focus if focused item is folded away -> prevent future waveform
                // adds being invisibly inserted
                if let Some(focused_item) = waves.focused_item {
                    let focused_level = waves
                        .items_tree
                        .get_visible(focused_item)
                        .expect("Inconsistent state")
                        .level;
                    if !unfold & (focused_level > 0) {
                        waves.focused_item = None;
                    }
                }
                waves.items_tree.xfold_all(unfold);
            }
            #[cfg(target_arch = "wasm32")]
            Message::StartWcpServer { .. } => {
                error!("Wcp is not supported on wasm")
            }
            #[cfg(target_arch = "wasm32")]
            Message::StopWcpServer => {
                error!("Wcp is not supported on wasm")
            }
            #[cfg(not(target_arch = "wasm32"))]
            Message::StartWcpServer { address, initiate } => {
                self.start_wcp_server(address, initiate);
            }
            #[cfg(not(target_arch = "wasm32"))]
            Message::StopWcpServer => {
                self.stop_wcp_server();
            }
            Message::SetupChannelWCP => {
                #[cfg(target_arch = "wasm32")]
                {
                    use futures::executor::block_on;
                    self.channels.wcp_c2s_receiver = block_on(WCP_CS_HANDLER.rx.write()).take();
                    if self.channels.wcp_c2s_receiver.is_none() {
                        error!("Failed to claim wasm tx, was SetupWasmWCP executed twice?");
                    }
                    self.channels.wcp_s2c_sender = Some(WCP_SC_HANDLER.tx.clone());
                }
            }
            Message::BuildAnalogCache {
                display_id,
                cache_key,
            } => {
                let waves = self.user.waves.as_mut()?;
                let generation = waves.cache_generation;

                // Check if already have valid entry (building or ready)
                let item = waves.displayed_items.get(&display_id)?;
                let DisplayedItem::Variable(var) = item else {
                    return None;
                };
                if var
                    .analog
                    .as_ref()?
                    .cache
                    .as_ref()
                    .is_some_and(|e| e.generation == generation && e.cache_key == cache_key)
                {
                    return None;
                }

                // Try to share from in-flight builds first (handles removed-but-still-building case)
                if let Some(entry) = waves.inflight_caches.get(&cache_key)
                    && entry.generation == generation
                {
                    if let DisplayedItem::Variable(var) =
                        waves.displayed_items.get_mut(&display_id)?
                    {
                        var.analog.as_mut()?.cache = Some(entry.clone());
                    }
                    return None; // Shared from in-flight build
                }

                // Try to share from another displayed variable (O(n) scan - only during cache build)
                let existing = waves
                    .displayed_items
                    .values()
                    .filter_map(|item| match item {
                        DisplayedItem::Variable(v) => v.analog.as_ref()?.cache.as_ref(),
                        _ => None,
                    })
                    .find(|e| e.cache_key == cache_key && e.generation == generation)
                    .cloned();

                if let Some(entry) = existing {
                    if let DisplayedItem::Variable(var) =
                        waves.displayed_items.get_mut(&display_id)?
                    {
                        var.analog.as_mut()?.cache = Some(entry);
                    }
                    return None; // Shared existing entry (may still be building)
                }

                // Clone variable_ref only when we need to spawn builder
                let variable_ref = match waves.displayed_items.get(&display_id)? {
                    DisplayedItem::Variable(v) => v.variable_ref.clone(),
                    _ => return None,
                };

                // Create new entry and spawn builder
                let entry = std::sync::Arc::new(crate::analog_signal_cache::AnalogCacheEntry::new(
                    cache_key.clone(),
                    generation,
                ));

                if let DisplayedItem::Variable(var) = waves.displayed_items.get_mut(&display_id)? {
                    var.analog.as_mut()?.cache = Some(entry.clone());
                }

                let translator = self.translators.clone_translator(&cache_key.1);

                // Track in-flight build for sharing with other variables
                waves
                    .inflight_caches
                    .insert(cache_key.clone(), entry.clone());

                waves.build_analog_cache_async(
                    entry,
                    &variable_ref,
                    translator,
                    &self.channels.msg_sender,
                );
            }
            Message::AnalogCacheBuilt { entry, result } => {
                OUTSTANDING_TRANSACTIONS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                // Remove from in-flight registry (may already be gone if generation changed)
                if let Some(waves) = self.user.waves.as_mut() {
                    waves.inflight_caches.remove(&entry.cache_key);
                }
                match result {
                    Ok(cache) => {
                        entry.set(cache);
                    }
                    Err(err) => {
                        warn!("Failed to build analog cache: {err}");
                    }
                }
                self.invalidate_draw_commands();
            }
            Message::Exit | Message::ToggleFullscreen => {} // Handled in eframe::update
            Message::AddViewport => {
                let waves = self.user.waves.as_mut()?;
                let viewport = Viewport::new();
                waves.viewports.push(viewport);
                self.draw_data.borrow_mut().push(None);
            }
            Message::RemoveViewport => {
                let waves = self.user.waves.as_mut()?;
                if waves.viewports.len() > 1 {
                    waves.viewports.pop();
                    self.draw_data.borrow_mut().pop();
                }
            }
            Message::AddTile => {
                self.user.tile_tree.add_debug_tile();
                self.invalidate_draw_commands();
            }
            Message::SelectTheme(theme_name) => {
                let theme = SurferTheme::new(theme_name)
                    .with_context(|| "Failed to set theme")
                    .ok()?;
                self.user.config.theme = theme;
                let ctx = self.context.as_ref()?;
                ctx.set_visuals(self.get_visuals());
            }
            Message::EnableAnimations(enable) => {
                let ctx = self.context.as_ref()?;
                self.user.animation_enabled = Some(enable);
                ctx.all_styles_mut(|style| {
                    style.animation_time = if self.animation_enabled() {
                        self.user.config.animation_time
                    } else {
                        0.0
                    };
                });
            }
            Message::AsyncDone(_) => (),
            Message::AddGraphic(id, g) => {
                let waves = self.user.waves.as_mut()?;
                waves.graphics.insert(id, g);
            }
            Message::RemoveGraphic(id) => {
                let waves = self.user.waves.as_mut()?;
                waves.graphics.retain(|k, _| k != &id);
            }
            Message::ExpandDrawnItem { item, levels } => {
                self.items_to_expand.borrow_mut().push((item, levels));
            }
            Message::AddCharToPrompt(c) => *self.char_to_add_to_prompt.borrow_mut() = Some(c),
        }
        Some(())
    }

    pub fn add_scope_as_group(
        &mut self,
        scope: &ScopeRef,
        pos: TargetPosition,
        recursive: bool,
        variable_name_type: Option<VariableNameType>,
    ) -> TargetPosition {
        let Some(waves) = self.user.waves.as_mut() else {
            return pos;
        };
        let Some(container) = waves.inner.as_waves() else {
            return pos;
        };

        let variables = container
            .variables_in_scope(scope)
            .iter()
            .sorted_by(|a, b| numeric_sort::cmp(&a.name, &b.name))
            .cloned()
            .collect_vec();
        let child_scopes = container.child_scopes(scope);
        let variable_name_type = variable_name_type.or_else(|| {
            container
                .scope_is_variable(scope)
                .then_some(VariableNameType::Local)
        });

        waves.add_group(scope.name(), Some(pos));
        let into_group_pos = TargetPosition {
            before: ItemIndex(pos.before.0 + 1),
            level: pos.level + 1,
        };

        let (cmd, variable_refs) = waves.add_variables(
            &self.translators,
            variables,
            Some(into_group_pos),
            false,
            false,
            variable_name_type,
        );
        let mut into_group_pos = TargetPosition {
            before: ItemIndex(into_group_pos.before.0 + variable_refs.len()),
            level: pos.level + 1,
        };

        if let Some(cmd) = cmd {
            self.load_variables(cmd);
        }

        if recursive {
            for child in child_scopes.unwrap_or(vec![]) {
                into_group_pos =
                    self.add_scope_as_group(&child, into_group_pos, recursive, variable_name_type);
                into_group_pos.level = pos.level + 1;
            }
        }
        into_group_pos
    }

    fn handle_variable_clipboard_operation<F>(
        &self,
        vidx: MessageTarget<VisibleItemIndex>,
        get_text: F,
    ) where
        F: FnOnce(&WaveData, DisplayedItemRef) -> Option<String>,
    {
        let Some(waves) = &self.user.waves else {
            return;
        };
        let vidx = if let MessageTarget::Explicit(vidx) = vidx {
            vidx
        } else if let Some(focused) = waves.focused_item {
            focused
        } else {
            return;
        };
        let Some(item_ref) = waves.items_tree.get_visible(vidx).map(|node| node.item_ref) else {
            return;
        };

        if let Some(text) = get_text(waves, item_ref)
            && let Some(ctx) = &self.context
        {
            ctx.copy_text(text);
        }
    }
}

pub fn dump_tree(waves: &WaveData) {
    let mut result = String::new();
    for (idx, node) in waves.items_tree.iter().enumerate() {
        for _ in 0..node.level.saturating_sub(1) {
            result.push(' ');
        }

        if node.level > 0 {
            match waves.items_tree.get(ItemIndex(idx + 1)) {
                Some(next) if next.level < node.level => result.push_str("╰╴"),
                _ => result.push_str("├╴"),
            }
        }

        result.push_str(
            &waves
                .displayed_items
                .get(&node.item_ref)
                .map_or("?".to_owned(), displayed_item::DisplayedItem::name),
        );
        result.push_str(&format!("   ({:?})", node.item_ref));
        if node.selected {
            result.push_str(" !SEL! ");
        }
        result.push('\n');
    }
    info!("tree: \n{}", &result);
}

pub struct StateWrapper(Arc<RwLock<SystemState>>);
impl App for StateWrapper {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        App::update(&mut *self.0.write().unwrap(), ctx, frame);
    }
}

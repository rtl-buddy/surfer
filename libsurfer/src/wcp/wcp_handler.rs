use crate::{
    SystemState, WcpClientCapabilities,
    displayed_item::{DisplayedItem, DisplayedItemRef},
    message::{Message, MessageTarget},
    time::TimeUnit,
    wave_container::{ScopeRefExt, VariableRef, VariableRefExt},
    wave_data::WaveData,
    wave_source::{LoadOptions, WaveSource, string_to_wavesource},
};

use futures::executor::block_on;
use itertools::Itertools;
use num::BigInt;
use std::sync::atomic::Ordering;
use surfer_translation_types::ScopeRef;
use tracing::{trace, warn};

use surfer_wcp::{
    ItemInfo, MarkerInfo, WcpCSMessage, WcpCommand, WcpResponse, WcpSCMessage, WcpTimeUnit,
};

impl SystemState {
    pub fn handle_wcp_commands(&mut self) {
        let Some(receiver) = &mut self.channels.wcp_c2s_receiver else {
            return;
        };

        let mut messages = vec![];
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    messages.push(command);
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    trace!("WCP Command sender disconnected");
                    break;
                }
            }
        }
        for message in messages {
            self.handle_wcp_cs_message(&message);
        }
    }

    fn handle_wcp_cs_message(&mut self, message: &WcpCSMessage) {
        if !self.wcp_greeted_signal.load(Ordering::Relaxed) {
            if let WcpCSMessage::greeting { .. } = message {
            } else {
                self.send_error("WCP server has not received greeting messages", vec![], "");
                return;
            }
        }
        match message {
            WcpCSMessage::command(command) => {
                match command {
                    WcpCommand::get_item_list => {
                        if let Some(waves) = &self.user.waves {
                            let ids: Vec<surfer_wcp::DisplayedItemRef> = self
                                .get_displayed_items(waves)
                                .iter()
                                .map(std::convert::Into::into)
                                .collect_vec();
                            self.send_response(WcpResponse::get_item_list { ids });
                        } else {
                            self.send_error("No waveform loaded", vec![], "No waveform loaded");
                        }
                    }
                    WcpCommand::get_item_info { ids } => {
                        let Some(waves) = &self.user.waves else {
                            self.send_error("remove_items", vec![], "No waveform loaded");
                            return;
                        };
                        let mut items: Vec<ItemInfo> = Vec::new();
                        for id in ids {
                            if let Some(item) = waves.displayed_items.get(&id.into()) {
                                let (name, item_type) = match item {
                                    DisplayedItem::Variable(var) => (
                                        var.manual_name.clone().unwrap_or(var.display_name.clone()),
                                        "Variable".to_string(),
                                    ),
                                    DisplayedItem::Divider(item) => (
                                        item.name.clone().unwrap_or("Name not found!".to_string()),
                                        "Divider".to_string(),
                                    ),
                                    DisplayedItem::Marker(item) => (
                                        item.name.clone().unwrap_or("Name not found!".to_string()),
                                        "Marker".to_string(),
                                    ),
                                    DisplayedItem::TimeLine(item) => (
                                        item.name.clone().unwrap_or("Name not found!".to_string()),
                                        "TimeLine".to_string(),
                                    ),
                                    DisplayedItem::Placeholder(item) => (
                                        item.manual_name
                                            .clone()
                                            .unwrap_or("Name not found!".to_string()),
                                        "Placeholder".to_string(),
                                    ),
                                    DisplayedItem::Stream(item) => (
                                        item.manual_name
                                            .clone()
                                            .unwrap_or(item.display_name.clone()),
                                        "Stream".to_string(),
                                    ),
                                    DisplayedItem::Group(item) => {
                                        (item.name.clone(), "Group".to_string())
                                    }
                                };
                                items.push(ItemInfo {
                                    name,
                                    t: item_type,
                                    id: *id,
                                });
                            } else {
                                self.send_error(
                                    "get_item_info",
                                    vec![],
                                    &format!("No item with id {id:?}"),
                                );
                                return;
                            }
                        }
                        self.send_response(WcpResponse::get_item_info { results: items });
                    }
                    WcpCommand::add_variables { variables } => {
                        if self.user.waves.is_some() {
                            self.save_current_canvas(format!("Add {} variables", variables.len()));
                        }
                        if let Some(waves) = self.user.waves.as_mut() {
                            // Resolve every requested path up front so we can
                            // report which inputs didn't match a variable in the
                            // currently-loaded waveform. add_variables itself
                            // silently drops unresolved refs, which makes the
                            // empty-ids response indistinguishable from "all
                            // paths bogus" for any external driver (WCP CLI,
                            // rtl-buddy hub bridge, etc.).
                            let wave_cont = waves.inner.as_waves().unwrap();
                            let mut not_found: Vec<String> = Vec::new();
                            let mut variable_refs: Vec<VariableRef> = Vec::new();
                            for name in variables {
                                let vref = VariableRef::from_hierarchy_string(name);
                                if wave_cont.variable_meta(&vref).is_ok() {
                                    variable_refs.push(vref);
                                } else {
                                    not_found.push(name.clone());
                                }
                            }
                            let (cmd, ids) = waves.add_variables(
                                &self.translators,
                                variable_refs,
                                None,
                                true,
                                false,
                                None,
                            );
                            if let Some(cmd) = cmd {
                                self.load_variables(cmd);
                            }
                            self.send_response(WcpResponse::add_variables {
                                ids: ids.into_iter().map(std::convert::Into::into).collect_vec(),
                                not_found,
                            });
                            self.invalidate_draw_commands();
                        } else {
                            self.send_error(
                                "add_variables",
                                vec![],
                                "Can't add signals. No waveform loaded",
                            );
                        }
                    }
                    WcpCommand::add_scope { scope, recursive } => {
                        if self.user.waves.is_some() {
                            self.save_current_canvas(format!("Add scope {scope}"));
                        }
                        let scope_str = scope.clone();
                        let scope_ref = ScopeRef::from_hierarchy_string(scope);
                        // A scope that doesn't exist at all is a clear user
                        // error; an existing-but-empty scope returns
                        // not_found=[] (caller asked for nothing and got
                        // nothing, which is fine).
                        let mut not_found: Vec<String> = Vec::new();
                        if let Some(waves) = self.user.waves.as_ref() {
                            if !waves.inner.as_waves().unwrap().scope_exists(&scope_ref) {
                                not_found.push(scope_str);
                            }
                        }
                        let variables = self.get_scope(scope_ref, *recursive);
                        if let Some(waves) = self.user.waves.as_mut() {
                            let (cmd, ids) = waves.add_variables(
                                &self.translators,
                                variables,
                                None,
                                true,
                                false,
                                None,
                            );
                            if let Some(cmd) = cmd {
                                self.load_variables(cmd);
                            }
                            self.send_response(WcpResponse::add_scope {
                                ids: ids.into_iter().map(std::convert::Into::into).collect_vec(),
                                not_found,
                            });
                            self.invalidate_draw_commands();
                        } else {
                            self.send_error("scope_add", vec![], "No waveform loaded");
                        }
                    }
                    WcpCommand::add_items { items, recursive } => {
                        if self.user.waves.is_some() {
                            self.save_current_canvas(format!("Add {} items", items.len()));
                        }

                        // Each input item is treated as either a variable OR a
                        // scope (the existing handler tries both). A path that
                        // matches neither is reported in not_found so callers
                        // can distinguish "no matches" from "got something".
                        let mut not_found: Vec<String> = Vec::new();
                        let mut variables: Vec<VariableRef> = Vec::new();
                        for item in items {
                            let variable_ref = VariableRef::from_hierarchy_string(item);
                            let scope = ScopeRef::from_hierarchy_string(item);
                            let scope_variables = self.get_scope(scope.clone(), *recursive);
                            let var_ok = self
                                .user
                                .waves
                                .as_ref()
                                .and_then(|w| {
                                    w.inner.as_waves().unwrap().variable_meta(&variable_ref).ok()
                                })
                                .is_some();
                            let scope_ok = self
                                .user
                                .waves
                                .as_ref()
                                .map(|w| w.inner.as_waves().unwrap().scope_exists(&scope))
                                .unwrap_or(false);
                            if !var_ok && !scope_ok {
                                not_found.push(item.clone());
                            }
                            variables.push(variable_ref);
                            variables.extend(scope_variables);
                        }

                        if let Some(waves) = self.user.waves.as_mut() {
                            let (cmd, ids) = waves.add_variables(
                                &self.translators,
                                variables,
                                None,
                                true,
                                true,
                                None,
                            );
                            if let Some(cmd) = cmd {
                                self.load_variables(cmd);
                            }
                            self.send_response(WcpResponse::add_items {
                                ids: ids.into_iter().map(std::convert::Into::into).collect_vec(),
                                not_found,
                            });
                            self.invalidate_draw_commands();
                        } else {
                            self.send_error(
                                "add_items",
                                vec![],
                                "Can't add items. No waveform loaded",
                            );
                        }
                    }
                    WcpCommand::add_markers { markers } => {
                        if self.user.waves.is_some() {
                            self.save_current_canvas(format!("Add {} markers", markers.len()));
                        }
                        if let Some(waves) = self.user.waves.as_mut() {
                            let mut ids = vec![];
                            for marker in markers {
                                let MarkerInfo {
                                    time,
                                    name,
                                    move_focus,
                                } = marker;
                                if let Some(id) = waves.add_marker(time, name.clone(), *move_focus)
                                {
                                    ids.push(id.into());
                                } else {
                                    self.send_error("add_markers", vec![], "Cannot add marker");
                                    return;
                                }
                            }
                            self.send_response(WcpResponse::add_markers { ids });
                        } else {
                            self.send_error("add_markers", vec![], "No waveform loaded");
                        }
                    }
                    WcpCommand::reload => {
                        self.update(Message::ReloadWaveform(false));
                        self.send_response(WcpResponse::ack);
                    }
                    WcpCommand::set_viewport_to {
                        timestamp,
                        time_unit,
                    } => {
                        let native = match self.wcp_to_native_ticks(timestamp, *time_unit) {
                            Ok(v) => v,
                            Err(msg) => {
                                self.send_error("set_viewport_to", vec![], &msg);
                                return;
                            }
                        };
                        self.update(Message::GoToTime(Some(native), 0));
                        self.send_response(WcpResponse::ack);
                    }
                    WcpCommand::set_viewport_range {
                        start,
                        end,
                        time_unit,
                    } => {
                        let native_start = match self.wcp_to_native_ticks(start, *time_unit) {
                            Ok(v) => v,
                            Err(msg) => {
                                self.send_error("set_viewport_range", vec![], &msg);
                                return;
                            }
                        };
                        let native_end = match self.wcp_to_native_ticks(end, *time_unit) {
                            Ok(v) => v,
                            Err(msg) => {
                                self.send_error("set_viewport_range", vec![], &msg);
                                return;
                            }
                        };
                        self.update(Message::ZoomToRange {
                            start: native_start,
                            end: native_end,
                            viewport_idx: 0,
                        });
                        self.send_response(WcpResponse::ack);
                    }
                    WcpCommand::set_item_color { id, color } => {
                        let Some(waves) = &self.user.waves else {
                            self.send_error("set_item_color", vec![], "No waveform loaded");
                            return;
                        };

                        if let Some(idx) = waves.get_displayed_item_index(&id.into()) {
                            self.update(Message::ItemColorChange(
                                MessageTarget::Explicit(idx),
                                Some(color.clone()),
                            ));
                            self.send_response(WcpResponse::ack);
                        } else {
                            self.send_error(
                                "set_item_color",
                                vec![],
                                format!("Item {id:?} not found").as_str(),
                            );
                        }
                    }
                    WcpCommand::remove_items { ids } => {
                        let Some(_) = self.user.waves.as_mut() else {
                            self.send_error("remove_items", vec![], "No waveform loaded");
                            return;
                        };
                        let msgs = vec![Message::RemoveItems(
                            ids.iter().map(std::convert::Into::into).collect(),
                        )];
                        self.update(Message::Batch(msgs));

                        self.send_response(WcpResponse::ack);
                    }
                    WcpCommand::focus_item { id } => {
                        let Some(waves) = &self.user.waves else {
                            self.send_error("remove_items", vec![], "No waveform loaded");
                            return;
                        };
                        // TODO: Create a `.into` function here instead of unwrapping and wrapping
                        // it to prevent future type errors
                        if let Some(vidx) = waves.get_displayed_item_index(&id.into()) {
                            self.update(Message::FocusItem(vidx));

                            self.send_response(WcpResponse::ack);
                        } else {
                            self.send_error(
                                "focus_item",
                                vec![],
                                format!("No item with ID {id:?}").as_str(),
                            );
                        }
                    }
                    WcpCommand::clear => {
                        if let Some(wave) = &self.user.waves {
                            self.update(Message::RemoveItems(self.get_displayed_items(wave)));
                        }

                        self.send_response(WcpResponse::ack);
                    }
                    WcpCommand::load { source } => {
                        match string_to_wavesource(source) {
                            WaveSource::Url(url) => {
                                self.update(Message::LoadWaveformFileFromUrl(
                                    url,
                                    LoadOptions::Clear,
                                ));
                                self.send_response(WcpResponse::ack);
                            }
                            WaveSource::File(file) => {
                                // FIXME add support for loading transaction files via Message::LoadTransactionFile
                                let msg = Message::LoadFile(file, LoadOptions::Clear);
                                self.update(msg);
                                self.send_response(WcpResponse::ack);
                            }
                            _ => {
                                self.send_error(
                                    "load",
                                    vec![],
                                    format!("{source} is not legal wave source").as_str(),
                                );
                            }
                        }
                    }
                    WcpCommand::zoom_to_fit { viewport_idx } => {
                        self.update(Message::ZoomToFit {
                            viewport_idx: *viewport_idx,
                        });
                        self.send_response(WcpResponse::ack);
                    }
                    WcpCommand::set_cursor {
                        timestamp,
                        time_unit,
                    } => {
                        let native = match self.wcp_to_native_ticks(timestamp, *time_unit) {
                            Ok(v) => v,
                            Err(msg) => {
                                self.send_error("set_cursor", vec![], &msg);
                                return;
                            }
                        };
                        self.update(Message::CursorSet(native));
                        self.send_response(WcpResponse::ack);
                    }
                    WcpCommand::shutdown => {
                        warn!("WCP Shutdown message should not reach this place");
                    }
                }
            }
            WcpCSMessage::greeting { version, commands } => {
                if version == "0" {
                    self.wcp_client_capabilities = WcpClientCapabilities::new();
                    if commands.iter().any(|s| s == "waveforms_loaded") {
                        self.wcp_client_capabilities.waveforms_loaded = true;
                    }
                    if commands.iter().any(|s| s == "goto_declaration") {
                        self.wcp_client_capabilities.goto_declaration = true;
                    }
                    if commands.iter().any(|s| s == "add_drivers") {
                        self.wcp_client_capabilities.add_drivers = true;
                    }
                    if commands.iter().any(|s| s == "add_loads") {
                        self.wcp_client_capabilities.add_loads = true;
                    }
                    self.wcp_greeted_signal.store(true, Ordering::Relaxed);
                    self.wcp_greeted_signal.store(true, Ordering::Relaxed);
                    self.send_greeting();
                } else {
                    self.send_error(
                        "greeting",
                        vec![],
                        &format!("Surfer only supports WCP version 0, client requested {version}"),
                    );
                }
            }
        }
    }

    fn send_greeting(&self) {
        let commands = vec![
            "add_variables",
            "set_viewport_to",
            "cursor_set",
            "reload",
            "add_scope",
            "add_items",
            "get_item_list",
            "set_item_color",
            "get_item_info",
            "clear_item",
            "focus_item",
            "clear",
            "load",
            "zoom_to_fit",
            "add_markers",
            "set_viewport_range_to",
        ]
        .into_iter()
        .map(str::to_string)
        .collect_vec();

        let greeting = WcpSCMessage::create_greeting(0, commands);

        self.channels
            .wcp_s2c_sender
            .as_ref()
            .map(|ch| block_on(ch.send(greeting)));
    }

    fn send_response(&self, result: WcpResponse) {
        self.channels
            .wcp_s2c_sender
            .as_ref()
            .map(|ch| block_on(ch.send(WcpSCMessage::response(result))));
    }

    fn send_error(&self, error: &str, arguments: Vec<String>, message: &str) {
        self.channels.wcp_s2c_sender.as_ref().map(|ch| {
            block_on(ch.send(WcpSCMessage::create_error(
                error.to_string(),
                arguments,
                message.to_string(),
            )))
        });
    }

    fn get_displayed_items(&self, waves: &WaveData) -> Vec<DisplayedItemRef> {
        // TODO check call sites since visible items may now differ from loaded items
        waves
            .items_tree
            .iter_visible()
            .map(|node| node.item_ref)
            .collect_vec()
    }

    /// Convert a WCP timestamp into the loaded waveform's native tick units.
    ///
    /// When `unit` is `None`, the value is already in native ticks (the
    /// historical pre-`time_unit` behaviour) and is returned unchanged. When
    /// `unit` is set, surfer reads the loaded waveform's timescale and
    /// rescales the value with integer arithmetic — division truncates, so
    /// callers asking for sub-tick precision get round-toward-zero.
    ///
    /// Errors when no waveform is loaded or when the loaded waveform's
    /// timescale is `Unknown` / `Auto` (a unit-bearing request can't be
    /// honored without a known native unit on this end).
    fn wcp_to_native_ticks(
        &self,
        value: &BigInt,
        unit: Option<WcpTimeUnit>,
    ) -> Result<BigInt, String> {
        let Some(unit) = unit else {
            return Ok(value.clone());
        };
        let Some(waves) = self.user.waves.as_ref() else {
            return Err("no waveform loaded; can't honor time_unit".to_string());
        };
        let timescale = waves.inner.metadata().timescale;
        // Match libsurfer's TimeUnit to a power-of-ten exponent. None / Auto
        // mean the loaded format didn't declare a unit (e.g. unit-less VCD)
        // so we can't honor a unit-bearing WCP request.
        let ts_exp: i32 = match timescale.unit {
            TimeUnit::ZeptoSeconds => -21,
            TimeUnit::AttoSeconds => -18,
            TimeUnit::FemtoSeconds => -15,
            TimeUnit::PicoSeconds => -12,
            TimeUnit::NanoSeconds => -9,
            TimeUnit::MicroSeconds => -6,
            TimeUnit::MilliSeconds => -3,
            TimeUnit::Seconds => 0,
            TimeUnit::None | TimeUnit::Auto => {
                return Err(
                    "loaded waveform has no declared timescale; can't honor time_unit".to_string(),
                );
            }
        };
        let multiplier = BigInt::from(timescale.multiplier.unwrap_or(1).max(1));
        let from_exp = unit.exponent();
        let diff = from_exp - ts_exp;
        let ten = BigInt::from(10);
        // native = value × 10^diff / multiplier
        if diff >= 0 {
            let factor = ten.pow(diff as u32);
            Ok(value * factor / multiplier)
        } else {
            let divisor = ten.pow((-diff) as u32) * multiplier;
            Ok(value / divisor)
        }
    }
}

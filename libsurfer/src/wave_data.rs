use std::collections::HashMap;

use eyre::{Result, WrapErr};
use num::bigint::ToBigInt as _;
use num::{BigInt, BigUint, One, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use surfer_translation_types::{TranslationPreference, Translator, VariableValue};
use tracing::{error, info, warn};

use crate::data_container::DataContainer;
use crate::displayed_item::{
    DisplayedDivider, DisplayedFieldRef, DisplayedGroup, DisplayedItem, DisplayedItemRef,
    DisplayedStream, DisplayedTimeLine, DisplayedVariable,
};
use crate::displayed_item_tree::{DisplayedItemTree, ItemIndex, TargetPosition, VisibleItemIndex};
use crate::graphics::{Graphic, GraphicId};
use crate::transaction_container::{StreamScopeRef, TransactionRef, TransactionStreamRef};
use crate::transactions::calculate_rows_of_stream;
use crate::translation::{DynTranslator, TranslatorList, VariableInfoExt};
use crate::variable_name_type::VariableNameType;
use crate::view::ItemDrawingInfo;
use crate::viewport::Viewport;
use crate::wave_container::{
    AnalogCacheKey, ScopeRef, VariableMeta, VariableRef, VariableRefExt, WaveContainer,
};
use crate::wave_source::{WaveFormat, WaveSource};
use crate::wellen::LoadSignalsCmd;
use ftr_parser::types::Transaction;
use itertools::Itertools;
use std::fmt::Formatter;
use std::ops::Not;

pub const PER_SCROLL_EVENT: f32 = 50.0;
pub const SCROLL_EVENTS_PER_PAGE: f32 = 20.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScopeType {
    WaveScope(ScopeRef),
    StreamScope(StreamScopeRef),
}

impl std::fmt::Display for ScopeType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ScopeType::WaveScope(w) => w.fmt(f),
            ScopeType::StreamScope(s) => s.fmt(f),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct WaveData {
    #[serde(skip, default = "DataContainer::__new_empty")]
    pub inner: DataContainer,
    pub source: WaveSource,
    pub format: WaveFormat,
    pub active_scope: Option<ScopeType>,
    /// Root items (variables, dividers, ...) to display
    pub items_tree: DisplayedItemTree,
    pub displayed_items: HashMap<DisplayedItemRef, DisplayedItem>,
    /// Tracks the consecutive displayed item refs
    pub display_item_ref_counter: usize,
    pub viewports: Vec<Viewport>,
    pub cursor: Option<BigInt>,
    pub markers: HashMap<u8, BigInt>,
    pub focused_item: Option<VisibleItemIndex>,
    pub focused_transaction: (Option<TransactionRef>, Option<Transaction>),
    pub default_variable_name_type: VariableNameType,
    pub scroll_offset: f32,
    pub display_variable_indices: bool,
    pub graphics: HashMap<GraphicId, Graphic>,
    /// These are just stored during operation, so no need to serialize
    #[serde(skip)]
    pub drawing_infos: Vec<ItemDrawingInfo>,
    #[serde(skip)]
    pub top_item_draw_offset: f32,
    #[serde(skip)]
    pub total_height: f32,
    #[serde(skip)]
    pub old_num_timestamps: Option<BigInt>,
    /// Generation counter for analog cache invalidation on waveform reload.
    #[serde(skip)]
    pub cache_generation: u64,
    /// Registry of in-flight analog cache builds for sharing.
    /// Cleared on waveform reload when generation changes.
    #[serde(skip)]
    pub inflight_caches:
        HashMap<AnalogCacheKey, std::sync::Arc<crate::analog_signal_cache::AnalogCacheEntry>>,
}

fn select_preferred_translator(var: &VariableMeta, translators: &TranslatorList) -> String {
    let mut preferred: Vec<_> = translators
        .all_translators()
        .iter()
        .filter_map(|t| match t.translates(var) {
            Ok(TranslationPreference::Prefer) => Some(t.name()),
            Ok(TranslationPreference::Yes) => None,
            Ok(TranslationPreference::No) => None,
            Err(e) => {
                error!(
                    "Failed to check if {} translates {}\n{e:#?}",
                    t.name(),
                    var.var.full_path_string_no_index()
                );
                None
            }
        })
        .collect();
    if preferred.len() > 1 {
        // For a single bit that has other preferred translators in addition to "Bit", like enum,
        // we would like to select the other one.
        let bit = "Bit".to_string();
        if var.num_bits == Some(1) {
            preferred.retain(|x| x != &bit);
        }
        if preferred.len() > 1 {
            warn!(
                "More than one preferred translator for variable {} in scope {}: {}",
                var.var.name,
                var.var.path.strs.join("."),
                preferred.join(", ")
            );
            preferred.sort();
        }
    }
    // make sure we always pick the same translator, at least
    preferred
        .pop()
        .unwrap_or_else(|| translators.default.clone())
}

pub fn variable_translator<'a, F>(
    translator: Option<&String>,
    field: &[String],
    translators: &'a TranslatorList,
    meta: F,
) -> &'a DynTranslator
where
    F: FnOnce() -> Result<VariableMeta>,
{
    let translator_name = translator.cloned().unwrap_or_else(|| {
        if field.is_empty() {
            meta().as_ref().map_or_else(
                |e| {
                    warn!("{e:#?}");
                    translators.default.clone()
                },
                |meta| select_preferred_translator(meta, translators).clone(),
            )
        } else {
            translators.default.clone()
        }
    });

    (translators.get_translator(&translator_name)) as _
}

impl WaveData {
    #[must_use]
    pub fn update_with_waves(
        mut self,
        new_waves: Box<WaveContainer>,
        source: WaveSource,
        format: WaveFormat,
        translators: &TranslatorList,
        keep_unavailable: bool,
    ) -> (WaveData, Option<LoadSignalsCmd>) {
        let active_scope = self.active_scope.take().filter(|m| {
            if let ScopeType::WaveScope(w) = m {
                new_waves.scope_exists(w)
            } else {
                false
            }
        });
        let display_items = Self::update_displayed_items(
            &new_waves,
            &self.displayed_items,
            keep_unavailable,
            translators,
            &mut self.items_tree,
        );

        let old_num_timestamps = self.num_timestamps();
        let mut new_wavedata = WaveData {
            inner: DataContainer::Waves(*new_waves),
            source,
            format,
            active_scope,
            items_tree: self.items_tree,
            displayed_items: display_items,
            display_item_ref_counter: self.display_item_ref_counter,
            viewports: self.viewports,
            cursor: self.cursor.clone(),
            markers: self.markers.clone(),
            focused_item: self.focused_item,
            focused_transaction: self.focused_transaction,
            default_variable_name_type: self.default_variable_name_type,
            display_variable_indices: self.display_variable_indices,
            scroll_offset: self.scroll_offset,
            drawing_infos: vec![],
            top_item_draw_offset: 0.,
            graphics: HashMap::new(),
            total_height: 0.,
            old_num_timestamps,
            cache_generation: self.cache_generation + 1, // Invalidate all existing caches
            inflight_caches: HashMap::new(),
        };

        new_wavedata.update_metadata(translators);
        let load_commands = new_wavedata.load_waves();
        (new_wavedata, load_commands)
    }

    pub fn update_with_items(
        &mut self,
        new_items: &HashMap<DisplayedItemRef, DisplayedItem>,
        mut items_tree: DisplayedItemTree,
        translators: &TranslatorList,
    ) -> Option<LoadSignalsCmd> {
        self.displayed_items = Self::update_displayed_items(
            self.inner.as_waves().unwrap(),
            new_items,
            true,
            translators,
            &mut items_tree,
        );
        self.items_tree = items_tree;

        self.display_item_ref_counter = self
            .displayed_items
            .keys()
            .map(|dir| dir.0)
            .max()
            .unwrap_or(0);

        self.update_metadata(translators);
        self.load_waves()
    }

    /// Go through all signals and update the metadata for all signals
    ///
    /// Used after loading new waves, signals or switching a bunch of translators
    fn update_metadata(&mut self, translators: &TranslatorList) {
        for di in self.displayed_items.values_mut() {
            let DisplayedItem::Variable(displayed_variable) = di else {
                continue;
            };

            let meta = self
                .inner
                .as_waves()
                .unwrap()
                .variable_meta(&displayed_variable.variable_ref.clone())
                .unwrap();
            let translator =
                variable_translator(displayed_variable.get_format(&[]), &[], translators, || {
                    Ok(meta.clone())
                });
            let info = translator.variable_info(&meta).ok();

            match info {
                Some(info) => displayed_variable
                    .field_formats
                    .retain(|ff| info.has_subpath(&ff.field)),
                _ => displayed_variable.field_formats.clear(),
            }
        }
    }

    /// Get the underlying wave container to load all signals that are being displayed
    ///
    /// This is needed for wave containers that lazy-load signals.
    fn load_waves(&mut self) -> Option<LoadSignalsCmd> {
        let variables = self.displayed_items.values().filter_map(|item| match item {
            DisplayedItem::Variable(r) => Some(&r.variable_ref),
            _ => None,
        });
        self.inner
            .as_waves_mut()
            .unwrap()
            .load_variables(variables)
            .expect("internal error: failed to load variables")
    }

    /// Needs to be called after `update_with`, once the new number of timestamps is available in
    /// the inner `WaveContainer`.
    pub fn update_viewports(&mut self) {
        if let Some(old_num_timestamps) = std::mem::take(&mut self.old_num_timestamps) {
            // FIXME: I'm not sure if Defaulting to 1 time step is the right thing to do if we
            // have none, but it does avoid some potentially nasty division by zero problems
            let new_num_timestamps = self
                .inner
                .max_timestamp()
                .unwrap_or_else(BigUint::one)
                .to_bigint()
                .unwrap();
            if new_num_timestamps != old_num_timestamps {
                for viewport in &mut self.viewports {
                    *viewport = viewport.clip_to(&old_num_timestamps, &new_num_timestamps);
                }
            }
        }
    }

    fn update_displayed_items(
        waves: &WaveContainer,
        items: &HashMap<DisplayedItemRef, DisplayedItem>,
        keep_unavailable: bool,
        translators: &TranslatorList,
        items_tree: &mut DisplayedItemTree,
    ) -> HashMap<DisplayedItemRef, DisplayedItem> {
        items
            .iter()
            .filter_map(|(&id, i)| {
                let new = match i {
                    // keep without a change
                    DisplayedItem::Divider(_)
                    | DisplayedItem::Marker(_)
                    | DisplayedItem::TimeLine(_)
                    | DisplayedItem::Stream(_)
                    | DisplayedItem::Group(_) => Some((id, i.clone())),
                    DisplayedItem::Variable(s) => {
                        s.update(waves, keep_unavailable).map(|r| (id, r))
                    }
                    DisplayedItem::Placeholder(p) => {
                        match waves.update_variable_ref(&p.variable_ref) {
                            None => {
                                if keep_unavailable {
                                    Some((id, DisplayedItem::Placeholder(p.clone())))
                                } else {
                                    None
                                }
                            }
                            Some(new_variable_ref) => {
                                let Ok(meta) = waves
                                    .variable_meta(&new_variable_ref)
                                    .context("When updating")
                                    .map_err(|e| error!("{e:#?}"))
                                else {
                                    return Some((id, DisplayedItem::Placeholder(p.clone())));
                                };
                                let translator = variable_translator(
                                    p.format.as_ref(),
                                    &[],
                                    translators,
                                    || Ok(meta.clone()),
                                );
                                let info = translator.variable_info(&meta).unwrap();
                                Some((
                                    id,
                                    DisplayedItem::Variable(
                                        p.clone().into_variable(info, new_variable_ref),
                                    ),
                                ))
                            }
                        }
                    }
                };

                // remove element from item_tree if we are about to remove it from the displayed_items
                // we only remove variables or placeholders, so we don't have to think about traversing
                if new.is_none() {
                    let removed = items_tree.drain_recursive_if(|n| n.item_ref == id);
                    assert!(
                        removed.len() <= 1,
                        "more elements removed then should be possible"
                    )
                }

                new
            })
            .collect()
    }

    #[must_use]
    pub fn select_preferred_translator(
        &self,
        var: &VariableMeta,
        translators: &TranslatorList,
    ) -> String {
        select_preferred_translator(var, translators)
    }

    #[must_use]
    pub fn variable_translator<'a>(
        &'a self,
        field: &DisplayedFieldRef,
        translators: &'a TranslatorList,
    ) -> &'a DynTranslator {
        let Some(DisplayedItem::Variable(displayed_variable)) =
            self.displayed_items.get(&field.item)
        else {
            panic!("asking for translator for a non DisplayItem::Variable item")
        };

        variable_translator(
            displayed_variable.get_format(&field.field),
            &field.field,
            translators,
            || {
                self.inner
                    .as_waves()
                    .unwrap()
                    .variable_meta(&displayed_variable.variable_ref)
            },
        )
    }

    #[must_use]
    pub fn variable_translator_with_meta<'a>(
        &'a self,
        field: &DisplayedFieldRef,
        translators: &'a TranslatorList,
        meta: &VariableMeta,
    ) -> &'a DynTranslator {
        let Some(DisplayedItem::Variable(displayed_variable)) =
            self.displayed_items.get(&field.item)
        else {
            panic!("asking for translator for a non DisplayItem::Variable item")
        };

        variable_translator(
            displayed_variable.get_format(&field.field),
            &field.field,
            translators,
            || Ok(meta.clone()),
        )
    }

    pub fn add_variables(
        &mut self,
        translators: &TranslatorList,
        variables: Vec<VariableRef>,
        target_position: Option<TargetPosition>,
        update_display_names: bool,
        ignore_failures: bool,
        variable_name_type: Option<VariableNameType>,
    ) -> (Option<LoadSignalsCmd>, Vec<DisplayedItemRef>) {
        let mut indices = vec![];
        // load variables from waveform
        let res = match self
            .inner
            .as_waves_mut()
            .unwrap()
            .load_variables(variables.iter())
        {
            Err(e) => {
                error!("{e:#?}");
                return (None, indices);
            }
            Ok(res) => res,
        };

        // initialize translator and add display item
        let mut target_position = target_position
            .or_else(|| self.insert_position(self.focused_item))
            .unwrap_or(self.end_insert_position());
        for variable in variables {
            let Ok(meta) = self
                .inner
                .as_waves()
                .unwrap()
                .variable_meta(&variable)
                .context("When adding variable")
                .map_err(|e| error!("{e:#?}"))
            else {
                if ignore_failures {
                    continue;
                }
                return (res, indices);
            };

            let translator = variable_translator(None, &[], translators, || Ok(meta.clone()));
            let info = translator.variable_info(&meta).unwrap();

            let new_variable = DisplayedItem::Variable(DisplayedVariable {
                variable_ref: variable.clone(),
                info,
                color: None,
                background_color: None,
                display_name: variable.name.clone(),
                display_name_type: variable_name_type.unwrap_or(self.default_variable_name_type),
                manual_name: None,
                format: None,
                field_formats: vec![],
                height_scaling_factor: None,
                analog: None,
            });

            indices.push(self.insert_item(new_variable, Some(target_position), true));
            target_position = TargetPosition {
                before: ItemIndex(target_position.before.0 + 1),
                level: target_position.level,
            }
        }

        if update_display_names {
            self.compute_variable_display_names();
        }
        (res, indices)
    }

    /// Remove a single item, it's legal to call this function with an invalid ID
    pub fn remove_displayed_item(&mut self, id: DisplayedItemRef) {
        let Some(idx) = self
            .items_tree
            .iter()
            .enumerate()
            .find(|(_, node)| node.item_ref == id)
            .map(|(idx, _)| ItemIndex(idx))
        else {
            return;
        };

        let focused_item_ref = self
            .focused_item
            .and_then(|vidx| self.items_tree.get_visible(vidx))
            .map(|node| node.item_ref);

        for removed_ref in self.items_tree.remove_recursive(idx) {
            if let Some(DisplayedItem::Marker(m)) = self.displayed_items.remove(&removed_ref) {
                self.markers.remove(&m.idx);
            }
        }

        self.focused_item = focused_item_ref.and_then(|focused_item_ref| {
            match self
                .items_tree
                .iter_visible()
                .find_position(|node| node.item_ref == focused_item_ref)
                .map(|(vidx, _)| VisibleItemIndex(vidx))
            {
                Some(vidx) => Some(vidx),
                None if self
                    .focused_item
                    .and_then(|focused_vidx| self.items_tree.to_displayed(focused_vidx))
                    .is_some() =>
                {
                    Some(self.focused_item.unwrap())
                }
                None => self
                    .items_tree
                    .iter_visible()
                    .count()
                    .checked_sub(1)
                    .map(VisibleItemIndex),
            }
        });
    }

    pub fn add_divider(&mut self, name: Option<String>, vidx: Option<VisibleItemIndex>) {
        self.insert_item(
            DisplayedItem::Divider(DisplayedDivider {
                color: None,
                background_color: None,
                name,
            }),
            self.insert_position(vidx),
            true,
        );
    }

    pub fn add_timeline(&mut self, vidx: Option<VisibleItemIndex>) {
        self.insert_item(
            DisplayedItem::TimeLine(DisplayedTimeLine {
                color: None,
                background_color: None,
                name: None,
            }),
            self.insert_position(vidx),
            true,
        );
    }

    pub fn add_group(
        &mut self,
        name: String,
        target_position: Option<TargetPosition>,
    ) -> DisplayedItemRef {
        self.insert_item(
            DisplayedItem::Group(DisplayedGroup {
                name,
                color: None,
                background_color: None,
                content: vec![],
                is_open: false,
            }),
            target_position,
            true,
        )
    }

    pub fn add_generator(&mut self, gen_ref: TransactionStreamRef) {
        let Some(gen_id) = gen_ref.gen_id else { return };
        let Some(transactions) = self.inner.as_transactions_mut() else {
            return;
        };
        let is_empty = {
            let Some(generator) = transactions.get_generator(gen_id) else {
                return;
            };
            generator.transactions.is_empty()
        };
        if is_empty {
            info!("(Generator {gen_id}) Loading transactions into memory!");
            match transactions
                .inner
                .load_stream_into_memory(gen_ref.stream_id)
            {
                Ok(()) => info!("(Generator {gen_id}) Finished loading transactions!"),
                Err(_) => return,
            }
        }

        let mut last_times_on_row = vec![(BigUint::ZERO, BigUint::ZERO)];
        let Some(generator) = transactions.get_generator(gen_id) else {
            return;
        };
        calculate_rows_of_stream(&generator.transactions, &mut last_times_on_row);

        let new_gen = DisplayedItem::Stream(DisplayedStream {
            display_name: gen_ref.name.clone(),
            transaction_stream_ref: gen_ref,
            color: None,
            background_color: None,
            manual_name: None,
            rows: last_times_on_row.len(),
        });

        self.insert_item(new_gen, None, true);
    }

    pub fn add_stream(&mut self, stream_ref: TransactionStreamRef) {
        if self
            .inner
            .as_transactions_mut()
            .unwrap()
            .get_stream(stream_ref.stream_id)
            .unwrap()
            .transactions_loaded
            .not()
        {
            info!("(Stream) Loading transactions into memory!");
            match self
                .inner
                .as_transactions_mut()
                .unwrap()
                .inner
                .load_stream_into_memory(stream_ref.stream_id)
            {
                Ok(()) => info!(
                    "(Stream {}) Finished loading transactions!",
                    stream_ref.stream_id
                ),
                Err(_) => return,
            }
        }

        let stream = self
            .inner
            .as_transactions()
            .unwrap()
            .get_stream(stream_ref.stream_id)
            .unwrap();
        let mut last_times_on_row = vec![(BigUint::ZERO, BigUint::ZERO)];

        for gen_id in &stream.generators {
            let generator = self
                .inner
                .as_transactions()
                .unwrap()
                .get_generator(*gen_id)
                .unwrap();
            calculate_rows_of_stream(&generator.transactions, &mut last_times_on_row);
        }

        let new_stream = DisplayedItem::Stream(DisplayedStream {
            display_name: stream_ref.name.clone(),
            transaction_stream_ref: stream_ref,
            color: None,
            background_color: None,
            manual_name: None,
            rows: last_times_on_row.len(),
        });

        self.insert_item(new_stream, None, true);
    }

    pub fn add_all_streams(&mut self) {
        let mut streams: Vec<(usize, String)> = vec![];
        for stream in self.inner.as_transactions().unwrap().get_streams() {
            streams.push((stream.id, stream.name.clone()));
        }

        for (id, name) in streams {
            self.add_stream(TransactionStreamRef::new_stream(id, name));
        }
    }

    /// Return an insert position based on item
    ///
    /// If an item is passed, and it is
    /// - an unfolded group, insert index is to the first element of the group
    /// - a folded group, insert index is to before the next sibling (if exists)
    /// - otherwise insert index is past it on the same level
    #[must_use]
    pub fn insert_position(&self, vidx: Option<VisibleItemIndex>) -> Option<TargetPosition> {
        let vidx = vidx?;
        let item_index = self.items_tree.to_displayed(vidx)?;
        let node = self.items_tree.get(item_index)?;
        let item = self.displayed_items.get(&node.item_ref)?;

        // TODO add get_next_sibling to tree?
        let (before, level) = match item {
            DisplayedItem::Group(..) if node.unfolded => (item_index.0 + 1, node.level + 1),
            DisplayedItem::Group(..) => {
                let next_idx = self.items_tree.to_displayed(VisibleItemIndex(vidx.0 + 1));
                match next_idx {
                    Some(idx) => (idx.0, node.level),
                    None => (self.items_tree.len(), node.level),
                }
            }
            _ => (item_index.0 + 1, node.level),
        };
        Some(TargetPosition {
            before: ItemIndex(before),
            level,
        })
    }

    /// Return insert position as last item
    #[must_use]
    pub fn end_insert_position(&self) -> TargetPosition {
        TargetPosition {
            before: ItemIndex(self.items_tree.len()),
            level: 0,
        }
    }

    #[must_use]
    pub fn index_for_ref_or_focus(&self, item_ref: Option<DisplayedItemRef>) -> Option<ItemIndex> {
        if let Some(item_ref) = item_ref {
            self.items_tree
                .iter()
                .enumerate()
                .find_map(|(idx, node)| (node.item_ref == item_ref).then_some(ItemIndex(idx)))
        } else if let Some(focused_item) = self.focused_item {
            self.items_tree
                .get_visible_extra(focused_item)
                .map(|info| info.idx)
        } else {
            None
        }
    }

    /// Insert item after item vidx if Some(vidx).
    /// If None, insert in relation to focused item (see [`Self::focused_insert_position()`]).
    /// If nothing is selected, fall back to appending.
    /// Focus on the inserted item if there was a focused item.
    pub(crate) fn insert_item(
        &mut self,
        new_item: DisplayedItem,
        target_position: Option<TargetPosition>,
        move_focus: bool,
    ) -> DisplayedItemRef {
        let target_position = target_position
            .or_else(|| self.insert_position(self.focused_item))
            .unwrap_or_else(|| self.end_insert_position());

        let item_ref = self.next_displayed_item_ref();
        let insert_index = self
            .items_tree
            .insert_item(item_ref, target_position)
            .unwrap();
        self.displayed_items.insert(item_ref, new_item);
        if move_focus {
            self.focused_item = self.focused_item.and_then(|_| {
                self.items_tree
                    .iter_visible_extra()
                    .find_map(|info| (info.idx == insert_index).then_some(info.vidx))
            });
        }
        self.items_tree.xselect_all_visible(false);
        item_ref
    }

    pub fn go_to_cursor_if_not_in_view(&mut self) -> bool {
        if let Some(cursor) = &self.cursor {
            let num_timestamps = self.safe_num_timestamps();
            self.viewports[0].go_to_cursor_if_not_in_view(cursor, &num_timestamps)
        } else {
            false
        }
    }

    #[inline]
    pub fn numbered_marker_location(&self, idx: u8, viewport: &Viewport, view_width: f32) -> f32 {
        viewport.pixel_from_time(
            self.numbered_marker_time(idx),
            view_width,
            &self.safe_num_timestamps(),
        )
    }

    #[inline]
    #[must_use]
    pub fn numbered_marker_time(&self, idx: u8) -> &BigInt {
        self.markers.get(&idx).unwrap()
    }

    #[must_use]
    pub fn viewport_all(&self) -> Viewport {
        Viewport::new()
    }

    pub fn remove_placeholders(&mut self) {
        let removed_refs = self.items_tree.drain_recursive_if(|node| {
            matches!(
                self.displayed_items.get(&node.item_ref),
                Some(DisplayedItem::Placeholder(_))
            )
        });
        for removed_ref in removed_refs {
            self.displayed_items.remove(&removed_ref);
        }
    }

    #[inline]
    #[must_use]
    pub fn any_displayed(&self) -> bool {
        !self.displayed_items.is_empty()
    }

    /// Find the top-most of the currently visible items.
    #[must_use]
    /// Returns the index of the item currently at the top of the visible area.
    pub fn get_top_item(&self) -> usize {
        if self.drawing_infos.is_empty() {
            return 0;
        }
        // drawing_infos contains content-space positions from the last draw.
        // The visible top is at: first_element_y + scroll_offset
        let first_element_y = self.drawing_infos.first().unwrap().top();
        let visible_top = first_element_y + self.scroll_offset;

        self.drawing_infos
            .iter()
            .enumerate()
            .find(|(_, di)| di.top() >= visible_top - 1.) // 1px margin for floating-point errors
            .map_or(self.drawing_infos.len() - 1, |(idx, _)| idx)
    }

    /// Find the item at a given y-location.
    #[must_use]
    pub fn get_item_at_y(&self, y: f32) -> Option<VisibleItemIndex> {
        if self.drawing_infos.is_empty() {
            return None;
        }
        let first_element_top = self.drawing_infos.first().unwrap().top();
        let first_element_bottom = self.drawing_infos.last().unwrap().bottom();
        let threshold = y + first_element_top + self.scroll_offset;
        if first_element_bottom <= threshold {
            return None;
        }
        self.drawing_infos
            .iter()
            .enumerate()
            .rev()
            .find(|(_, di)| di.top() <= threshold)
            .map(|(vidx, _)| VisibleItemIndex(vidx))
    }

    pub fn scroll_to_item(&mut self, idx: usize) {
        if self.drawing_infos.is_empty() {
            return;
        }
        let first_element_y = self.drawing_infos.first().unwrap().top();
        let last_element_bottom = self.drawing_infos.last().unwrap().bottom();
        let content_height = last_element_bottom - first_element_y;

        // Don't scroll if all content fits in viewport
        let max_scroll = content_height - self.total_height;
        if max_scroll <= 0.0 {
            return;
        }

        let item_y = self
            .drawing_infos
            .get(idx)
            .unwrap_or_else(|| self.drawing_infos.last().unwrap())
            .top();
        let target_scroll = item_y - first_element_y;

        // Clamp scroll to valid range: [0, max_scroll]
        self.scroll_offset = target_scroll.clamp(0.0, max_scroll);
    }

    /// Set cursor at next (or previous, if `next` is false) transition of `variable`. If `skip_zero` is true,
    /// use the next transition to a non-zero value.
    pub fn set_cursor_at_transition(
        &mut self,
        next: bool,
        variable: Option<VisibleItemIndex>,
        skip_zero: bool,
    ) {
        if let Some(vidx) = variable.or(self.focused_item)
            && let Some(cursor) = &self.cursor
            && let Some(DisplayedItem::Variable(variable)) = &self
                .items_tree
                .get_visible(vidx)
                .and_then(|node| self.displayed_items.get(&node.item_ref))
            && let Ok(Some(res)) = self.inner.as_waves().unwrap().query_variable(
                &variable.variable_ref,
                &cursor.to_biguint().unwrap_or_default(),
            )
        {
            if next {
                if let Some(ref time) = res.next {
                    let stime = time.to_bigint();
                    if stime.is_some() {
                        self.cursor.clone_from(&stime);
                    }
                } else {
                    // No next transition, go to end
                    if let Some(end_time) = self.num_timestamps() {
                        self.cursor = Some(end_time);
                    } else {
                        warn!(
                            "Set cursor at transition: No timestamp count even though waveforms should be loaded"
                        );
                    }
                }
            } else if let Some(stime) = res.current.unwrap().0.to_bigint() {
                let bigone = BigInt::from(1);
                // Check if we are on a transition
                if stime == *cursor && *cursor >= bigone {
                    // If so, subtract cursor position by one
                    if let Ok(Some(newres)) = self.inner.as_waves().unwrap().query_variable(
                        &variable.variable_ref,
                        &(cursor - bigone).to_biguint().unwrap_or_default(),
                    ) && let Some(current) = newres.current
                    {
                        let newstime = current.0.to_bigint();
                        if newstime.is_some() {
                            self.cursor.clone_from(&newstime);
                        }
                    }
                } else {
                    self.cursor = Some(stime);
                }
            }

            // if zero edges should be skipped
            if skip_zero {
                // check if the next transition is 0, if so and requested, go to
                // next positive transition
                if let Some(time) = &self.cursor {
                    let next_value = self.inner.as_waves().unwrap().query_variable(
                        &variable.variable_ref,
                        &time.to_biguint().unwrap_or_default(),
                    );
                    if next_value.is_ok_and(|r| {
                        r.is_some_and(|r| {
                            r.current.is_some_and(|v| match v.1 {
                                VariableValue::BigUint(v) => v.is_zero(),
                                VariableValue::String(_) => false,
                            })
                        })
                    }) {
                        self.set_cursor_at_transition(next, Some(vidx), false);
                    }
                }
            }
        }
    }

    pub fn next_displayed_item_ref(&mut self) -> DisplayedItemRef {
        self.display_item_ref_counter += 1;
        self.display_item_ref_counter.into()
    }

    /// Returns the number of timestamps in the current waves. For now, this adjusts the
    /// number of timestamps as returned by wave sources if they specify 0 timestamps. This is
    /// done to avoid having to consider what happens with the viewport.
    #[must_use]
    pub fn num_timestamps(&self) -> Option<BigInt> {
        self.inner
            .max_timestamp()
            .and_then(|r| if r.is_zero() { None } else { Some(r) })
            .and_then(|r| r.to_bigint())
    }

    /// Returns the number of timestamps in the current waves. This is like `num_timestamps` but
    /// will always return at least 1.
    #[must_use]
    pub fn safe_num_timestamps(&self) -> BigInt {
        self.num_timestamps().unwrap_or_else(BigInt::one)
    }

    #[must_use]
    pub fn get_displayed_item_index(
        &self,
        item_ref: &DisplayedItemRef,
    ) -> Option<VisibleItemIndex> {
        // TODO check where this is called since it could now fail...
        self.items_tree
            .iter_visible()
            .enumerate()
            .find_map(|(vidx, node)| {
                if node.item_ref == *item_ref {
                    Some(VisibleItemIndex(vidx))
                } else {
                    None
                }
            })
    }

    /// Spawn async worker to build analog cache. Worker holds Arc clone.
    pub fn build_analog_cache_async(
        &self,
        entry: std::sync::Arc<crate::analog_signal_cache::AnalogCacheEntry>,
        variable_ref: &VariableRef,
        translator: crate::translation::AnyTranslator,
        sender: &std::sync::mpsc::Sender<crate::message::Message>,
    ) -> Option<()> {
        let wave_container = self.inner.as_waves()?;
        let meta = wave_container.variable_meta(variable_ref).ok()?.clone();

        let num_timestamps = self.num_timestamps()?.to_u64()?;

        let accessor = wave_container.signal_accessor(entry.cache_key.0).ok()?;

        let sender_clone = sender.clone();
        crate::async_util::perform_work(move || {
            let result = crate::analog_signal_cache::AnalogSignalCache::build(
                accessor,
                &translator,
                &meta,
                num_timestamps,
                None,
            );

            let msg = match result {
                Some(cache) => crate::message::Message::AnalogCacheBuilt {
                    entry: entry.clone(),
                    result: Ok(cache),
                },
                None => crate::message::Message::AnalogCacheBuilt {
                    entry: entry.clone(),
                    result: Err("Failed to build analog cache".into()),
                },
            };

            crate::OUTSTANDING_TRANSACTIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = sender_clone.send(msg);

            if let Some(ctx) = crate::EGUI_CONTEXT.read().unwrap().as_ref() {
                ctx.request_repaint();
            }
        });

        Some(())
    }

    pub fn set_active_scope(&mut self, scope: Option<ScopeType>) -> Option<()> {
        if let Some(scope) = scope {
            let scope = if let ScopeType::StreamScope(StreamScopeRef::Empty(name)) = scope {
                let inner = self.inner.as_transactions()?;
                ScopeType::StreamScope(StreamScopeRef::new_stream_from_name(inner, name))
            } else {
                scope
            };

            if self.inner.scope_exists(&scope) {
                self.active_scope = Some(scope);
            } else {
                warn!("Setting active scope to {scope} which does not exist");
            }
        } else {
            // Set to top-level scope
            self.active_scope = None;
        };
        Some(())
    }
}

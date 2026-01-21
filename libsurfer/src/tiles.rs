//! Tile-based layout system for Central Panel using egui_tiles.

use ecolor::Color32;
use egui::{ScrollArea, TextWrapMode, Ui, Visuals};
use egui_tiles::{
    Behavior, Container, LinearDir, SimplificationOptions, TabState, Tile, TileId, Tiles, Tree,
};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

use crate::message::Message;
use crate::system_state::SystemState;

/// Unique identifier for tiles within the application
pub type SurferTileId = u64;

/// Pane types supported by the tiling system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SurferPane {
    /// The primary waveform view with variable list, values, and canvas.
    /// Only one instance exists; it cannot be closed or removed.
    Waveform,
    /// Placeholder tile, for debug purposes.
    /// Renders current tile tree. Can be closed by user.
    DebugTile(SurferTileId),
}

impl SurferPane {
    /// Returns the display name for this pane type
    fn title(&self) -> String {
        match self {
            SurferPane::Waveform => "Waveform".to_string(),
            SurferPane::DebugTile(id) => format!("Debug Tile {id}"),
        }
    }
}

/// Wrapper around egui_tiles::Tree with Surfer-specific configuration
#[derive(Serialize, Deserialize)]
pub struct SurferTileTree {
    /// The underlying egui_tiles tree
    pub tree: Tree<SurferPane>,
    /// Counter for generating unique tile IDs
    next_tile_id: SurferTileId,
}

impl Default for SurferTileTree {
    fn default() -> Self {
        Self::new()
    }
}

impl SurferTileTree {
    /// Creates a new tile tree with a single waveform tile
    fn new() -> Self {
        let mut tiles = Tiles::default();
        let waveform_pane = tiles.insert_pane(SurferPane::Waveform);
        let root = tiles.insert_tab_tile(vec![waveform_pane]);

        Self {
            tree: Tree::new("surfer_tiles", root, tiles),
            next_tile_id: 1,
        }
    }

    /// Returns the number of visible panes in the tree
    fn pane_count(&self) -> usize {
        self.tree
            .tiles
            .iter()
            .filter(|(_, tile)| matches!(tile, Tile::Pane(_)))
            .count()
    }

    /// Returns true if only the waveform pane exists (no additional tiles)
    pub fn is_single_waveform(&self) -> bool {
        self.pane_count() == 1
    }

    /// Adds a new debug tile to the tree.
    ///
    /// Layout strategy:
    /// - First add: Creates a vertical split with waveform on top, new tile on bottom
    /// - Subsequent adds: Splits the bottom section horizontally
    pub fn add_debug_tile(&mut self) {
        let Some(root) = self.tree.root() else {
            return;
        };

        let new_pane = self
            .tree
            .tiles
            .insert_pane(SurferPane::DebugTile(self.next_tile_id));
        self.next_tile_id += 1;

        // Check if root is a vertical split with a bottom section
        let bottom_id = self.tree.tiles.get(root).and_then(|tile| {
            if let Tile::Container(Container::Linear(linear)) = tile
                && linear.dir == LinearDir::Vertical
                && linear.children.len() >= 2
            {
                Some(linear.children[1])
            } else {
                None
            }
        });

        let Some(bottom_id) = bottom_id else {
            // First tile: create vertical split with waveform on top, new tile on bottom
            let new_root = self.tree.tiles.insert_vertical_tile(vec![root, new_pane]);
            self.tree.root = Some(new_root);
            return;
        };

        // If bottom is already horizontal, add directly to it
        if let Some(Tile::Container(Container::Linear(bottom))) = self.tree.tiles.get_mut(bottom_id)
            && bottom.dir == LinearDir::Horizontal
        {
            bottom.add_child(new_pane);
            return;
        }

        // Wrap existing bottom in a horizontal split
        let new_horizontal = self
            .tree
            .tiles
            .insert_horizontal_tile(vec![bottom_id, new_pane]);
        if let Some(Tile::Container(Container::Linear(linear))) = self.tree.tiles.get_mut(root) {
            linear.children[1] = new_horizontal;
        }
    }

    /// Removes a tile by its TileId.
    /// The waveform tile cannot be removed.
    fn remove_tile(&mut self, tile_id: TileId) {
        // Never remove the waveform tile
        if let Some(Tile::Pane(SurferPane::Waveform)) = self.tree.tiles.get(tile_id) {
            return;
        }
        self.tree.remove_recursively(tile_id);
    }
}

pub struct SurferTileBehavior<'a> {
    pub state: &'a mut SystemState,
    pub ctx: &'a egui::Context,
    pub msgs: &'a mut Vec<Message>,
    pub hide_chrome: bool,
    pub tile_to_remove: Option<TileId>,
    pub debug_tree: String,
}

impl Behavior<SurferPane> for SurferTileBehavior<'_> {
    fn pane_ui(
        &mut self,
        ui: &mut Ui,
        _tile_id: TileId,
        pane: &mut SurferPane,
    ) -> egui_tiles::UiResponse {
        match pane {
            SurferPane::Waveform => {
                self.state.draw_waveform_tile(self.ctx, ui, self.msgs);
            }
            SurferPane::DebugTile(id) => {
                ui.label(format!("Debug Tile {id}"));
                ScrollArea::both().show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(self.debug_tree.as_str()).monospace())
                            .wrap_mode(TextWrapMode::Extend),
                    );
                });
            }
        }
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &SurferPane) -> egui::WidgetText {
        pane.title().into()
    }

    fn is_tab_closable(&self, tiles: &Tiles<SurferPane>, tile_id: TileId) -> bool {
        // Waveform tile is never closable; Empty tiles are closable when chrome is visible
        if let Some(Tile::Pane(SurferPane::Waveform)) = tiles.get(tile_id) {
            return false;
        }
        !self.hide_chrome
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<SurferPane>, tile_id: TileId) -> bool {
        self.tile_to_remove = Some(tile_id);
        true
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        if self.hide_chrome {
            0.0 // Hide tab bar in single waveform mode
        } else {
            24.0
        }
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        if self.hide_chrome { 0.0 } else { 2.0 }
    }

    /// Returns options that control how the tile tree is auto-simplified after modifications.
    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            prune_empty_tabs: true,
            prune_empty_containers: true,
            prune_single_child_tabs: true,
            prune_single_child_containers: true,
            all_panes_must_have_tabs: true,
            join_nested_linear_containers: true,
        }
    }

    fn tab_bar_color(&self, _visuals: &Visuals) -> Color32 {
        self.state.user.config.theme.secondary_ui_color.background
    }

    fn tab_bg_color(
        &self,
        _visuals: &Visuals,
        _tiles: &Tiles<SurferPane>,
        _tile_id: TileId,
        tab_state: &TabState,
    ) -> Color32 {
        if tab_state.active {
            self.state.user.config.theme.primary_ui_color.background
        } else {
            self.state.user.config.theme.secondary_ui_color.background
        }
    }
}

impl SystemState {
    /// Renders the tile tree using the take-and-restore pattern:
    pub fn draw_tiles(&mut self, ctx: &egui::Context, msgs: &mut Vec<Message>, ui: &mut Ui) {
        // `egui_tiles::Tree::ui()` requires `&mut Tree` to handle user interactions
        // (tab reordering, resizing, closing). Meanwhile, `SurferTileBehavior::pane_ui()`
        // needs `&mut SystemState` for waveform rendering. This creates a borrow conflict.
        // Take tree out of self to enable disjoint borrows.
        let mut tile_tree = std::mem::take(&mut self.user.tile_tree);
        let debug_tree_str = format_tile_tree_cli(&tile_tree.tree);
        let hide_chrome = tile_tree.is_single_waveform();

        let mut behavior = SurferTileBehavior {
            state: self,
            ctx,
            msgs,
            hide_chrome,
            tile_to_remove: None,
            debug_tree: debug_tree_str,
        };

        tile_tree.tree.ui(&mut behavior, ui);

        // Handle deferred tile removal, does it make sense to create a message?
        if let Some(tile_id) = behavior.tile_to_remove {
            tile_tree.remove_tile(tile_id);
        }

        // Restore tree back to self
        self.user.tile_tree = tile_tree;
    }
}

/// Format tiles into a CLI-style tree for debug output.
fn format_tile_tree_cli(tree: &Tree<SurferPane>) -> String {
    let mut out = String::new();
    let Some(root) = tree.root else {
        out.push_str("(empty)\n");
        return out;
    };

    writeln!(&mut out, "root").ok();
    let mut stack = vec![(root, String::new(), true)];
    while let Some((tile_id, prefix, is_last)) = stack.pop() {
        let branch = if is_last { "└── " } else { "├── " };
        let next_prefix = if is_last { "    " } else { "│   " };

        let description = match tree.tiles.get(tile_id) {
            Some(Tile::Pane(pane)) => format!("{tile_id:?} Pane({})", pane.title()),
            Some(Tile::Container(container)) => {
                let kind = match container {
                    Container::Tabs(_) => "Tabs".to_string(),
                    Container::Linear(linear) => format!(
                        "Linear({})",
                        match linear.dir {
                            LinearDir::Horizontal => "Horizontal",
                            LinearDir::Vertical => "Vertical",
                        }
                    ),
                    Container::Grid(_) => "Grid".to_string(),
                };
                format!("{tile_id:?} {kind}")
            }
            None => format!("{tile_id:?} DANGLING"),
        };

        writeln!(out, "{prefix}{branch}{description}").ok();

        if let Some(Tile::Container(container)) = tree.tiles.get(tile_id) {
            let children: Vec<TileId> = container.children().copied().collect();
            let last_index = children.len().saturating_sub(1);
            let child_prefix = format!("{prefix}{next_prefix}");
            for (index, child) in children.iter().enumerate().rev() {
                let child_is_last = index == last_index;
                stack.push((*child, child_prefix.clone(), child_is_last));
            }
        }
    }
    out
}

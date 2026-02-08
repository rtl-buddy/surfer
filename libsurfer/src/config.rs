use config::builder::DefaultState;
use config::{Config, ConfigBuilder};
#[cfg(not(target_arch = "wasm32"))]
use config::{Environment, File};
use derive_more::{Display, FromStr};
#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;
use ecolor::Color32;
use enum_iterator::Sequence;
use epaint::{PathStroke, Stroke};
use eyre::Report;
use eyre::{Context, Result};
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::hierarchy::{HierarchyStyle, ParameterDisplayLocation};
use crate::keyboard_shortcuts::{SurferShortcuts, deserialize_shortcuts};
use crate::mousegestures::GestureZones;
use crate::time::TimeFormat;
use crate::wave_container::VariableMeta;
use crate::{clock_highlighting::ClockHighlightType, variable_name_type::VariableNameType};
use surfer_translation_types::VariableEncoding;

macro_rules! theme {
    ($name:expr) => {
        (
            $name,
            include_str!(concat!("../../themes/", $name, ".toml")),
        )
    };
}

/// Built-in theme names and their corresponding embedded content
static BUILTIN_THEMES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        theme!("dark+"),
        theme!("dark-high-contrast"),
        theme!("ibm"),
        theme!("light+"),
        theme!("light-high-contrast"),
        ("okabe/ito", include_str!("../../themes/okabe-ito.toml")),
        theme!("petroff-dark"),
        theme!("petroff-light"),
        theme!("solarized"),
    ])
});

#[cfg(not(target_arch = "wasm32"))]
pub static PROJECT_DIR: LazyLock<Option<ProjectDirs>> =
    LazyLock::new(|| ProjectDirs::from("org", "surfer-project", "surfer"));
#[cfg(not(target_arch = "wasm32"))]
const OLD_CONFIG_FILE: &str = "surfer.toml";
#[cfg(not(target_arch = "wasm32"))]
const CONFIG_FILE: &str = "config.toml";
#[cfg(not(target_arch = "wasm32"))]
const THEMES_DIR: &str = "themes";
#[cfg(not(target_arch = "wasm32"))]
pub const LOCAL_DIR: &str = ".surfer";

/// Select the function of the arrow keys
#[derive(Clone, Copy, Debug, Deserialize, Display, FromStr, PartialEq, Eq, Sequence, Serialize)]
pub enum ArrowKeyBindings {
    /// The left/right arrow keys step to the next edge
    Edge,

    /// The left/right arrow keys scroll the viewport left/right
    Scroll,
}

#[derive(Clone, Copy, Debug, Deserialize, Display, FromStr, PartialEq, Eq, Sequence, Serialize)]
pub enum TransitionValue {
    /// Transition value is the previous value
    Previous,
    /// Transition value is the next value
    Next,
    /// Transition value is both previous and next value
    Both,
}

/// Select the function when dragging with primary mouse button
#[derive(Debug, Deserialize, Display, PartialEq, Eq, Sequence, Serialize, Clone, Copy)]
pub enum PrimaryMouseDrag {
    /// The left/right arrow keys step to the next edge
    #[display("Measure time")]
    Measure,

    /// The left/right arrow keys scroll the viewport left/right
    #[display("Move cursor")]
    Cursor,
}

#[derive(Debug, Deserialize, Display, PartialEq, Eq, Sequence, Serialize, Clone, Copy)]
pub enum AutoLoad {
    Always,
    Never,
    Ask,
}

impl AutoLoad {
    #[must_use]
    pub fn from_bool(auto_load: bool) -> Self {
        if auto_load {
            AutoLoad::Always
        } else {
            AutoLoad::Never
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SurferConfig {
    pub layout: SurferLayout,
    #[serde(deserialize_with = "deserialize_theme")]
    pub theme: SurferTheme,
    /// Mouse gesture configurations. Color and linewidth are configured in the theme using [`SurferTheme::gesture`].
    pub gesture: SurferGesture,
    pub behavior: SurferBehavior,
    /// Time stamp format
    pub default_time_format: TimeFormat,
    pub default_variable_name_type: VariableNameType,
    default_clock_highlight_type: ClockHighlightType,
    /// Distance in pixels for cursor snap
    pub snap_distance: f32,
    /// Maximum size of the undo stack
    pub undo_stack_size: usize,
    /// Reload changed waves
    autoreload_files: AutoLoad,
    /// Load state file
    autoload_sibling_state_files: AutoLoad,
    /// WCP Configuration
    pub wcp: WcpConfig,
    /// HTTP Server Configuration
    pub server: ServerConfig,
    /// Animation time for UI elements in seconds
    pub animation_time: f32,
    /// UI animation enabled
    pub animation_enabled: bool,
    /// Maximum URL length for remote connections.
    /// Should only be changed in case you are behind a proxy that limits the URL length
    pub max_url_length: u16,
    /// Keyboard shortcuts
    #[serde(deserialize_with = "deserialize_shortcuts")]
    pub shortcuts: SurferShortcuts,
}

impl SurferConfig {
    #[must_use]
    pub fn default_clock_highlight_type(&self) -> ClockHighlightType {
        self.default_clock_highlight_type
    }

    #[must_use]
    pub fn autoload_sibling_state_files(&self) -> AutoLoad {
        self.autoload_sibling_state_files
    }

    #[must_use]
    pub fn autoreload_files(&self) -> AutoLoad {
        self.autoreload_files
    }

    #[must_use]
    pub fn animation_enabled(&self) -> bool {
        self.animation_enabled
    }
}

#[derive(Debug, Deserialize)]
pub struct SurferLayout {
    /// Flag to show/hide the hierarchy view
    show_hierarchy: bool,
    /// Flag to show/hide the menu
    show_menu: bool,
    /// Flag to show/hide toolbar
    show_toolbar: bool,
    /// Flag to show/hide tick lines
    show_ticks: bool,
    /// Flag to show/hide tooltip for variables
    show_tooltip: bool,
    /// Flag to show/hide tooltip for scopes
    show_scope_tooltip: bool,
    /// Flag to show/hide the overview
    show_overview: bool,
    /// Flag to show/hide the statusbar
    show_statusbar: bool,
    /// Flag to show/hide the indices of variables in the variable list
    show_variable_indices: bool,
    /// Flag to show/hide the variable direction icon
    show_variable_direction: bool,
    /// Flag to show/hide a default timeline
    show_default_timeline: bool,
    /// Flag to show/hide empty scopes
    show_empty_scopes: bool,
    /// Flag to show/hide scope and variable type icons in the hierarchy
    show_hierarchy_icons: bool,
    /// Where to show parameters in the hierarchy
    parameter_display_location: ParameterDisplayLocation,
    /// Initial window height
    pub window_height: usize,
    /// Initial window width
    pub window_width: usize,
    /// Align variable names right
    align_names_right: bool,
    /// Set style of hierarchy
    hierarchy_style: HierarchyStyle,
    /// Text size in points for values in waves
    pub waveforms_text_size: f32,
    /// Line height in points for waves
    pub waveforms_line_height: f32,
    /// Line height multiples for higher variables
    pub waveforms_line_height_multiples: Vec<f32>,
    /// Line height in points for transaction streams
    pub transactions_line_height: f32,
    /// UI zoom factors
    pub zoom_factors: Vec<f32>,
    /// Default UI zoom factor
    pub default_zoom_factor: f32,
    #[serde(default)]
    /// Highlight the waveform of the focused item?
    highlight_focused: bool,
    /// Move the focus to the newly inserted marker?
    move_focus_on_inserted_marker: bool,
    /// Fill high values in boolean waveforms
    #[serde(default = "default_true")]
    fill_high_values: bool,
    /// Dinotrace drawing style (thick upper line for all-ones, no upper line for all-zeros)
    #[serde(default)]
    use_dinotrace_style: bool,
    /// Value to display when cursor is on a transition
    #[serde(default = "default_next")]
    transition_value: TransitionValue,
}

fn default_true() -> bool {
    true
}

fn default_next() -> TransitionValue {
    TransitionValue::Next
}

impl SurferLayout {
    #[must_use]
    pub fn show_hierarchy(&self) -> bool {
        self.show_hierarchy
    }
    #[must_use]
    pub fn show_menu(&self) -> bool {
        self.show_menu
    }
    #[must_use]
    pub fn show_ticks(&self) -> bool {
        self.show_ticks
    }
    #[must_use]
    pub fn show_tooltip(&self) -> bool {
        self.show_tooltip
    }
    #[must_use]
    pub fn show_scope_tooltip(&self) -> bool {
        self.show_scope_tooltip
    }
    #[must_use]
    pub fn show_default_timeline(&self) -> bool {
        self.show_default_timeline
    }
    #[must_use]
    pub fn show_toolbar(&self) -> bool {
        self.show_toolbar
    }
    #[must_use]
    pub fn show_overview(&self) -> bool {
        self.show_overview
    }
    #[must_use]
    pub fn show_statusbar(&self) -> bool {
        self.show_statusbar
    }
    #[must_use]
    pub fn align_names_right(&self) -> bool {
        self.align_names_right
    }
    #[must_use]
    pub fn show_variable_indices(&self) -> bool {
        self.show_variable_indices
    }
    #[must_use]
    pub fn show_variable_direction(&self) -> bool {
        self.show_variable_direction
    }
    #[must_use]
    pub fn default_zoom_factor(&self) -> f32 {
        self.default_zoom_factor
    }
    #[must_use]
    pub fn show_empty_scopes(&self) -> bool {
        self.show_empty_scopes
    }
    #[must_use]
    pub fn show_hierarchy_icons(&self) -> bool {
        self.show_hierarchy_icons
    }
    #[must_use]
    pub fn parameter_display_location(&self) -> ParameterDisplayLocation {
        self.parameter_display_location
    }
    #[must_use]
    pub fn highlight_focused(&self) -> bool {
        self.highlight_focused
    }
    #[must_use]
    pub fn move_focus_on_inserted_marker(&self) -> bool {
        self.move_focus_on_inserted_marker
    }
    #[must_use]
    pub fn fill_high_values(&self) -> bool {
        self.fill_high_values
    }
    #[must_use]
    pub fn hierarchy_style(&self) -> HierarchyStyle {
        self.hierarchy_style
    }
    #[must_use]
    pub fn use_dinotrace_style(&self) -> bool {
        self.use_dinotrace_style
    }
    #[must_use]
    pub fn transition_value(&self) -> TransitionValue {
        self.transition_value
    }
}

#[derive(Debug, Deserialize)]
pub struct SurferBehavior {
    /// Keep or remove variables if unavailable during reload
    pub keep_during_reload: bool,
    /// Select the functionality bound to the arrow keys
    pub arrow_key_bindings: ArrowKeyBindings,
    /// Whether dragging with primary mouse button will measure time or move cursor
    /// (press shift for the other)
    primary_button_drag_behavior: PrimaryMouseDrag,
}

impl SurferBehavior {
    #[must_use]
    pub fn primary_button_drag_behavior(&self) -> PrimaryMouseDrag {
        self.primary_button_drag_behavior
    }

    #[must_use]
    pub fn arrow_key_bindings(&self) -> ArrowKeyBindings {
        self.arrow_key_bindings
    }
}

#[derive(Debug, Deserialize)]
/// Mouse gesture configurations. Color and linewidth are configured in the theme using [`SurferTheme::gesture`].
pub struct SurferGesture {
    /// Size of the overlay help
    pub size: f32,
    /// (Squared) minimum distance to move to remove the overlay help and perform gesture
    pub deadzone: f32,
    /// Circle radius for background as a factor of size/2
    pub background_radius: f32,
    /// Gamma factor for background circle, between 0 (opaque) and 1 (transparent)
    pub background_gamma: f32,
    /// Mapping between the eight directions and actions
    pub mapping: GestureZones,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SurferLineStyle {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub color: Color32,
    pub width: f32,
}

impl From<SurferLineStyle> for Stroke {
    fn from(style: SurferLineStyle) -> Self {
        Stroke {
            color: style.color,
            width: style.width,
        }
    }
}

impl From<&SurferLineStyle> for Stroke {
    fn from(style: &SurferLineStyle) -> Self {
        Stroke {
            color: style.color,
            width: style.width,
        }
    }
}

impl From<&SurferLineStyle> for PathStroke {
    fn from(style: &SurferLineStyle) -> Self {
        PathStroke::new(style.width, style.color)
    }
}

#[derive(Debug, Deserialize)]
/// Tick mark configuration
pub struct SurferTicks {
    /// 0 to 1, where 1 means as many ticks that can fit without overlap
    pub density: f32,
    /// Line style to use for ticks
    pub style: SurferLineStyle,
}

#[derive(Debug, Deserialize)]
pub struct SurferRelationArrow {
    /// Arrow line style
    pub style: SurferLineStyle,

    /// Arrowhead angle in degrees
    pub head_angle: f32,

    /// Arrowhead length
    pub head_length: f32,
}

#[derive(Debug, Deserialize)]
pub struct SurferTheme {
    /// Color used for text across the UI
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color of borders between UI elements
    pub border_color: Color32,
    /// Color used for text across the markers
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub alt_text_color: Color32,
    /// Colors used for the background and text of the wave view
    pub canvas_colors: ThemeColorTriple,
    /// Colors used for most UI elements not on the variable canvas
    pub primary_ui_color: ThemeColorPair,
    /// Colors used for the variable and value list, as well as secondary elements
    /// like text fields
    pub secondary_ui_color: ThemeColorPair,
    /// Color used for selected ui elements such as the currently selected hierarchy
    pub selected_elements_colors: ThemeColorPair,

    pub accent_info: ThemeColorPair,
    pub accent_warn: ThemeColorPair,
    pub accent_error: ThemeColorPair,

    ///  Line style for cursor
    pub cursor: SurferLineStyle,

    /// Line style for mouse gesture lines
    pub gesture: SurferLineStyle,

    /// Line style for measurement lines
    pub measure: SurferLineStyle,

    ///  Line style for clock highlight lines
    pub clock_highlight_line: SurferLineStyle,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub clock_highlight_cycle: Color32,
    /// Draw arrows on rising clock edges
    pub clock_rising_marker: bool,

    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Default variable color
    pub variable_default: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for high-impedance variables
    pub variable_highimp: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for undefined variables
    pub variable_undef: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for don't-care variables
    pub variable_dontcare: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for weak variables
    pub variable_weak: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for constant variables (parameters)
    pub variable_parameter: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Default transaction color
    pub transaction_default: Color32,
    // Relation arrows of transactions
    pub relation_arrow: SurferRelationArrow,
    #[serde(deserialize_with = "deserialize_hex_color")]
    /// Color used for constant variables (parameters)
    pub variable_event: Color32,

    /// Opacity with which variable backgrounds are drawn. 0 is fully transparent and 1 is fully
    /// opaque.
    pub waveform_opacity: f32,
    /// Opacity of variable backgrounds for wide signals (signals with more than one bit)
    #[serde(default)]
    pub wide_opacity: f32,

    #[serde(default = "default_colors", deserialize_with = "deserialize_color_map")]
    pub colors: HashMap<String, Color32>,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub highlight_background: Color32,

    /// Variable line width
    pub linewidth: f32,

    /// Variable line width for accented variables
    pub thick_linewidth: f32,

    /// Vector transition max width
    pub vector_transition_width: f32,

    /// Number of lines using standard background before changing to
    /// alternate background and so on, set to zero to disable
    pub alt_frequency: usize,

    /// Viewport separator line
    pub viewport_separator: SurferLineStyle,

    // Drag hint and threshold parameters
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub drag_hint_color: Color32,
    pub drag_hint_width: f32,
    pub drag_threshold: f32,

    /// Tick information
    pub ticks: SurferTicks,

    /// List of theme names
    #[serde(default = "Vec::new")]
    pub theme_names: Vec<String>,

    /// Icons for scope types in the hierarchy view
    #[serde(default)]
    pub scope_icons: ScopeIcons,

    /// Icons for variable types in the hierarchy view
    #[serde(default)]
    pub variable_icons: VariableIcons,
}

/// Colors for different scope type icons in the hierarchy view.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ScopeIconColors {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub module: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub task: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub function: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub begin: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub fork: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub generate: Color32,
    #[serde(rename = "struct", deserialize_with = "deserialize_hex_color")]
    pub struct_: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub union: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub class: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub interface: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub package: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub program: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_architecture: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_procedure: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_function: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_record: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_process: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_block: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_for_generate: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_if_generate: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_generate: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_package: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub ghw_generic: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub vhdl_array: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub unknown: Color32,
}

impl Default for ScopeIconColors {
    fn default() -> Self {
        Self {
            module: Color32::from_rgb(0x4F, 0xC3, 0xF7), // Light Blue
            task: Color32::from_rgb(0xFF, 0xB7, 0x4D),   // Orange
            function: Color32::from_rgb(0xBA, 0x68, 0xC8), // Purple
            begin: Color32::from_rgb(0x81, 0xC7, 0x84),  // Green
            fork: Color32::from_rgb(0xFF, 0x80, 0x80),   // Red
            generate: Color32::from_rgb(0x64, 0xB5, 0xF6), // Blue
            struct_: Color32::from_rgb(0x4D, 0xD0, 0xE1), // Cyan
            union: Color32::from_rgb(0x4D, 0xD0, 0xE1),  // Cyan
            class: Color32::from_rgb(0xF0, 0x62, 0x92),  // Pink
            interface: Color32::from_rgb(0xAE, 0xD5, 0x81), // Light Green
            package: Color32::from_rgb(0xFF, 0xD5, 0x4F), // Yellow
            program: Color32::from_rgb(0xA1, 0x88, 0x7F), // Brown
            vhdl_architecture: Color32::from_rgb(0x4F, 0xC3, 0xF7), // Light Blue (like module)
            vhdl_procedure: Color32::from_rgb(0xFF, 0xB7, 0x4D), // Orange (like task)
            vhdl_function: Color32::from_rgb(0xBA, 0x68, 0xC8), // Purple (like function)
            vhdl_record: Color32::from_rgb(0x4D, 0xD0, 0xE1), // Cyan (like struct)
            vhdl_process: Color32::from_rgb(0x81, 0xC7, 0x84), // Green (like begin)
            vhdl_block: Color32::from_rgb(0x90, 0xA4, 0xAE), // Blue Grey
            vhdl_for_generate: Color32::from_rgb(0x64, 0xB5, 0xF6), // Blue (like generate)
            vhdl_if_generate: Color32::from_rgb(0x64, 0xB5, 0xF6), // Blue (like generate)
            vhdl_generate: Color32::from_rgb(0x64, 0xB5, 0xF6), // Blue (like generate)
            vhdl_package: Color32::from_rgb(0xFF, 0xD5, 0x4F), // Yellow (like package)
            ghw_generic: Color32::from_rgb(0xB0, 0xBE, 0xC5), // Blue Grey Light
            vhdl_array: Color32::from_rgb(0xCE, 0x93, 0xD8), // Light Purple
            unknown: Color32::from_rgb(0x9E, 0x9E, 0x9E), // Grey
        }
    }
}

/// Icons for different scope types in the hierarchy view.
/// Each field maps to a wellen::ScopeType and contains a Remix icon string.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ScopeIcons {
    // Verilog/SystemVerilog scope types
    pub module: String,
    pub task: String,
    pub function: String,
    pub begin: String,
    pub fork: String,
    pub generate: String,
    #[serde(rename = "struct")]
    pub struct_: String,
    pub union: String,
    pub class: String,
    pub interface: String,
    pub package: String,
    pub program: String,
    // VHDL scope types
    pub vhdl_architecture: String,
    pub vhdl_procedure: String,
    pub vhdl_function: String,
    pub vhdl_record: String,
    pub vhdl_process: String,
    pub vhdl_block: String,
    pub vhdl_for_generate: String,
    pub vhdl_if_generate: String,
    pub vhdl_generate: String,
    pub vhdl_package: String,
    pub ghw_generic: String,
    pub vhdl_array: String,
    pub unknown: String,
    /// Colors for scope icons
    #[serde(default)]
    pub colors: ScopeIconColors,
}

impl Default for ScopeIcons {
    fn default() -> Self {
        use egui_remixicon::icons;
        Self {
            // Verilog/SystemVerilog scope types
            module: icons::CPU_LINE.to_string(),
            task: icons::TASK_LINE.to_string(),
            function: icons::BRACES_LINE.to_string(),
            begin: icons::CODE_BOX_LINE.to_string(),
            fork: icons::GIT_BRANCH_LINE.to_string(),
            generate: icons::REPEAT_LINE.to_string(),
            struct_: icons::TABLE_LINE.to_string(),
            union: icons::MERGE_CELLS_HORIZONTAL.to_string(),
            class: icons::TABLE_LINE.to_string(),
            interface: icons::PLUG_LINE.to_string(),
            package: icons::BOX_3_LINE.to_string(),
            program: icons::FILE_CODE_LINE.to_string(),
            // VHDL scope types
            vhdl_architecture: icons::CPU_LINE.to_string(),
            vhdl_procedure: icons::TERMINAL_LINE.to_string(),
            vhdl_function: icons::BRACES_LINE.to_string(),
            vhdl_record: icons::TABLE_LINE.to_string(),
            vhdl_process: icons::FLASHLIGHT_LINE.to_string(),
            vhdl_block: icons::CODE_BLOCK.to_string(),
            vhdl_for_generate: icons::REPEAT_LINE.to_string(),
            vhdl_if_generate: icons::QUESTION_LINE.to_string(),
            vhdl_generate: icons::REPEAT_LINE.to_string(),
            vhdl_package: icons::BOX_3_LINE.to_string(),
            ghw_generic: icons::SETTINGS_3_LINE.to_string(),
            vhdl_array: icons::BRACKETS_LINE.to_string(),
            unknown: icons::QUESTION_LINE.to_string(),
            colors: ScopeIconColors::default(),
        }
    }
}

impl ScopeIcons {
    /// Returns the icon and color for a given scope type.
    /// If `scope_type` is `None`, returns the default module icon and color.
    #[must_use]
    pub fn get_icon(&self, scope_type: Option<wellen::ScopeType>) -> (&str, Color32) {
        use wellen::ScopeType;
        match scope_type {
            None => (&self.module, self.colors.module),
            Some(st) => match st {
                ScopeType::Module => (&self.module, self.colors.module),
                ScopeType::Task => (&self.task, self.colors.task),
                ScopeType::Function => (&self.function, self.colors.function),
                ScopeType::Begin => (&self.begin, self.colors.begin),
                ScopeType::Fork => (&self.fork, self.colors.fork),
                ScopeType::Generate => (&self.generate, self.colors.generate),
                ScopeType::Struct => (&self.struct_, self.colors.struct_),
                ScopeType::Union => (&self.union, self.colors.union),
                ScopeType::Class => (&self.class, self.colors.class),
                ScopeType::Interface => (&self.interface, self.colors.interface),
                ScopeType::Package => (&self.package, self.colors.package),
                ScopeType::Program => (&self.program, self.colors.program),
                ScopeType::VhdlArchitecture => {
                    (&self.vhdl_architecture, self.colors.vhdl_architecture)
                }
                ScopeType::VhdlProcedure => (&self.vhdl_procedure, self.colors.vhdl_procedure),
                ScopeType::VhdlFunction => (&self.vhdl_function, self.colors.vhdl_function),
                ScopeType::VhdlRecord => (&self.vhdl_record, self.colors.vhdl_record),
                ScopeType::VhdlProcess => (&self.vhdl_process, self.colors.vhdl_process),
                ScopeType::VhdlBlock => (&self.vhdl_block, self.colors.vhdl_block),
                ScopeType::VhdlForGenerate => {
                    (&self.vhdl_for_generate, self.colors.vhdl_for_generate)
                }
                ScopeType::VhdlIfGenerate => (&self.vhdl_if_generate, self.colors.vhdl_if_generate),
                ScopeType::VhdlGenerate => (&self.vhdl_generate, self.colors.vhdl_generate),
                ScopeType::VhdlPackage => (&self.vhdl_package, self.colors.vhdl_package),
                ScopeType::GhwGeneric => (&self.ghw_generic, self.colors.ghw_generic),
                ScopeType::VhdlArray => (&self.vhdl_array, self.colors.vhdl_array),
                ScopeType::Unknown => (&self.unknown, self.colors.unknown),
                _ => (&self.unknown, self.colors.unknown),
            },
        }
    }
}

/// Colors for different variable type icons in the hierarchy view.
/// Each field contains a Color32 value for the corresponding variable type.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct VariableIconColors {
    /// Color for 1-bit wire signals
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub wire: Color32,
    /// Color for multi-bit bus signals
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub bus: Color32,
    /// Color for string variables
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub string: Color32,
    /// Color for event variables
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub event: Color32,
    /// Color for other types (integers, floats, enums)
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub other: Color32,
}

impl Default for VariableIconColors {
    fn default() -> Self {
        Self {
            wire: Color32::from_rgb(0x81, 0xC7, 0x84),   // Green
            bus: Color32::from_rgb(0x64, 0xB5, 0xF6),    // Blue
            string: Color32::from_rgb(0xFF, 0xB7, 0x4D), // Orange
            event: Color32::from_rgb(0xF0, 0x62, 0x92),  // Pink
            other: Color32::from_rgb(0xBA, 0x68, 0xC8),  // Purple
        }
    }
}

/// Icons for different variable types in the hierarchy view.
/// Each field contains a Remix icon string.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct VariableIcons {
    /// 1-bit wire signals
    pub wire: String,
    /// Multi-bit bus signals
    pub bus: String,
    /// String variables
    pub string: String,
    /// Event variables
    pub event: String,
    /// Other types (integers, floats, enums)
    pub other: String,
    /// Colors for variable icons
    #[serde(default)]
    pub colors: VariableIconColors,
}

impl Default for VariableIcons {
    fn default() -> Self {
        use egui_remixicon::icons;
        Self {
            wire: icons::GIT_COMMIT_LINE.to_string(),
            bus: icons::BRACKETS_LINE.to_string(),
            string: icons::TEXT.to_string(),
            event: icons::ARROW_UP_LONG_LINE.to_string(),
            other: icons::NUMBERS_LINE.to_string(),
            colors: VariableIconColors::default(),
        }
    }
}

impl VariableIcons {
    /// Returns the icon and color for a given variable meta.
    /// If `meta` is `None`, returns the default "other" icon and color.
    #[must_use]
    pub fn get_icon(&self, meta: Option<&VariableMeta>) -> (&str, Color32) {
        let Some(meta) = meta else {
            return (&self.other, self.colors.other);
        };

        match meta.encoding {
            VariableEncoding::String => (&self.string, self.colors.string),
            VariableEncoding::Event => (&self.event, self.colors.event),
            VariableEncoding::Real => (&self.other, self.colors.other),
            VariableEncoding::BitVector => match meta.num_bits {
                Some(1) => (&self.wire, self.colors.wire),
                Some(n) if n > 1 => (&self.bus, self.colors.bus),
                _ => (&self.other, self.colors.other),
            },
        }
    }
}

fn get_luminance(color: Color32) -> f32 {
    let rg = if color.r() < 10 {
        f32::from(color.r()) / 3294.0
    } else {
        (f32::from(color.r()) / 269.0 + 0.0513).powf(2.4)
    };
    let gg = if color.g() < 10 {
        f32::from(color.g()) / 3294.0
    } else {
        (f32::from(color.g()) / 269.0 + 0.0513).powf(2.4)
    };
    let bg = if color.b() < 10 {
        f32::from(color.b()) / 3294.0
    } else {
        (f32::from(color.b()) / 269.0 + 0.0513).powf(2.4)
    };
    0.2126 * rg + 0.7152 * gg + 0.0722 * bg
}

impl SurferTheme {
    #[must_use]
    pub fn get_color(&self, color: &str) -> Option<Color32> {
        self.colors.get(color).copied()
    }

    #[must_use]
    pub fn get_best_text_color(&self, backgroundcolor: Color32) -> Color32 {
        // Based on https://ux.stackexchange.com/questions/82056/how-to-measure-the-contrast-between-any-given-color-and-white

        // Compute luminance
        let l_foreground = get_luminance(self.foreground);
        let l_alt_text_color = get_luminance(self.alt_text_color);
        let l_background = get_luminance(backgroundcolor);

        // Compute contrast ratio
        let mut cr_foreground = (l_foreground + 0.05) / (l_background + 0.05);
        cr_foreground = cr_foreground.max(1. / cr_foreground);
        let mut cr_alt_text_color = (l_alt_text_color + 0.05) / (l_background + 0.05);
        cr_alt_text_color = cr_alt_text_color.max(1. / cr_alt_text_color);

        // Return color with highest contrast
        if cr_foreground > cr_alt_text_color {
            self.foreground
        } else {
            self.alt_text_color
        }
    }

    fn generate_defaults(
        theme_name: Option<&String>,
    ) -> (ConfigBuilder<DefaultState>, Vec<String>) {
        let default_theme = String::from(include_str!("../../default_theme.toml"));

        let mut theme = Config::builder().add_source(config::File::from_str(
            &default_theme,
            config::FileFormat::Toml,
        ));

        let theme_names = all_theme_names();

        let override_theme = theme_name
            .as_ref()
            .and_then(|name| BUILTIN_THEMES.get(name.as_str()).copied())
            .unwrap_or("");

        theme = theme.add_source(config::File::from_str(
            override_theme,
            config::FileFormat::Toml,
        ));
        (theme, theme_names)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(theme_name: Option<String>) -> Result<Self> {
        use eyre::anyhow;

        let (theme, _) = Self::generate_defaults(theme_name.as_ref());

        let theme = theme.set_override("theme_names", all_theme_names())?;

        theme
            .build()?
            .try_deserialize()
            .map_err(|e| anyhow!("Failed to parse config {e}"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(theme_name: Option<String>) -> eyre::Result<Self> {
        use std::fs::ReadDir;

        use eyre::anyhow;

        let (mut theme, mut theme_names) = Self::generate_defaults(theme_name.as_ref());

        let mut add_themes_from_dir = |dir: ReadDir| {
            for theme in dir.flatten() {
                if let Ok(theme_path) = theme.file_name().into_string()
                    && let Some(fname_str) = theme_path.strip_suffix(".toml")
                {
                    let fname = fname_str.to_string();
                    if !fname.is_empty() && !theme_names.contains(&fname) {
                        theme_names.push(fname);
                    }
                }
            }
        };

        // read themes from config directory
        if let Some(proj_dirs) = &*PROJECT_DIR {
            let config_themes_dir = proj_dirs.config_dir().join(THEMES_DIR);
            if let Ok(config_themes_dir) = std::fs::read_dir(config_themes_dir) {
                add_themes_from_dir(config_themes_dir);
            }
        }

        // Read themes from local directories.
        let local_config_dirs = find_local_configs();

        // Add any existing themes from most top-level to most local. This allows overwriting of
        // higher-level theme settings with a local `.surfer` directory.
        local_config_dirs
            .iter()
            .filter_map(|p| std::fs::read_dir(p.join(THEMES_DIR)).ok())
            .for_each(add_themes_from_dir);

        if matches!(theme_name, Some(ref name) if !name.is_empty()) {
            let theme_path =
                Path::new(THEMES_DIR).join(theme_name.as_ref().unwrap().to_owned() + ".toml");

            // First filter out all the existing local themes and add them in the aforementioned
            // order.
            let local_themes: Vec<PathBuf> = local_config_dirs
                .iter()
                .map(|p| p.join(&theme_path))
                .filter(|p| p.exists())
                .collect();
            if local_themes.is_empty() {
                // If no local themes exist, search in the config directory.
                if let Some(proj_dirs) = &*PROJECT_DIR {
                    let config_theme_path = proj_dirs.config_dir().join(theme_path);
                    if config_theme_path.exists() {
                        theme = theme.add_source(File::from(config_theme_path).required(false));
                    }
                }
            } else {
                theme = local_themes
                    .into_iter()
                    .fold(theme, |t, p| t.add_source(File::from(p).required(false)));
            }
        }

        let theme = theme.set_override("theme_names", theme_names)?;

        theme
            .build()?
            .try_deserialize()
            .map_err(|e| anyhow!("Failed to parse theme {e}"))
    }
}

#[derive(Debug, Deserialize)]
pub struct ThemeColorPair {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub background: Color32,
}

#[derive(Debug, Deserialize)]
pub struct ThemeColorTriple {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub background: Color32,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub alt_background: Color32,
}

#[derive(Debug, Deserialize)]
pub struct WcpConfig {
    /// Controls if a server is started after Surfer is launched
    pub autostart: bool,
    /// Address to bind to (address:port)
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// IP address to bind the HTTP server to
    pub bind_address: String,
    /// Default port for the HTTP server
    pub port: u16,
}

fn default_colors() -> HashMap<String, Color32> {
    [
        ("Green", "a7e47e"),
        ("Red", "c52e2e"),
        ("Yellow", "f3d54a"),
        ("Blue", "81a2be"),
        ("Purple", "b294bb"),
        ("Aqua", "8abeb7"),
        ("Gray", "c5c8c6"),
    ]
    .iter()
    .map(|(name, hexcode)| {
        (
            (*name).to_string(),
            hex_string_to_color32((*hexcode).to_string()).unwrap(),
        )
    })
    .collect()
}

impl SurferConfig {
    #[cfg(target_arch = "wasm32")]
    pub fn new(_force_default_config: bool) -> Result<Self> {
        Self::new_from_toml(&include_str!("../../default_config.toml"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(force_default_config: bool) -> eyre::Result<Self> {
        use eyre::anyhow;
        use tracing::warn;

        let default_config = String::from(include_str!("../../default_config.toml"));

        let mut config = Config::builder().add_source(config::File::from_str(
            &default_config,
            config::FileFormat::Toml,
        ));

        let config = if force_default_config {
            config
        } else {
            if let Some(proj_dirs) = &*PROJECT_DIR {
                let config_file = proj_dirs.config_dir().join(CONFIG_FILE);
                config = config.add_source(File::from(config_file).required(false));
            }

            let old_config_path = Path::new(OLD_CONFIG_FILE);
            if old_config_path.exists() {
                warn!(
                    "Configuration in 'surfer.toml' is deprecated. Please move your configuration to '.surfer/config.toml'."
                );
            }

            // `surfer.toml` will not be searched for upward, as it is deprecated.
            config = config.add_source(File::from(old_config_path).required(false));

            // Add configs from most top-level to most local. This allows overwriting of
            // higher-level settings with a local `.surfer` directory.
            find_local_configs()
                .into_iter()
                .fold(config, |c, p| {
                    c.add_source(File::from(p.join(CONFIG_FILE)).required(false))
                })
                .add_source(Environment::with_prefix("surfer")) // Add environment finally
        };

        config
            .build()?
            .try_deserialize()
            .map_err(|e| anyhow!("Failed to parse config {e}"))
    }

    pub fn new_from_toml(config: &str) -> Result<Self> {
        Ok(toml::from_str(config)?)
    }
}

impl Default for SurferConfig {
    fn default() -> Self {
        Self::new(false).expect("Failed to load default config")
    }
}

fn hex_string_to_color32(str: String) -> Result<Color32> {
    let str = if str.len() == 3 {
        str.chars().flat_map(|c| [c, c]).collect()
    } else {
        str
    };
    if str.len() == 6 {
        let r = u8::from_str_radix(&str[0..2], 16)
            .with_context(|| format!("'{str}' is not a valid RGB hex color"))?;
        let g = u8::from_str_radix(&str[2..4], 16)
            .with_context(|| format!("'{str}' is not a valid RGB hex color"))?;
        let b = u8::from_str_radix(&str[4..6], 16)
            .with_context(|| format!("'{str}' is not a valid RGB hex color"))?;
        Ok(Color32::from_rgb(r, g, b))
    } else {
        Result::Err(Report::msg(format!("'{str}' is not a valid RGB hex color")))
    }
}

fn all_theme_names() -> Vec<String> {
    BUILTIN_THEMES.keys().map(ToString::to_string).collect()
}

fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<Color32, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    hex_string_to_color32(buf).map_err(de::Error::custom)
}

fn deserialize_color_map<'de, D>(deserializer: D) -> Result<HashMap<String, Color32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Wrapper(#[serde(deserialize_with = "deserialize_hex_color")] Color32);

    let v = HashMap::<String, Wrapper>::deserialize(deserializer)?;
    Ok(v.into_iter().map(|(k, Wrapper(v))| (k, v)).collect())
}

fn deserialize_theme<'de, D>(deserializer: D) -> Result<SurferTheme, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    SurferTheme::new(Some(buf)).map_err(de::Error::custom)
}

/// Searches for `.surfer` directories upward from the current location until it reaches root.
/// Returns an empty vector in case the search fails in any way. If any `.surfer` directories
/// are found, they will be returned in a `Vec<PathBuf>` in a pre-order of most top-level to most
/// local. All plain files are ignored.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_local_configs() -> Vec<PathBuf> {
    use crate::util::search_upward;
    match std::env::current_dir() {
        Ok(dir) => search_upward(dir, "/", LOCAL_DIR)
            .into_iter()
            .filter(|p| p.is_dir()) // Only keep directories and ignore plain files.
            .rev() // Reverse for pre-order traversal of directories.
            .collect(),
        Err(_) => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_string_3_chars() {
        // Test that 3-character hex strings are doubled correctly
        let result = hex_string_to_color32("abc".to_string()).unwrap();
        let expected = Color32::from_rgb(0xaa, 0xbb, 0xcc);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_hex_string_6_chars() {
        // Test standard 6-character hex string
        let result = hex_string_to_color32("a7e47e".to_string()).unwrap();
        let expected = Color32::from_rgb(0xa7, 0xe4, 0x7e);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_hex_string_black() {
        // Test black color (all zeros)
        let result = hex_string_to_color32("000000".to_string()).unwrap();
        let expected = Color32::from_rgb(0x00, 0x00, 0x00);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_hex_string_white() {
        // Test white color (all ones)
        let result = hex_string_to_color32("ffffff".to_string()).unwrap();
        let expected = Color32::from_rgb(0xff, 0xff, 0xff);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_hex_string_uppercase() {
        // Test uppercase hex characters
        let result = hex_string_to_color32("ABCDEF".to_string()).unwrap();
        let expected = Color32::from_rgb(0xab, 0xcd, 0xef);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_hex_string_mixed_case() {
        // Test mixed case hex characters
        let result = hex_string_to_color32("Ab5DeF".to_string()).unwrap();
        let expected = Color32::from_rgb(0xab, 0x5d, 0xef);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_hex_string_invalid_length() {
        // Test that invalid length returns error
        let result = hex_string_to_color32("ab".to_string());
        assert!(result.is_err());

        let result = hex_string_to_color32("abcde".to_string());
        assert!(result.is_err());

        let result = hex_string_to_color32("abcdefgh".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_string_invalid_characters() {
        // Test that invalid hex characters return error
        let result = hex_string_to_color32("GGGGGG".to_string());
        assert!(result.is_err());

        let result = hex_string_to_color32("12345g".to_string());
        assert!(result.is_err());

        let result = hex_string_to_color32("zzzzzz".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_string_empty() {
        // Test empty string
        let result = hex_string_to_color32(String::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_string_3_chars_doubling() {
        // Test specific 3-character doubling behavior
        let result = hex_string_to_color32("050".to_string()).unwrap();
        let expected = Color32::from_rgb(0x00, 0x55, 0x00);
        assert_eq!(result, expected);
    }
}

use std::sync::Mutex;

use chrono::prelude::{DateTime, Utc};
use eyre::{Result, bail};
use num::BigUint;
use serde::{Deserialize, Serialize};
use surfer_translation_types::VariableValue;

use crate::cxxrtl_container::CxxrtlContainer;
use crate::time::{TimeScale, TimeUnit};
use crate::wellen::{BodyResult, LoadSignalsCmd, LoadSignalsResult, WellenContainer};

pub type FieldRef = surfer_translation_types::FieldRef<VarId, ScopeId>;
pub type ScopeRef = surfer_translation_types::ScopeRef<ScopeId>;
pub type VariableRef = surfer_translation_types::VariableRef<VarId, ScopeId>;
pub type VariableMeta = surfer_translation_types::VariableMeta<VarId, ScopeId>;

/// Cache key for analog signal data: (`signal_id`, `translator_name`)
pub type AnalogCacheKey = (SignalId, String);

#[derive(Debug, Clone)]
pub enum SimulationStatus {
    Paused,
    Running,
    Finished,
}

pub struct MetaData {
    pub date: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub timescale: TimeScale,
}

/// A backend-specific, numeric reference for fast access to the associated scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScopeId {
    #[default]
    None,
    Wellen(wellen::ScopeRef),
}

/// A backend-specific, numeric reference for fast access to the associated variable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VarId {
    #[default]
    None,
    Wellen(wellen::VarRef),
}

/// A backend-specific, numeric reference for fast access to the associated signal data.
/// Used as cache key for signal data lookups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalId {
    #[default]
    None,
    Wellen(wellen::SignalRef),
}

/// Backend-agnostic enum for accessing signal data.
/// Variants provide iteration over signal changes.
pub enum SignalAccessor {
    Wellen(crate::wellen::WellenSignalAccessor),
    // Future: Cxxrtl(CxxrtlSignalAccessor),
}

impl SignalAccessor {
    /// Iterator over signal changes as (`time_u64`, value) pairs
    #[must_use]
    pub fn iter_changes(&self) -> Box<dyn Iterator<Item = (u64, VariableValue)> + '_> {
        match self {
            SignalAccessor::Wellen(accessor) => accessor.iter_changes(),
        }
    }
}

#[derive(Debug, Default)]
pub struct QueryResult {
    pub current: Option<(BigUint, VariableValue)>,
    pub next: Option<BigUint>,
}

#[local_impl::local_impl]
impl ScopeRefExt for ScopeRef {
    fn empty() -> Self {
        Self {
            strs: vec![],
            id: ScopeId::default(),
        }
    }

    fn from_strs<S: ToString>(s: &[S]) -> Self {
        Self::from_strs_with_id(s, ScopeId::default())
    }

    fn from_strs_with_id(s: &[impl ToString], id: ScopeId) -> Self {
        let strs = s.iter().map(ToString::to_string).collect();
        Self { strs, id }
    }

    /// Creates a `ScopeRef` from a string with each scope separated by `.`
    fn from_hierarchy_string(s: &str) -> Self {
        let strs = s.split('.').map(ToString::to_string).collect();
        let id = ScopeId::default();
        Self { strs, id }
    }

    fn with_subscope(&self, subscope: String, id: ScopeId) -> Self {
        let mut result = self.clone();
        result.strs.push(subscope);
        // the result refers to a different scope, which we do not know the ID of
        result.id = id;
        result
    }

    fn name(&self) -> String {
        self.strs.last().cloned().unwrap_or_default()
    }

    fn strs(&self) -> &[String] {
        &self.strs
    }

    fn with_id(&self, id: ScopeId) -> Self {
        let mut out = self.clone();
        out.id = id;
        out
    }

    fn cxxrtl_repr(&self) -> String {
        self.strs.join(" ")
    }

    fn has_empty_strs(&self) -> bool {
        self.strs.is_empty()
    }
}

fn extract_index(s: String) -> (String, Option<i64>) {
    if let Some(start_idx) = s.rfind('[')
        && start_idx > 0
        && s.ends_with(']')
    {
        let index_str = &s[start_idx + 1..s.len() - 1];
        if let Ok(index) = index_str.parse::<i64>() {
            let name = s[..start_idx].to_string();
            return (name, Some(index));
        }
    }
    (s, None)
}

#[local_impl::local_impl]
impl VariableRefExt for VariableRef {
    fn new(path: ScopeRef, name: String, index: Option<i64>) -> Self {
        Self::new_with_id(path, name, VarId::default(), index)
    }

    fn new_with_id(path: ScopeRef, name: String, id: VarId, index: Option<i64>) -> Self {
        let (name, index) = if index.is_none() {
            extract_index(name)
        } else {
            (name, index)
        };
        Self {
            path,
            name,
            id,
            index,
        }
    }

    fn from_hierarchy_string(s: &str) -> Self {
        let components = s.split('.').map(ToString::to_string).collect::<Vec<_>>();

        if components.is_empty() {
            Self {
                path: ScopeRef::empty(),
                name: String::new(),
                id: VarId::default(),
                index: None,
            }
        } else {
            let name = components.last().unwrap().to_string();
            let (name, index) = extract_index(name);
            Self {
                path: ScopeRef::from_strs(&components[..(components.len()) - 1]),
                name,
                id: VarId::default(),
                index,
            }
        }
    }

    fn from_hierarchy_string_with_id(s: &str, id: VarId) -> Self {
        let components = s
            .split('.')
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();

        if components.is_empty() {
            Self {
                path: ScopeRef::empty(),
                name: String::new(),
                id,
                index: None,
            }
        } else {
            Self {
                path: ScopeRef::from_strs(&components[..(components.len()) - 1]),
                name: components.last().unwrap().to_string(),
                id,
                index: None,
            }
        }
    }

    /// A human readable full path to the scope
    fn full_path_string(&self) -> String {
        if self.path.has_empty_strs() {
            self.name.clone()
        } else {
            format!("{}.{}", self.path, self.name)
        }
    }

    fn full_path(&self) -> Vec<String> {
        self.path
            .strs()
            .iter()
            .cloned()
            .chain([self.name.clone()])
            .collect()
    }

    fn from_strs(s: &[&str]) -> Self {
        Self {
            path: ScopeRef::from_strs(&s[..(s.len() - 1)]),
            name: (*s.last().expect("from_strs called with an empty string")).to_string(),
            id: VarId::default(),
            index: None,
        }
    }

    fn clear_id(&mut self) {
        self.id = VarId::default();
    }

    fn cxxrtl_repr(&self) -> String {
        self.full_path().join(" ")
    }
}

#[local_impl::local_impl]
impl FieldRefExt for FieldRef {
    fn without_fields(root: VariableRef) -> Self {
        Self {
            root,
            field: vec![],
        }
    }

    fn from_strs(root: &[&str], field: &[&str]) -> Self {
        Self {
            root: VariableRef::from_strs(root),
            field: field.iter().map(ToString::to_string).collect(),
        }
    }
}

pub enum WaveContainer {
    Wellen(Box<WellenContainer>),
    /// A wave container that contains nothing. Currently, the only practical use for this is
    /// a placehodler when serializing and deserializing wave state.
    Empty,
    Cxxrtl(Box<Mutex<CxxrtlContainer>>),
}

impl WaveContainer {
    #[must_use]
    pub fn new_waveform(hierarchy: std::sync::Arc<wellen::Hierarchy>) -> Self {
        WaveContainer::Wellen(Box::new(WellenContainer::new(hierarchy, None)))
    }

    #[must_use]
    pub fn new_remote_waveform(
        server_url: &str,
        hierarchy: std::sync::Arc<wellen::Hierarchy>,
    ) -> Self {
        WaveContainer::Wellen(Box::new(WellenContainer::new(
            hierarchy,
            Some(server_url.to_string()),
        )))
    }

    /// Creates a new empty wave container. Should only be used as a default for serde. If
    /// no wave container is present, the `WaveData` should be None, rather than this being
    /// Empty
    #[must_use]
    pub fn __new_empty() -> Self {
        WaveContainer::Empty
    }

    // Perform tasks that are done on the main thread each frame
    pub fn tick(&self) {
        match self {
            WaveContainer::Wellen(_) => {}
            WaveContainer::Empty => {}
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().tick(),
        }
    }

    #[must_use]
    pub fn wants_anti_aliasing(&self) -> bool {
        match self {
            WaveContainer::Wellen(_) => true,
            WaveContainer::Empty => true,
            // FIXME: Once we do AA on the server side, we can set this to false
            WaveContainer::Cxxrtl(_) => true,
        }
    }

    /// Returns true if all requested signals have been loaded.
    /// Used for testing to make sure the GUI is at its final state before taking a
    /// snapshot.
    #[must_use]
    pub fn is_fully_loaded(&self) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.is_fully_loaded(),
            WaveContainer::Empty => true,
            WaveContainer::Cxxrtl(_) => true,
        }
    }

    /// Returns the full names of all variables in the design.
    #[must_use]
    pub fn variable_names(&self) -> Vec<String> {
        match self {
            WaveContainer::Wellen(f) => f.variable_names(),
            WaveContainer::Empty => vec![],
            // I don't know if we can do
            WaveContainer::Cxxrtl(_) => vec![], // FIXME: List variable names
        }
    }

    /// Return all variables (excluding parameters) in the whole design.
    #[must_use]
    pub fn variables(&self) -> Vec<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.variables(),
            WaveContainer::Empty => vec![],
            WaveContainer::Cxxrtl(_) => vec![],
        }
    }

    /// Return all variables (excluding parameters) in a scope.
    #[must_use]
    pub fn variables_in_scope(&self, scope: &ScopeRef) -> Vec<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.variables_in_scope(scope),
            WaveContainer::Empty => vec![],
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().variables_in_module(scope),
        }
    }

    /// Return all parameters in a scope.
    #[must_use]
    pub fn parameters_in_scope(&self, scope: &ScopeRef) -> Vec<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.parameters_in_scope(scope),
            WaveContainer::Empty => vec![],
            // No parameters in Cxxrtl
            WaveContainer::Cxxrtl(_) => vec![],
        }
    }

    /// Return true if there are no variables or parameters in the scope.
    #[must_use]
    pub fn no_variables_in_scope(&self, scope: &ScopeRef) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.no_variables_in_scope(scope),
            WaveContainer::Empty => true,
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().no_variables_in_module(scope),
        }
    }

    /// Loads multiple variables at once. This is useful when we want to add multiple variables in one go.
    pub fn load_variables<S: AsRef<VariableRef>, T: Iterator<Item = S>>(
        &mut self,
        variables: T,
    ) -> Result<Option<LoadSignalsCmd>> {
        match self {
            WaveContainer::Wellen(f) => f.load_variables(variables),
            WaveContainer::Empty => bail!("Cannot load variables from empty container."),
            WaveContainer::Cxxrtl(c) => {
                c.get_mut().unwrap().load_variables(variables);
                Ok(None)
            }
        }
    }
    /// Load all the parameters in the design so that the value can be displayed.
    pub fn load_parameters(&mut self) -> Result<Option<LoadSignalsCmd>> {
        match self {
            WaveContainer::Wellen(f) => f.load_all_params(),
            WaveContainer::Empty => bail!("Cannot load parameters from empty container."),
            WaveContainer::Cxxrtl(_) => {
                // Cxxrtl does not deal with parameters
                Ok(None)
            }
        }
    }

    /// Callback for when wellen signals have been loaded. Might lead to a new load variable
    /// command since new variables might have been requested in the meantime.
    pub fn on_signals_loaded(&mut self, res: LoadSignalsResult) -> Result<Option<LoadSignalsCmd>> {
        match self {
            WaveContainer::Wellen(f) => f.on_signals_loaded(res),
            WaveContainer::Empty => {
                bail!("on_load_signals should only be called with the wellen backend.")
            }
            WaveContainer::Cxxrtl(_) => {
                bail!("on_load_signals should only be called with the wellen backend.")
            }
        }
    }

    pub fn variable_meta<'a>(&'a self, variable: &'a VariableRef) -> Result<VariableMeta> {
        match self {
            WaveContainer::Wellen(f) => f.variable_to_meta(variable),
            WaveContainer::Empty => bail!("Getting meta from empty wave container"),
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().variable_meta(variable),
        }
    }

    /// Query the value of the variable at a certain time step.
    /// Returns `None` if we do not have any values for the variable.
    /// That generally happens if the corresponding variable is still being loaded.
    pub fn query_variable(
        &self,
        variable: &VariableRef,
        time: &BigUint,
    ) -> Result<Option<QueryResult>> {
        match self {
            WaveContainer::Wellen(f) => f.query_variable(variable, time),
            WaveContainer::Empty => bail!("Querying variable from empty wave container"),
            WaveContainer::Cxxrtl(c) => Ok(c.lock().unwrap().query_variable(variable, time)),
        }
    }

    pub fn signal_accessor(&self, signal_id: SignalId) -> Result<SignalAccessor> {
        match (self, signal_id) {
            (WaveContainer::Wellen(f), SignalId::Wellen(signal_ref)) => {
                Ok(SignalAccessor::Wellen(f.signal_accessor(signal_ref)?))
            }
            _ => bail!("Invalid signal accessor combination"),
        }
    }
    /// Get the `SignalId` for a variable (canonical signal identity for cache keys)
    pub fn signal_id(&self, variable: &VariableRef) -> Result<SignalId> {
        match self {
            WaveContainer::Wellen(f) => Ok(SignalId::Wellen(f.signal_ref(variable)?)),
            WaveContainer::Empty => bail!("No signal data"),
            WaveContainer::Cxxrtl(_) => bail!("Not supported for Cxxrtl yet"),
        }
    }

    /// Check if a signal is already loaded (data available)
    #[must_use]
    pub fn is_signal_loaded(&self, signal_id: &SignalId) -> bool {
        match (self, signal_id) {
            (WaveContainer::Wellen(f), SignalId::Wellen(signal_ref)) => {
                f.is_signal_loaded(*signal_ref)
            }
            _ => false,
        }
    }

    /// Looks up the variable _by name_ and returns a new reference with an updated `id` if the variable is found.
    #[must_use]
    pub fn update_variable_ref(&self, variable: &VariableRef) -> Option<VariableRef> {
        match self {
            WaveContainer::Wellen(f) => f.update_variable_ref(variable),
            WaveContainer::Empty => None,
            WaveContainer::Cxxrtl(_) => None,
        }
    }

    /// Returns the full names of all scopes in the design.
    #[must_use]
    pub fn scope_names(&self) -> Vec<String> {
        match self {
            WaveContainer::Wellen(f) => f.scope_names(),
            WaveContainer::Empty => vec![],
            WaveContainer::Cxxrtl(c) => c
                .lock()
                .unwrap()
                .modules()
                .iter()
                .map(|m| m.strs().last().cloned().unwrap_or("root".to_string()))
                .collect(),
        }
    }

    #[must_use]
    pub fn metadata(&self) -> MetaData {
        match self {
            WaveContainer::Wellen(f) => f.metadata(),
            WaveContainer::Empty => MetaData {
                date: None,
                version: None,
                timescale: TimeScale {
                    unit: TimeUnit::None,
                    multiplier: None,
                },
            },
            WaveContainer::Cxxrtl(_) => {
                MetaData {
                    date: None,
                    version: None,
                    timescale: TimeScale {
                        // Cxxrtl always uses FemtoSeconds
                        unit: TimeUnit::FemtoSeconds,
                        multiplier: None,
                    },
                }
            }
        }
    }

    #[must_use]
    pub fn root_scopes(&self) -> Vec<ScopeRef> {
        match self {
            WaveContainer::Wellen(f) => f.root_scopes(),
            WaveContainer::Empty => vec![],
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().root_modules(),
        }
    }

    pub fn child_scopes(&self, scope: &ScopeRef) -> Result<Vec<ScopeRef>> {
        match self {
            WaveContainer::Wellen(f) => f.child_scopes(scope),
            WaveContainer::Empty => bail!("Getting child modules from empty wave container"),
            WaveContainer::Cxxrtl(c) => Ok(c.lock().unwrap().child_scopes(scope)),
        }
    }

    #[must_use]
    pub fn max_timestamp(&self) -> Option<BigUint> {
        match self {
            WaveContainer::Wellen(f) => f.max_timestamp(),
            WaveContainer::Empty => None,
            WaveContainer::Cxxrtl(c) => c
                .lock()
                .unwrap()
                .max_displayed_timestamp()
                .map(|t| t.as_femtoseconds()),
        }
    }

    #[must_use]
    pub fn scope_exists(&self, scope: &ScopeRef) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.scope_exists(scope),
            WaveContainer::Empty => false,
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().module_exists(scope),
        }
    }

    #[must_use]
    /// True if scope is a compound variable
    pub fn scope_is_variable(&self, scope: &ScopeRef) -> bool {
        match self {
            WaveContainer::Wellen(f) => f.scope_is_variable(scope),
            WaveContainer::Empty => false,
            WaveContainer::Cxxrtl(_) => false, // TODO: Check if scope is variable
        }
    }

    /// Returns a human readable string with information about a scope.
    /// The scope name itself should not be included, since it will be prepended automatically.
    #[must_use]
    pub fn get_scope_tooltip_data(&self, scope: &ScopeRef) -> String {
        match self {
            WaveContainer::Wellen(f) => f.get_scope_tooltip_data(scope),
            WaveContainer::Empty => String::new(),
            // FIXME: Tooltip
            WaveContainer::Cxxrtl(_) => String::new(),
        }
    }

    /// Returns the scope type for a given scope.
    /// Returns `None` for backends that don't support scope types
    #[must_use]
    pub fn get_scope_type(&self, scope: &ScopeRef) -> Option<wellen::ScopeType> {
        match self {
            WaveContainer::Wellen(f) => f.get_scope_type(scope),
            WaveContainer::Empty | WaveContainer::Cxxrtl(_) => None,
        }
    }

    /// Returns the simulation status for this wave source if it exists. Wave sources which have no
    /// simulation status should return None here, otherwise buttons for controlling simulation
    /// will be shown
    #[must_use]
    pub fn simulation_status(&self) -> Option<SimulationStatus> {
        match self {
            WaveContainer::Wellen(_) => None,
            WaveContainer::Empty => None,
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().simulation_status(),
        }
    }

    /// If [`WaveContainer::simulation_status`] is `Some(SimulationStatus::Paused)`, attempt to unpause the
    /// simulation otherwise does nothing
    pub fn unpause_simulation(&self) {
        match self {
            WaveContainer::Wellen(_) => {}
            WaveContainer::Empty => {}
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().unpause(),
        }
    }

    /// See [`WaveContainer::unpause_simulation`]
    pub fn pause_simulation(&self) {
        match self {
            WaveContainer::Wellen(_) => {}
            WaveContainer::Empty => {}
            WaveContainer::Cxxrtl(c) => c.lock().unwrap().pause(),
        }
    }

    /// Called for `wellen` container, when the body of the waveform file has been parsed.
    pub fn wellen_add_body(&mut self, body: BodyResult) -> Result<Option<LoadSignalsCmd>> {
        match self {
            WaveContainer::Wellen(inner) => inner.add_body(body),
            _ => {
                bail!("Should never call this function on a non wellen container!")
            }
        }
    }

    #[must_use]
    pub fn body_loaded(&self) -> bool {
        match self {
            WaveContainer::Wellen(inner) => inner.body_loaded(),
            WaveContainer::Empty => true,
            WaveContainer::Cxxrtl(_) => true,
        }
    }

    /// Returns true if this wave container supports analog rendering options in the GUI.
    /// Currently only the wellen backend (VCD/FST/GHW) supports analog rendering.
    #[must_use]
    pub fn supports_analog(&self) -> bool {
        matches!(self, WaveContainer::Wellen(_))
    }
}

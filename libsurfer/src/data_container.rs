use crate::time::{TimeScale, TimeUnit};
use crate::transaction_container::{StreamScopeRef, TransactionContainer, TransactionStreamRef};
use crate::wave_container::{MetaData, SimulationStatus, VariableRef, WaveContainer};
use crate::wave_data::ScopeType;
use crate::wave_data::ScopeType::{StreamScope, WaveScope};
use num::BigUint;

#[allow(clippy::large_enum_variant)]
pub enum DataContainer {
    Waves(WaveContainer),
    Transactions(TransactionContainer),
    Empty,
}

#[derive(Debug, Clone)]
pub enum VariableType {
    Variable(VariableRef),
    Generator(TransactionStreamRef),
}

impl VariableType {
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            VariableType::Variable(v) => v.name.clone(),
            VariableType::Generator(g) => g.name.clone(),
        }
    }

    #[must_use]
    pub fn name_with_index(&self) -> String {
        match self {
            VariableType::Variable(v) => {
                if let Some(index) = v.index {
                    format!("{}[{}]", v.name, index)
                } else {
                    v.name.clone()
                }
            }
            VariableType::Generator(g) => g.name.clone(),
        }
    }
}

impl DataContainer {
    #[must_use]
    pub fn __new_empty() -> Self {
        DataContainer::Empty
    }

    #[must_use]
    pub fn as_waves(&self) -> Option<&WaveContainer> {
        match self {
            DataContainer::Waves(w) => Some(w),
            DataContainer::Transactions(_) => None,
            DataContainer::Empty => None,
        }
    }

    pub fn as_waves_mut(&mut self) -> Option<&mut WaveContainer> {
        match self {
            DataContainer::Waves(w) => Some(w),
            DataContainer::Transactions(_) => None,
            DataContainer::Empty => None,
        }
    }

    #[must_use]
    pub fn as_transactions(&self) -> Option<&TransactionContainer> {
        match self {
            DataContainer::Waves(_) => None,
            DataContainer::Transactions(t) => Some(t),
            DataContainer::Empty => None,
        }
    }

    pub fn as_transactions_mut(&mut self) -> Option<&mut TransactionContainer> {
        match self {
            DataContainer::Waves(_) => None,
            DataContainer::Transactions(t) => Some(t),
            DataContainer::Empty => None,
        }
    }

    #[must_use]
    pub fn is_waves(&self) -> bool {
        match self {
            DataContainer::Waves(_) => true,
            DataContainer::Transactions(_) => false,
            DataContainer::Empty => false,
        }
    }

    #[must_use]
    pub fn is_transactions(&self) -> bool {
        match self {
            DataContainer::Waves(_) => false,
            DataContainer::Transactions(_) => true,
            DataContainer::Empty => false,
        }
    }

    #[must_use]
    pub fn max_timestamp(&self) -> Option<BigUint> {
        match self {
            DataContainer::Waves(w) => w.max_timestamp(),
            DataContainer::Transactions(t) => t.max_timestamp(),
            DataContainer::Empty => None,
        }
    }

    #[must_use]
    pub fn root_scopes(&self) -> Vec<ScopeType> {
        match self {
            DataContainer::Waves(w) => {
                let scopes = w.root_scopes();
                scopes
                    .iter()
                    .map(|s| ScopeType::WaveScope(s.clone()))
                    .collect()
            }
            DataContainer::Transactions(_) => {
                vec![ScopeType::StreamScope(StreamScopeRef::Root)]
            }
            DataContainer::Empty => vec![],
        }
    }

    #[must_use]
    pub fn scope_exists(&self, scope: &ScopeType) -> bool {
        match (self, scope) {
            (DataContainer::Waves(waves), WaveScope(scope)) => waves.scope_exists(scope),
            (DataContainer::Transactions(transactions), StreamScope(scope)) => {
                transactions.stream_scope_exists(scope)
            }
            (_, _) => false,
        }
    }

    #[must_use]
    pub fn scope_names(&self) -> Vec<String> {
        match self {
            DataContainer::Waves(w) => w.scope_names(),
            DataContainer::Transactions(t) => t.stream_names(),
            DataContainer::Empty => vec![],
        }
    }

    #[must_use]
    pub fn variable_names(&self) -> Vec<String> {
        match self {
            DataContainer::Waves(w) => w.variable_names(),
            DataContainer::Transactions(t) => t.generator_names(),
            DataContainer::Empty => vec![],
        }
    }

    #[must_use]
    pub fn variables_in_scope(&self, scope: &ScopeType) -> Vec<VariableType> {
        match (self, scope) {
            (DataContainer::Waves(w), WaveScope(s)) => {
                let variables = w.variables_in_scope(s);
                variables
                    .iter()
                    .map(|v| VariableType::Variable(v.clone()))
                    .collect()
            }
            (DataContainer::Transactions(t), StreamScope(s)) => {
                let variables = t.generators_in_stream(s);
                variables
                    .iter()
                    .map(|g| VariableType::Generator(g.clone()))
                    .collect()
            }
            _ => panic!("Container and Scope are of incompatible types"),
        }
    }

    #[must_use]
    pub fn metadata(&self) -> MetaData {
        match self {
            DataContainer::Waves(w) => w.metadata(),
            DataContainer::Transactions(t) => t.metadata(),
            DataContainer::Empty => MetaData {
                date: None,
                version: None,
                timescale: TimeScale {
                    unit: TimeUnit::None,
                    multiplier: None,
                },
            },
        }
    }

    #[must_use]
    pub fn body_loaded(&self) -> bool {
        match self {
            DataContainer::Waves(w) => w.body_loaded(),
            DataContainer::Transactions(t) => t.body_loaded(),
            DataContainer::Empty => true,
        }
    }

    #[must_use]
    pub fn is_fully_loaded(&self) -> bool {
        match self {
            DataContainer::Waves(w) => w.is_fully_loaded(),
            DataContainer::Transactions(t) => t.is_fully_loaded(),
            DataContainer::Empty => true,
        }
    }

    #[must_use]
    pub fn simulation_status(&self) -> Option<SimulationStatus> {
        match self {
            DataContainer::Waves(w) => w.simulation_status(),
            DataContainer::Transactions(_) => None,
            DataContainer::Empty => None,
        }
    }
}

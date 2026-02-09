use derive_more::{Display, FromStr};
use enum_iterator::Sequence;
use itertools::Itertools;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::displayed_item::DisplayedItemRef;
use crate::displayed_item_tree::{Node, VisibleItemIndex};
use crate::message::MessageTarget;
use crate::wave_container::{ScopeRefExt, VariableRefExt};
use crate::{displayed_item::DisplayedItem, wave_container::VariableRef, wave_data::WaveData};

#[derive(PartialEq, Copy, Clone, Debug, Deserialize, Display, FromStr, Serialize, Sequence)]
pub enum VariableNameType {
    /// Local variable name only (i.e. for tb.dut.clk => clk)
    Local,

    /// Add unique prefix, prefix + local
    Unique,

    /// Full variable name (i.e. tb.dut.clk => tb.dut.clk)
    Global,
}

const ELLIPSIS: &str = "…";

impl WaveData {
    pub fn compute_variable_display_names(&mut self) {
        // First pass: collect all unique variable refs
        let full_names: Vec<&VariableRef> = self
            .items_tree
            .iter()
            .filter_map(|node| {
                self.displayed_items
                    .get(&node.item_ref)
                    .and_then(|item| match item {
                        DisplayedItem::Variable(variable) => Some(&variable.variable_ref),
                        _ => None,
                    })
            })
            .unique()
            .collect();
        // Compute minimal unique display names for collision groups.
        let minimal_map = compute_minimal_display_map(&full_names);

        // Single pass: update display names for all items using the precomputed map
        for Node { item_ref, .. } in self.items_tree.iter() {
            self.displayed_items
                .entry(*item_ref)
                .and_modify(|item| match item {
                    DisplayedItem::Variable(variable) => {
                        variable.display_name = match variable.display_name_type {
                            VariableNameType::Local => variable.variable_ref.name.clone(),
                            VariableNameType::Global => variable.variable_ref.full_path_string(),
                            VariableNameType::Unique => minimal_map
                                .get(&variable.variable_ref.full_path_string())
                                .cloned()
                                .unwrap_or_else(|| variable.variable_ref.name.clone()),
                        };
                        if self.display_variable_indices {
                            let index = self
                                .inner
                                .as_waves()
                                .unwrap()
                                .variable_meta(&variable.variable_ref)
                                .ok()
                                .as_ref()
                                .and_then(|meta| meta.index)
                                .map(|index| format!(" {index}"))
                                .unwrap_or_default();
                            variable.display_name = format!("{}{}", variable.display_name, index);
                        }
                    }
                    DisplayedItem::Divider(_) => {}
                    DisplayedItem::Marker(_) => {}
                    DisplayedItem::TimeLine(_) => {}
                    DisplayedItem::Placeholder(_) => {}
                    DisplayedItem::Stream(_) => {}
                    DisplayedItem::Group(_) => {}
                });
        }
    }

    pub fn force_variable_name_type(&mut self, name_type: VariableNameType) {
        for Node { item_ref, .. } in self.items_tree.iter() {
            self.displayed_items.entry(*item_ref).and_modify(|item| {
                if let DisplayedItem::Variable(variable) = item {
                    variable.display_name_type = name_type;
                }
            });
        }
        self.default_variable_name_type = name_type;
        self.compute_variable_display_names();
    }

    pub fn change_variable_name_type(
        &mut self,
        target: MessageTarget<VisibleItemIndex>,
        name_type: VariableNameType,
    ) -> bool {
        let mut recompute_names = false;
        let mut change_type = |item_ref: DisplayedItemRef| {
            self.displayed_items.entry(item_ref).and_modify(|item| {
                if let DisplayedItem::Variable(variable) = item {
                    variable.display_name_type = name_type;
                    recompute_names = true;
                }
            });
        };

        match target {
            MessageTarget::Explicit(vidx) => {
                if let Some(item_ref) = self.items_tree.get_visible(vidx).map(|node| node.item_ref)
                {
                    change_type(item_ref);
                }
            }
            MessageTarget::CurrentSelection => {
                self.items_tree
                    .iter_visible_selected()
                    .for_each(|node| change_type(node.item_ref));
                self.focused_item
                    .and_then(|vidx| self.items_tree.get_visible(vidx))
                    .map(|node| node.item_ref)
                    .map(change_type);
            }
        }
        recompute_names
    }
}

/// Compute minimal unique display names for a set of variables.
/// Returns a map from `full_path_string()` -> minimal display name.
fn compute_minimal_display_map(all_variables: &[&VariableRef]) -> HashMap<String, String> {
    // Group variables by their local name: only collisions within the same
    // local name need disambiguation.
    let mut groups: HashMap<String, Vec<&VariableRef>> = HashMap::new();
    for v in all_variables {
        groups.entry(v.name.clone()).or_default().push(v);
    }

    let mut result: HashMap<String, String> = HashMap::with_capacity(all_variables.len());

    for (_local, vars) in groups {
        if vars.len() == 1 {
            let v = vars[0];
            result.insert(v.full_path_string(), v.name.clone());
            continue;
        }

        // Build reversed scope component vectors for sorting and comparison.
        let mut entries: Vec<(Vec<String>, &VariableRef)> = vars
            .iter()
            .map(|v| {
                let rev: Vec<String> = v.path.strs().iter().rev().cloned().collect();
                (rev, *v)
            })
            .collect();

        entries.sort_by(|a, b| a.0.cmp(&b.0));

        // Helper to compute common prefix length of two reversed component vectors.
        fn common_prefix_len(a: &[String], b: &[String]) -> usize {
            a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
        }

        for (i, (path_i, var_i)) in entries.iter().enumerate() {
            let mut need = 0usize;
            if i > 0 {
                let common = common_prefix_len(path_i, &entries[i - 1].0);
                need = need.max(common + 1);
            }
            if i + 1 < entries.len() {
                let common = common_prefix_len(path_i, &entries[i + 1].0);
                need = need.max(common + 1);
            }

            // If need is zero, fallback to local name.
            if need == 0 || path_i.is_empty() {
                result.insert(var_i.full_path_string(), var_i.name.clone());
                continue;
            }

            // Take up to `need` reversed components, then reverse them back for display.
            let take = need.min(path_i.len());
            let scope = path_i.iter().take(take).rev().cloned().join(".");
            let prefix = if take < path_i.len() { ELLIPSIS } else { "" };

            let display = if scope.is_empty() {
                var_i.name.clone()
            } else {
                format!("{}{}.{}", prefix, scope, var_i.name)
            };
            result.insert(var_i.full_path_string(), display);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_display_map_unique_locals() {
        let v1 = VariableRef::from_hierarchy_string("top.a");
        let v2 = VariableRef::from_hierarchy_string("top.b");
        let vars = vec![&v1, &v2];
        let map = compute_minimal_display_map(&vars);
        assert_eq!(
            map.get(&v1.full_path_string())
                .map(std::string::String::as_str),
            Some("a")
        );
        assert_eq!(
            map.get(&v2.full_path_string())
                .map(std::string::String::as_str),
            Some("b")
        );
    }

    #[test]
    fn minimal_display_map_collisions() {
        let v1 = VariableRef::from_hierarchy_string("top.dut.x");
        let v2 = VariableRef::from_hierarchy_string("other.dut.x");
        let v3 = VariableRef::from_hierarchy_string("top.sub.x");
        let vars = vec![&v1, &v2, &v3];
        let map = compute_minimal_display_map(&vars);

        assert_eq!(
            map.get(&v1.full_path_string())
                .map(std::string::String::as_str),
            Some("top.dut.x")
        );
        assert_eq!(
            map.get(&v2.full_path_string())
                .map(std::string::String::as_str),
            Some("other.dut.x")
        );
        assert_eq!(
            map.get(&v3.full_path_string())
                .map(std::string::String::as_str),
            Some(ELLIPSIS.to_owned() + "sub.x").as_deref()
        );
    }

    #[test]
    fn minimal_display_map_root_and_scoped() {
        let v1 = VariableRef::from_hierarchy_string("x");
        let v2 = VariableRef::from_hierarchy_string("a.x");
        let vars = vec![&v1, &v2];
        let map = compute_minimal_display_map(&vars);
        assert_eq!(
            map.get(&v1.full_path_string())
                .map(std::string::String::as_str),
            Some("x")
        );
        assert_eq!(
            map.get(&v2.full_path_string())
                .map(std::string::String::as_str),
            Some("a.x")
        );
    }
}

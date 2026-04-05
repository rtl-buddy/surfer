use crate::wave_container::VariableMeta;
use egui_remixicon::icons;
use surfer_translation_types::{VariableDirection, VariableNameInfo};

#[local_impl::local_impl]
impl VariableDirectionExt for VariableDirection {
    fn from_wellen_direction(direction: wellen::VarDirection) -> VariableDirection {
        match direction {
            wellen::VarDirection::Unknown => VariableDirection::Unknown,
            wellen::VarDirection::Implicit => VariableDirection::Implicit,
            wellen::VarDirection::Input => VariableDirection::Input,
            wellen::VarDirection::Output => VariableDirection::Output,
            wellen::VarDirection::InOut => VariableDirection::InOut,
            wellen::VarDirection::Buffer => VariableDirection::Buffer,
            wellen::VarDirection::Linkage => VariableDirection::Linkage,
        }
    }

    fn get_icon(&self) -> Option<&str> {
        match self {
            VariableDirection::Unknown => None,
            VariableDirection::Implicit => None,
            VariableDirection::Input => Some(icons::CONTRACT_RIGHT_FILL),
            VariableDirection::Output => Some(icons::EXPAND_RIGHT_FILL),
            VariableDirection::InOut => Some(icons::ARROW_LEFT_RIGHT_LINE),
            VariableDirection::Buffer => None,
            VariableDirection::Linkage => Some(icons::LINK),
        }
    }
}

#[must_use]
pub(crate) fn get_direction_string(
    meta: Option<&VariableMeta>,
    name_info: Option<&VariableNameInfo>,
) -> Option<String> {
    meta.as_ref()
        .and_then(|meta| meta.direction)
        .map(|direction| {
            format!(
                "{} ",
                // Icon based on direction
                direction.get_icon().unwrap_or_else(|| {
                    if meta.as_ref().is_some_and(|meta| meta.is_parameter()) {
                        // If parameter
                        icons::MAP_PIN_2_LINE
                    } else {
                        // Align other items (can be improved)
                        // The padding depends on if we will render monospace or not
                        if name_info.is_some() { "  " } else { "    " }
                    }
                })
            )
        })
}

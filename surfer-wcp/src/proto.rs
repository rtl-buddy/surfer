use num::{BigInt, FromPrimitive};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Number;

/// A reference to a currently displayed item. From the protocol perspective,
/// This can be any integer or a string and what it is is decided by the server,
/// in this case surfer.
/// Since the representation is up to the server, clients cannot generate these on its
/// own, it can only use the ones it has received from the server.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(transparent)]
pub struct DisplayedItemRef(pub usize);

impl From<&DisplayedItemRef> for DisplayedItemRef {
    fn from(value: &DisplayedItemRef) -> Self {
        DisplayedItemRef(value.0)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ItemInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub t: String,
    pub id: DisplayedItemRef,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
pub enum WcpResponse {
    get_item_list { ids: Vec<DisplayedItemRef> },
    get_item_info { results: Vec<ItemInfo> },
    add_items {
        ids: Vec<DisplayedItemRef>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        not_found: Vec<String>,
    },
    add_variables {
        ids: Vec<DisplayedItemRef>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        not_found: Vec<String>,
    },
    add_scope {
        ids: Vec<DisplayedItemRef>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        not_found: Vec<String>,
    },
    add_markers { ids: Vec<DisplayedItemRef> },
    add_dividers { ids: Vec<DisplayedItemRef> },
    query_variable_values {
        /// Native ticks at which the values were sampled. When the
        /// caller asked for a specific timestamp (with optional
        /// `time_unit` conversion), this is that value after conversion.
        /// When the caller asked for "at the cursor" by omitting
        /// `timestamp`, this is the cursor's current position. A driver
        /// can echo this back as the `timestamp` on a follow-up query
        /// to ensure the same sample point.
        ///
        /// Serialized as a decimal string to dodge JSON number precision
        /// loss for long simulations at fs resolution. Matches the
        /// `t_fs` convention used elsewhere in the rtl-buddy stack.
        #[serde(
            serialize_with = "serialize_bigint_as_string",
            deserialize_with = "deserialize_bigint_from_string"
        )]
        timestamp: BigInt,
        /// Per-variable values, in the same order as the request. A
        /// variable that resolved but had no recorded transitions
        /// before `timestamp` appears with `value: null`.
        values: Vec<QueryVariableValue>,
        /// Inputs that did not resolve to a variable in the loaded
        /// waveform. Same shape as the equivalent field on
        /// `add_variables`. Missing in the response when empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        not_found: Vec<String>,
    },
    ack,
}

/// One per (variable, value) row returned by `query_variable_values`.
/// `value` is the per-bit multi-state string (`"0"`, `"1"`, `"x"`,
/// `"z"`, …) matching the encoding `add_variables`-resolved variables
/// use internally. Drivers re-format to hex / decimal / SV-literal as
/// needed.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct QueryVariableValue {
    /// The variable's hierarchy string, echoed verbatim from the
    /// request. Lets the driver join the response back to its own
    /// expected order without indexing.
    pub variable: String,
    /// Per-bit multi-state string. `None` when the variable resolved
    /// but had no recorded transitions strictly before `timestamp`
    /// (e.g. asking for a value before t=0 on a 1-tick-delayed signal).
    pub value: Option<String>,
}

fn serialize_bigint_as_string<S>(value: &BigInt, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&value.to_string())
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "event")]
#[allow(non_camel_case_types)]
pub enum WcpEvent {
    waveforms_loaded { source: String },
    goto_declaration { variable: String, timestamp: Option<u64> },
    cursor_moved { timestamp: u64 },
    scope_changed { scope: String },
    add_drivers { variable: String },
    add_loads { variable: String },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
pub enum WcpSCMessage {
    greeting {
        version: String,
        commands: Vec<String>,
    },
    response(WcpResponse),
    error {
        error: String,
        arguments: Vec<String>,
        message: String,
    },
    event(WcpEvent),
}

impl WcpSCMessage {
    #[must_use]
    pub fn create_greeting(version: usize, commands: Vec<String>) -> Self {
        Self::greeting {
            version: version.to_string(),
            commands,
        }
    }

    #[must_use]
    pub fn create_error(error: String, arguments: Vec<String>, message: String) -> Self {
        Self::error {
            error,
            arguments,
            message,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct MarkerInfo {
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub time: BigInt,
    pub name: Option<String>,
    pub move_focus: bool,
}

/// Unit attached to a `timestamp` / `start` / `end` value on the
/// timestamp-bearing WCP commands. When omitted the integer is interpreted in
/// the loaded waveform's native time unit (the historical behaviour). When
/// set, surfer converts the value to native ticks before applying it.
///
/// Drivers that don't know the FST's timescale (the rtl-buddy hub bridge, a
/// scripted CLI, etc.) can send `"fs"` and stop guessing.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[allow(non_camel_case_types)]
pub enum WcpTimeUnit {
    fs,
    ps,
    ns,
    us,
    ms,
    s,
}

impl WcpTimeUnit {
    /// Power-of-ten exponent of this unit relative to seconds.
    /// e.g. `fs` → `-15`, `ns` → `-9`, `s` → `0`.
    #[must_use]
    pub fn exponent(self) -> i32 {
        match self {
            WcpTimeUnit::fs => -15,
            WcpTimeUnit::ps => -12,
            WcpTimeUnit::ns => -9,
            WcpTimeUnit::us => -6,
            WcpTimeUnit::ms => -3,
            WcpTimeUnit::s => 0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "command")]
#[allow(non_camel_case_types)]
pub enum WcpCommand {
    /// Responds with [`WcpResponse::get_item_list`] which contains a list of the items
    /// in the currently loaded waveforms
    get_item_list,
    /// Responds with [`WcpResponse::get_item_info`] which contains information about
    /// each item specified in `ids` in the same order as in the `ids` array.
    /// Responds with an error if any of the specified IDs are not items in the currently loaded
    /// waveform.
    get_item_info { ids: Vec<DisplayedItemRef> },
    /// Changes the color of the specified item to the specified color.
    /// Responds with [`WcpResponse::ack`]
    /// Responds with an error if the `id` does not exist in the currently loaded waveform.
    set_item_color { id: DisplayedItemRef, color: String },
    // TODO -- remove add_variables and add_scope
    /// Adds the specified variables to the view.
    /// Responds with [`WcpResponse::add_variables`] which contains a list of the item references
    /// that can be used to reference the added items later
    /// Responds with an error if no waveforms are loaded
    add_variables { variables: Vec<String> },
    /// Adds all variables in the specified scope to the view.
    /// Does so recursively if specified
    /// Responds with [`WcpResponse::add_variables`] which contains a list of the item references
    /// that can be used to reference the added items later
    /// Responds with an error if no waveforms are loaded
    add_scope {
        scope: String,
        #[serde(default)]
        recursive: bool,
    },
    /// Navigate surfer's active scope to the named one WITHOUT adding
    /// its variables to the displayed item list. Symmetric in payload
    /// to `add_scope` but does not mutate the variable panel — intended
    /// for cross-view "follow source-focus" tinting where the driver
    /// does not want to spam the user's selected signal set.
    ///
    /// Responds with [`WcpResponse::ack`].
    /// Responds with an error if no waveforms are loaded or if the
    /// scope does not exist in the loaded waveform.
    set_scope { scope: String },
    /// Adds the specified variables or variables in the specified scopes to the view.
    /// Does so recursively if specified
    /// Responds with [`WcpResponse::add_items`] which contains a list of the item references
    /// that can be used to reference the added items later
    /// Responds with an error if no waveforms are loaded
    add_items {
        items: Vec<String>,
        #[serde(default)]
        recursive: bool,
    },
    /// Adds the specified markers to the view.
    /// Responds with [`WcpResponse::add_markers`] which contains a list of the item references
    /// that can be used to reference the added items later
    /// Responds with an error if no waveforms are loaded
    add_markers { markers: Vec<MarkerInfo> },
    /// Reloads the waveform from disk if this is possible for the current waveform format.
    /// If it is not possible, this has no effect.
    /// Responds instantly with [`WcpResponse::ack`]
    /// Once the waveforms have been loaded, a separate event is triggered
    reload,
    /// Moves the viewport to center it on the specified timestamp. Does not affect the zoom
    /// level.
    /// Responds with [`WcpResponse::ack`]
    ///
    /// When `time_unit` is set, surfer converts `timestamp` from that unit to
    /// the loaded waveform's native ticks before applying. When omitted,
    /// the integer is treated as native ticks (historical behaviour).
    set_viewport_to {
        #[serde(deserialize_with = "deserialize_timestamp")]
        timestamp: BigInt,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_unit: Option<WcpTimeUnit>,
    },
    /// Moves the viewport to center it on the specified timestamps range. Does affect the zoom
    /// level.
    /// Responds with [`WcpResponse::ack`]
    ///
    /// When `time_unit` is set, surfer converts both `start` and `end` from
    /// that unit to native ticks before applying. When omitted, the integers
    /// are treated as native ticks (historical behaviour).
    set_viewport_range {
        #[serde(deserialize_with = "deserialize_timestamp")]
        start: BigInt,
        #[serde(deserialize_with = "deserialize_timestamp")]
        end: BigInt,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_unit: Option<WcpTimeUnit>,
    },
    /// Removes the specified items from the view.
    /// Responds with [`WcpResponse::ack`]
    /// Does not error if some of the IDs do not exist
    remove_items { ids: Vec<DisplayedItemRef> },
    /// Sets the specified ID as the _focused_ item.
    /// Responds with [`WcpResponse::ack`]
    /// Responds with an error if no waveforms are loaded or if the item reference
    /// does not exist
    // FIXME: What does this mean in the context of the protocol in general, feels kind
    // of like a Surfer specific thing. Do we have a use case for it
    focus_item { id: DisplayedItemRef },
    /// Removes all currently displayed items
    /// Responds with [`WcpResponse::ack`]
    clear,
    /// Loads a waveform from the specified file.
    /// Responds instantly with [`WcpResponse::ack`]
    /// Once the file is loaded, a [`WcpEvent::waveforms_loaded`] is emitted.
    load { source: String },
    /// Zooms out fully to fit the whole waveform in the view
    /// Responds instantly with [`WcpResponse::ack`]
    zoom_to_fit { viewport_idx: usize },
    /// Set the cursor to the given time.
    /// Responds instantly with [`WcpResponse::ack`]
    ///
    /// When `time_unit` is set, surfer converts `timestamp` from that unit to
    /// native ticks before applying. When omitted, the integer is treated as
    /// native ticks (historical behaviour).
    set_cursor {
        #[serde(deserialize_with = "deserialize_timestamp")]
        timestamp: BigInt,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_unit: Option<WcpTimeUnit>,
    },
    /// Sample one-or-more variables at a point in time and return the
    /// per-bit values. Intended for drivers (the rtl-buddy hub bridge,
    /// scripted CLIs) that want to mirror surfer's view of a signal
    /// without having to add the variable to the displayed panel.
    ///
    /// `timestamp` is optional — when absent the values are sampled at
    /// the current cursor position, which makes the no-arg form the
    /// canonical "what does the cursor currently see?" probe. When
    /// `timestamp` is set, `time_unit` follows the same rules as
    /// `set_cursor` (omitted = native ticks; `"fs"` etc. = unit-aware).
    ///
    /// Responds with [`WcpResponse::query_variable_values`] reporting
    /// both the timestamp actually used (after unit conversion) and
    /// one row per variable. Variables that don't exist in the loaded
    /// waveform land in `not_found` (same shape as `add_variables`);
    /// variables that exist but have no transition before `timestamp`
    /// come back with `value: null`.
    ///
    /// Responds with an error when no waveform is loaded, when
    /// `timestamp` is omitted but no cursor is set, or when
    /// `time_unit` conversion fails (same reasons as `set_cursor`).
    query_variable_values {
        variables: Vec<String>,
        /// When set, sample at this timestamp instead of the cursor.
        /// Decimal string to avoid JSON number precision loss for
        /// long simulations.
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_optional_timestamp",
            serialize_with = "serialize_optional_bigint_as_string"
        )]
        timestamp: Option<BigInt>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_unit: Option<WcpTimeUnit>,
    },
    /// Reorder displayed items. Moves the items named in `ids` (preserving
    /// their relative order) so the block starts at `target_index` in the
    /// visible item list (`target_index >= len` appends at the end, top
    /// level). Intended for drivers (the rtl-buddy hub bridge, scripted
    /// CLIs) that curate the wave view by item id.
    ///
    /// Responds with [`WcpResponse::ack`].
    /// Responds with an error if no waveform is loaded, if any id is not a
    /// currently-displayed item, or if the move is illegal (e.g. moving an
    /// item into its own subtree).
    move_items {
        ids: Vec<DisplayedItemRef>,
        target_index: usize,
    },
    /// Add one or more divider ("comment") rows to the view, in order.
    /// Each entry in `names` becomes one divider; a `null` entry is an
    /// unnamed divider. When `after` is set the dividers are inserted just
    /// after that item, otherwise they are appended.
    ///
    /// Responds with [`WcpResponse::add_dividers`] carrying the new item
    /// references (so they can be moved / removed later), mirroring
    /// `add_markers`.
    /// Responds with an error if no waveform is loaded or `after` names a
    /// non-existent item.
    add_dividers {
        names: Vec<Option<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<DisplayedItemRef>,
    },
    /// Shut down the WCP server.
    // FIXME: What does this mean? Does it kill the server, the current connection or surfer itself?
    shutdown,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
pub enum WcpCSMessage {
    #[serde(rename = "greeting")]
    greeting {
        version: String,
        commands: Vec<String>,
    },
    command(WcpCommand),
}

impl WcpCSMessage {
    #[must_use]
    pub fn create_greeting(version: usize, commands: Vec<String>) -> Self {
        Self::greeting {
            version: version.to_string(),
            commands,
        }
    }
}

fn deserialize_bigint_from_string<'de, D>(deserializer: D) -> Result<BigInt, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<BigInt>().map_err(de::Error::custom)
}

fn serialize_optional_bigint_as_string<S>(
    value: &Option<BigInt>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(v) => serializer.serialize_str(&v.to_string()),
        None => serializer.serialize_none(),
    }
}

/// Accept either a JSON number (back-compat with existing
/// timestamp-bearing requests) or a decimal string (forward-compat with
/// the response shape on `query_variable_values`).
fn deserialize_optional_timestamp<'de, D>(deserializer: D) -> Result<Option<BigInt>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Num(Number),
        Str(String),
    }
    let opt = Option::<Either>::deserialize(deserializer)?;
    let Some(either) = opt else { return Ok(None) };
    let bigint = match either {
        Either::Num(num) => {
            if let Some(n) = num.as_u128() {
                BigInt::from(n)
            } else if let Some(n) = num.as_i128() {
                BigInt::from(n)
            } else if let Some(n) = num.as_f64() {
                BigInt::from_f64(n).ok_or_else(|| {
                    <D::Error as serde::de::Error>::invalid_value(
                        serde::de::Unexpected::Float(n),
                        &"a finite value",
                    )
                })?
            } else {
                return Err(de::Error::custom(format!(
                    "Error during deserialization of timestamp value {num}"
                )));
            }
        }
        Either::Str(s) => s.parse::<BigInt>().map_err(de::Error::custom)?,
    };
    Ok(Some(bigint))
}

fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<BigInt, D::Error>
where
    D: Deserializer<'de>,
{
    let num = Number::deserialize(deserializer)?;
    if let Some(timestamp) = num.as_u128() {
        Ok(BigInt::from(timestamp))
    } else if let Some(timestamp) = num.as_i128() {
        Ok(BigInt::from(timestamp))
    } else if let Some(timestamp) = num.as_f64() {
        BigInt::from_f64(timestamp).ok_or_else(|| {
            <D::Error as serde::de::Error>::invalid_value(
                serde::de::Unexpected::Float(timestamp),
                &"a finite value",
            )
        })
    } else {
        Err(de::Error::custom(format!(
            "Error during deserialization of timestamp value {num}"
        )))
    }
}

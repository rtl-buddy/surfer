use egui::{Response, Ui};
use egui_extras::{Column, TableBuilder};
use ftr_parser::types::Transaction;
use num::BigUint;

use crate::{
    transaction_container::{TransactionRef, TransactionStreamRef},
    wave_container::{ScopeRef, VariableMeta, VariableRef, VariableRefExt},
    wave_data::WaveData,
};

// Try to locate a transaction for the tooltip without panicking
fn find_transaction<'a>(
    waves: &'a WaveData,
    gen_ref: &TransactionStreamRef,
    tx_ref: &TransactionRef,
) -> Option<&'a Transaction> {
    let txs = waves.inner.as_transactions()?;
    let gen_id = gen_ref.gen_id?;
    let generator = txs.get_generator(gen_id)?;
    generator
        .transactions
        .iter()
        .find(|transaction| transaction.get_tx_id() == tx_ref.id)
}

#[must_use]
pub(crate) fn variable_tooltip_text(meta: Option<&VariableMeta>, variable: &VariableRef) -> String {
    if let Some(meta) = meta {
        format!(
            "{}\nNum bits: {}\nType: {}\nDirection: {}",
            variable.full_path_string(),
            meta.num_bits
                .map_or_else(|| "unknown".to_string(), |bits| bits.to_string()),
            meta.variable_type_name
                .clone()
                .or_else(|| meta.variable_type.map(|t| t.to_string()))
                .unwrap_or_else(|| "unknown".to_string()),
            meta.direction
                .map_or_else(|| "unknown".to_string(), |direction| format!("{direction}"))
        )
    } else {
        variable.full_path_string()
    }
}

#[must_use]
pub(crate) fn scope_tooltip_text(
    wave: &WaveData,
    scope: &ScopeRef,
    include_parameters: bool,
) -> String {
    let mut parts = vec![format!("{scope}")];
    if let Some(wave_container) = &wave.inner.as_waves() {
        if include_parameters && let Some(waves) = &wave.inner.as_waves() {
            for param in &waves.parameters_in_scope(scope) {
                let value = wave_container
                    .query_variable(param, &BigUint::ZERO)
                    .ok()
                    .and_then(|o| o.and_then(|q| q.current.map(|v| format!("{}", v.1))))
                    .unwrap_or_else(|| "Undefined".to_string());
                parts.push(format!("{}: {}", param.name, value));
            }
        }
        let other = wave_container.get_scope_tooltip_data(scope);
        if !other.is_empty() {
            parts.push(other);
        }
    }
    parts.join("\n")
}

#[must_use]
pub(crate) fn handle_transaction_tooltip(
    response: Response,
    waves: &WaveData,
    gen_ref: &TransactionStreamRef,
    tx_ref: &TransactionRef,
) -> Response {
    response
        .on_hover_ui(|ui| {
            if let Some(tx) = find_transaction(waves, gen_ref, tx_ref) {
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.add(egui::Label::new(transaction_tooltip_text(waves, tx)));
            } else {
                ui.label("Transaction unavailable");
            }
        })
        .on_hover_ui(|ui| {
            // Seemingly a bit redundant to determine tx twice, but since the
            // alternative is to do it every frame for every transaction, this
            // is most likely still a better approach.
            // Feel free to use some Rust magic to only do it once though...
            if let Some(tx) = find_transaction(waves, gen_ref, tx_ref) {
                transaction_tooltip_table(ui, tx);
            } else {
                ui.label("Transaction details unavailable");
            }
        })
}

fn transaction_tooltip_text(waves: &WaveData, tx: &Transaction) -> String {
    let time_scale = waves
        .inner
        .as_transactions()
        .map(|t| t.inner.time_scale.to_string())
        .unwrap_or_default();

    format!(
        "tx#{}: {}{} - {}{}\nType: {}",
        tx.event.tx_id,
        tx.event.start_time,
        time_scale,
        tx.event.end_time,
        time_scale,
        waves
            .inner
            .as_transactions()
            .and_then(|t| t.get_generator(tx.get_gen_id()))
            .map_or_else(|| "unknown".to_string(), |g| g.name.clone()),
    )
}

fn transaction_tooltip_table(ui: &mut Ui, tx: &Transaction) {
    TableBuilder::new(ui)
        .column(Column::exact(80.))
        .column(Column::exact(80.))
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.heading("Attribute");
            });
            header.col(|ui| {
                ui.heading("Value");
            });
        })
        .body(|body| {
            let total_rows = tx.attributes.len();
            let attributes = &tx.attributes;
            body.rows(15., total_rows, |mut row| {
                if let Some(attribute) = attributes.get(row.index()) {
                    row.col(|ui| {
                        ui.label(attribute.name.clone());
                    });
                    row.col(|ui| {
                        ui.label(attribute.value());
                    });
                }
            });
        });
}

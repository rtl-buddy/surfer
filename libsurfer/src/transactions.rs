use egui::{Context, Layout, RichText, TextWrapMode, Ui};
use egui_extras::{Column, TableBody, TableBuilder};
use emath::Align;
use ftr_parser::types::Transaction;
use itertools::Itertools;
use num::BigUint;

use crate::SystemState;
use crate::message::Message;
use crate::transaction_container::TransactionStreamRef;
use crate::transaction_container::{StreamScopeRef, TransactionContainer};
use crate::wave_data::ScopeType;
use crate::wave_data::WaveData;

// Transactions file extension
pub const TRANSACTIONS_FILE_EXTENSION: &str = "ftr";

// Constants for transaction table drawing and UI labels
const ROW_HEIGHT: f32 = 15.;
const SECTION_GAP: f32 = 5.;
const SUBHEADER_GAP: f32 = 3.;
const SUBHEADER_SIZE: f32 = 15.;

// Root stream name
const TRANSACTION_ROOT_NAME: &str = "tr";

// Header / section titles
const FOCUSED_TX_DETAILS_HDR: &str = "Focused Transaction Details";
const PROPERTIES_HDR: &str = "Properties";
const ATTRIBUTES_SECTION_TITLE: &str = "Attributes";
const INCOMING_RELATIONS_TITLE: &str = "Incoming Relations";
const OUTGOING_RELATIONS_TITLE: &str = "Outgoing Relations";

// Column / field labels
const TX_ID_LABEL: &str = "Transaction ID";
const TX_TYPE_LABEL: &str = "Type";
const START_TIME_LABEL: &str = "Start Time";
const END_TIME_LABEL: &str = "End Time";
const SOURCE_TX_LABEL: &str = "Source Tx";
const SINK_TX_LABEL: &str = "Sink Tx";
const ATTR_NAME_LABEL: &str = "Name";
const ATTR_VALUE_LABEL: &str = "Value";

// Information label
const STREAM_NOT_FOUND_LABEL: &str = "Stream not found";

impl SystemState {
    pub fn draw_transaction_detail_panel(
        &self,
        ctx: &Context,
        max_width: f32,
        msgs: &mut Vec<Message>,
    ) {
        let Some(waves) = self.user.waves.as_ref() else {
            return;
        };
        let (Some(transaction_ref), focused_transaction) = &waves.focused_transaction else {
            return;
        };
        let Some(transactions) = waves.inner.as_transactions() else {
            return;
        };
        let Some(focused_transaction) = focused_transaction
            .as_ref()
            .or_else(|| transactions.get_transaction(transaction_ref))
        else {
            return;
        };

        egui::SidePanel::right("Transaction Details")
            .default_width(330.)
            .width_range(10.0..=max_width)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                self.handle_pointer_in_ui(ui, msgs);
                draw_focused_transaction_details(ui, transactions, focused_transaction);
            });
    }
}

fn draw_focused_transaction_details(
    ui: &mut Ui,
    transactions: &TransactionContainer,
    focused_transaction: &Transaction,
) {
    ui.with_layout(
        Layout::top_down(Align::LEFT).with_cross_justify(true),
        |ui| {
            ui.label(FOCUSED_TX_DETAILS_HDR);
            let column_width = ui.available_width() / 2.;
            TableBuilder::new(ui)
                .column(Column::exact(column_width))
                .column(Column::auto())
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.heading(PROPERTIES_HDR);
                    });
                })
                .body(|mut body| {
                    table_row(
                        &mut body,
                        TX_ID_LABEL,
                        &focused_transaction.get_tx_id().to_string(),
                    );
                    table_row(&mut body, TX_TYPE_LABEL, {
                        let generator = transactions
                            .get_generator(focused_transaction.get_gen_id())
                            .unwrap();
                        &generator.name
                    });
                    table_row(
                        &mut body,
                        START_TIME_LABEL,
                        &focused_transaction.get_start_time().to_string(),
                    );
                    table_row(
                        &mut body,
                        END_TIME_LABEL,
                        &focused_transaction.get_end_time().to_string(),
                    );
                    section_header(&mut body, ATTRIBUTES_SECTION_TITLE);
                    subheader(&mut body, ATTR_NAME_LABEL, ATTR_VALUE_LABEL);

                    for attr in &focused_transaction.attributes {
                        table_row(&mut body, &attr.name, &attr.value().to_string());
                    }

                    if !focused_transaction.inc_relations.is_empty() {
                        section_header(&mut body, INCOMING_RELATIONS_TITLE);
                        subheader(&mut body, SOURCE_TX_LABEL, SINK_TX_LABEL);

                        for rel in &focused_transaction.inc_relations {
                            table_row(
                                &mut body,
                                &rel.source_tx_id.to_string(),
                                &rel.sink_tx_id.to_string(),
                            );
                        }
                    }

                    if !focused_transaction.out_relations.is_empty() {
                        section_header(&mut body, OUTGOING_RELATIONS_TITLE);
                        subheader(&mut body, SOURCE_TX_LABEL, SINK_TX_LABEL);

                        for rel in &focused_transaction.out_relations {
                            table_row(
                                &mut body,
                                &rel.source_tx_id.to_string(),
                                &rel.sink_tx_id.to_string(),
                            );
                        }
                    }
                });
        },
    );
}

pub fn calculate_rows_of_stream(
    transactions: &[Transaction],
    last_times_on_row: &mut Vec<(BigUint, BigUint)>,
) {
    for transaction in transactions {
        let mut curr_row = 0;
        let start_time = transaction.get_start_time();
        let end_time = transaction.get_end_time();

        while last_times_on_row[curr_row].1 > start_time {
            curr_row += 1;
            if last_times_on_row.len() <= curr_row {
                last_times_on_row.push((BigUint::ZERO, BigUint::ZERO));
            }
        }
        last_times_on_row[curr_row] = (start_time, end_time);
    }
}

pub fn draw_transaction_variable_list(
    msgs: &mut Vec<Message>,
    streams: &WaveData,
    ui: &mut Ui,
    active_stream: &StreamScopeRef,
) {
    let Some(inner) = streams.inner.as_transactions() else {
        return;
    };
    match active_stream {
        StreamScopeRef::Root => {
            draw_transaction_root_variables(msgs, ui, inner);
        }
        StreamScopeRef::Stream(stream_ref) => {
            draw_transaction_stream_variables(msgs, ui, inner, stream_ref);
        }
        StreamScopeRef::Empty(_) => {}
    }
}

pub fn draw_transaction_root(msgs: &mut Vec<Message>, streams: &WaveData, ui: &mut Ui) {
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        egui::Id::from("Streams"),
        false,
    )
    .show_header(ui, |ui| {
        ui.with_layout(
            Layout::top_down(Align::LEFT).with_cross_justify(true),
            |ui| {
                let response = ui.selectable_label(
                    streams.active_scope == Some(ScopeType::StreamScope(StreamScopeRef::Root)),
                    TRANSACTION_ROOT_NAME,
                );
                if response.clicked() {
                    msgs.push(Message::SetActiveScope(Some(ScopeType::StreamScope(
                        StreamScopeRef::Root,
                    ))));
                }
            },
        );
    })
    .body(|ui| {
        if let Some(tx_container) = streams.inner.as_transactions() {
            for (id, stream) in &tx_container.inner.tx_streams {
                let selected = streams.active_scope.as_ref().is_some_and(|s| {
                    if let ScopeType::StreamScope(StreamScopeRef::Stream(scope_stream)) = s {
                        scope_stream.stream_id == *id
                    } else {
                        false
                    }
                });
                let response = ui.selectable_label(selected, &stream.name);
                if response.clicked() {
                    msgs.push(Message::SetActiveScope(Some(ScopeType::StreamScope(
                        StreamScopeRef::Stream(TransactionStreamRef::new_stream(
                            *id,
                            stream.name.clone(),
                        )),
                    ))));
                }
            }
        }
    });
}

fn draw_transaction_stream_variables(
    msgs: &mut Vec<Message>,
    ui: &mut Ui,
    inner: &TransactionContainer,
    stream_ref: &TransactionStreamRef,
) {
    if let Some(stream) = inner.get_stream(stream_ref.stream_id) {
        let sorted_generators = stream
            .generators
            .iter()
            .filter_map(|gen_id| {
                if let Some(g) = inner.get_generator(*gen_id) {
                    Some((*gen_id, g))
                } else {
                    tracing::warn!(
                        "Generator ID {} not found in stream {}",
                        gen_id,
                        stream_ref.stream_id
                    );
                    None
                }
            })
            .sorted_by(|(_, a), (_, b)| numeric_sort::cmp(&a.name, &b.name));

        for (gen_id, generator) in sorted_generators {
            ui.with_layout(
                Layout::top_down(Align::LEFT).with_cross_justify(true),
                |ui| {
                    if ui.selectable_label(false, &generator.name).clicked() {
                        msgs.push(Message::AddStreamOrGenerator(
                            TransactionStreamRef::new_gen(
                                stream_ref.stream_id,
                                gen_id,
                                generator.name.clone(),
                            ),
                        ));
                    }
                },
            );
        }
    } else {
        ui.label(STREAM_NOT_FOUND_LABEL);
        tracing::warn!(
            "Stream ID {} not found in transaction container",
            stream_ref.stream_id
        );
    }
}

fn draw_transaction_root_variables(
    msgs: &mut Vec<Message>,
    ui: &mut Ui,
    inner: &TransactionContainer,
) {
    let streams = inner.get_streams();
    let sorted_streams = streams
        .iter()
        .sorted_by(|a, b| numeric_sort::cmp(&a.name, &b.name));
    for stream in sorted_streams {
        ui.with_layout(
            Layout::top_down(Align::LEFT).with_cross_justify(true),
            |ui| {
                let response = ui.selectable_label(false, &stream.name);
                if response.clicked() {
                    msgs.push(Message::AddStreamOrGenerator(
                        TransactionStreamRef::new_stream(stream.id, stream.name.clone()),
                    ));
                }
            },
        );
    }
}

// Helper functions for drawing transaction details table

fn table_row(body: &mut TableBody, key: &str, val: &str) {
    body.row(ROW_HEIGHT, |mut row| {
        row.col(|ui| {
            ui.label(key);
        });
        row.col(|ui| {
            ui.label(val);
        });
    });
}

fn section_header(body: &mut TableBody, title: &str) {
    body.row(ROW_HEIGHT + SECTION_GAP, |mut row| {
        row.col(|ui| {
            ui.heading(title);
        });
    });
}

fn subheader(body: &mut TableBody, left: &str, right: &str) {
    body.row(ROW_HEIGHT + SUBHEADER_GAP, |mut row| {
        row.col(|ui| {
            ui.label(RichText::new(left).size(SUBHEADER_SIZE));
        });
        row.col(|ui| {
            ui.label(RichText::new(right).size(SUBHEADER_SIZE));
        });
    });
}

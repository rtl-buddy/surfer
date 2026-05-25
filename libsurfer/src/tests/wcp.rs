use std::path::PathBuf;

use crate::SystemState;
use crate::message::Message;
use crate::tests::snapshot::render_and_compare;
use itertools::Itertools;
use surfer_wcp::{
    MarkerInfo, WcpCSMessage, WcpCommand, WcpEvent, WcpResponse, WcpSCMessage, WcpTimeUnit, proto,
};

use eyre::Result;
use eyre::bail;
use futures::Future;
use num::BigInt;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc::{Receiver, Sender};

macro_rules! expect_response {
    ($rx:expr, $expected:pat$(,)?) => {
        let received = tokio::select! {
            result = $rx.recv() => {
                result
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                bail!("Timeout waiting for {}", stringify!($expected))
            }
        };

        let Some($expected) = received else {
            bail!(
                "Got unexpected response {received:?} expected {}",
                stringify!(expected)
            )
        };
    };
}

async fn expect_ack(rx: &mut tokio::sync::mpsc::Receiver<WcpSCMessage>) -> Result<()> {
    expect_response! {
        rx, WcpSCMessage::response(WcpResponse::ack)
    }
    Ok(())
}

fn run_wcp_test<C, F>(test_name: String, client: C)
where
    C: Fn(Sender<WcpCSMessage>, Receiver<WcpSCMessage>) -> F + Sync + Send + Clone + 'static,
    F: Future<Output = Result<()>> + Send + Sync,
{
    let test_name = format!("wcp/{test_name}");

    render_and_compare(&PathBuf::from(test_name), move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();

        // create state and add messages as batch commands
        let mut state = SystemState::new_default_config().unwrap();

        let setup_msgs = vec![
            // hide GUI elements
            Message::SetMenuVisible(false),
            Message::SetSidePanelVisible(false),
            Message::SetToolbarVisible(false),
            Message::SetOverviewVisible(false),
        ];

        for msg in setup_msgs {
            state.update(msg);
        }

        let (runner_tx, mut runner_rx) = tokio::sync::oneshot::channel();

        let (sc_tx, sc_rx) = tokio::sync::mpsc::channel(100);
        state.channels.wcp_s2c_sender = Some(sc_tx);
        let (cs_tx, cs_rx) = tokio::sync::mpsc::channel(100);
        state.channels.wcp_c2s_receiver = Some(cs_rx);
        state.wcp_running_signal.store(true, Ordering::Relaxed);

        {
            let client = client.clone();
            runtime.spawn(async move {
                let result: Result<()> = async { client(cs_tx, sc_rx).await }.await;
                runner_tx.send(result).unwrap();
            });
        }

        runtime.block_on(async {
            // update state until all batch commands have been processed
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
                        state.handle_async_messages();
                        state.handle_wcp_commands();
                    }
                    exit = &mut runner_rx => {
                        match exit {
                            Ok(Ok(())) => {
                                state.handle_async_messages();
                                state.handle_wcp_commands();
                                break;
                            }
                            Ok(Err(e)) => {
                                panic!("Runner exited with\n{e:#}")
                            }
                            Err(e) => {
                                panic!("Runner disconnected with\n{e:#}")
                            }
                        }
                    }

                }
            }

            state
        })
    });
}

macro_rules! wcp_test {
    ($test_name:ident, ($tx:ident, $rx:ident) $body:tt) => {
        #[test]
        fn $test_name() {
            async fn client($tx: Sender<WcpCSMessage>, mut $rx: Receiver<WcpSCMessage>) -> eyre::Result<()> $body

            run_wcp_test(stringify!($test_name).to_string(), client)
        }
    };
}

async fn send_commands(tx: &Sender<WcpCSMessage>, cmds: Vec<WcpCommand>) -> Result<()> {
    for cmd in cmds {
        tx.send(WcpCSMessage::command(cmd)).await?;
    }
    Ok(())
}

async fn greet(tx: &Sender<WcpCSMessage>, rx: &mut Receiver<WcpSCMessage>) -> Result<()> {
    let commands = vec!["waveforms_loaded", "goto_declaration"]
        .into_iter()
        .map(str::to_string)
        .collect_vec();
    tx.send(WcpCSMessage::greeting {
        version: "0".to_string(),
        commands,
    })
    .await?;

    expect_response!(
        rx,
        WcpSCMessage::greeting {
            version: v,
            commands
        },
    );
    assert_eq!(v, "0");
    let e_commands = vec![
        "add_variables",
        "set_viewport_to",
        "cursor_set",
        "reload",
        "add_scope",
        "set_scope",
        "add_items",
        "get_item_list",
        "set_item_color",
        "get_item_info",
        "clear_item",
        "focus_item",
        "clear",
        "load",
        "zoom_to_fit",
        "add_markers",
        "set_viewport_range_to",
    ];
    assert_eq!(commands, e_commands);

    Ok(())
}

async fn load_file(
    tx: &Sender<WcpCSMessage>,
    rx: &mut Receiver<WcpSCMessage>,
    file: &str,
) -> Result<()> {
    greet(tx, rx).await?;

    tx.send(WcpCSMessage::command(proto::WcpCommand::load {
        source: file.to_string(),
    }))
    .await?;
    expect_ack(rx).await?;

    expect_response!(
        rx,
        WcpSCMessage::event(WcpEvent::waveforms_loaded { source })
    );
    assert_eq!(source, file.to_string());

    Ok(())
}

wcp_test! {greeting_works, (tx, rx) {
    greet(&tx, &mut rx).await?;

    Ok(())
}}

wcp_test! {
    loading_waveforms_works,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_items{ items: vec![
                "tb._tmp",
                "tb.clk",
                "tb.overflow",
                "tb.reset"
            ].into_iter().map(str::to_string).collect(), recursive: false
        })).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: indices, not_found: _ }));

        dbg!(&indices);
        assert_eq!(indices.len(), 4);

        Ok(())
    }
}

wcp_test! {
    add_scope,
    (tx, rx) {
        greet(&tx, &mut rx).await?;

        tx.send(WcpCSMessage::command(proto::WcpCommand::load {
            source: "../examples/counter.vcd".to_string()
        })).await?;
        expect_ack(&mut rx).await?;

        expect_response!(rx, WcpSCMessage::event(WcpEvent::waveforms_loaded{source}));
        assert_eq!(source, "../examples/counter.vcd".to_string());

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: false})).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: indices, not_found: _ }));

        assert_eq!(indices.len(), 4);

        Ok(())
    }
}

wcp_test! {
    add_scope_recursive,
    (tx, rx) {
        greet(&tx, &mut rx).await?;

        tx.send(WcpCSMessage::command(proto::WcpCommand::load {
            source: "../examples/counter.vcd".to_string()
        })).await?;
        expect_ack(&mut rx).await?;

        expect_response!(rx, WcpSCMessage::event(WcpEvent::waveforms_loaded{source}));
        assert_eq!(source, "../examples/counter.vcd".to_string());

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: true})).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: indices, not_found: _ }));

        assert_eq!(indices.len(), 8);

        Ok(())
    }
}

wcp_test! {
    add_var_and_scope,
    (tx, rx) {
        greet(&tx, &mut rx).await?;

        tx.send(WcpCSMessage::command(proto::WcpCommand::load {
            source: "../examples/counter.vcd".to_string()
        })).await?;
        expect_ack(&mut rx).await?;

        expect_response!(rx, WcpSCMessage::event(WcpEvent::waveforms_loaded{source}));
        assert_eq!(source, "../examples/counter.vcd".to_string());

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_items{items: vec!["tb", "tb.dut.counter"].into_iter().map(str::to_string).collect(), recursive: false})).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: indices, not_found: _ }));

        assert_eq!(indices.len(), 5);
        Ok(())
    }
}

wcp_test! {
    add_markers,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::add_scope {scope: "tb".to_string(), recursive: false},
            WcpCommand::add_markers { markers: vec![MarkerInfo { time: BigInt::from(100), name: Some("marker".into()), move_focus: false }] },
        ]).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_scope{ ids: _, not_found: _ }));
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_markers{ ids: _ }));
        Ok(())
    }
}

wcp_test! {
    color_variables,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_items{ items: vec![
                "tb._tmp",
                "tb.clk",
                "tb.overflow",
                "tb.reset"
            ].into_iter().map(str::to_string).collect(),
            recursive: false
        })).await?;

        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: refs, not_found: _ }));

        for (i, c) in [(1, "Gray"), (2, "Yellow"), (3, "Blue")] {
            tx.send(WcpCSMessage::command(
                proto::WcpCommand::set_item_color { id: refs[i], color: c.to_string() }
            )).await?;
            expect_ack(&mut rx).await?;
        }

        Ok(())
    }
}

wcp_test! {
    remove_2_variables,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: false})).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: refs, not_found: _ }));

        send_commands(&tx, vec![
            WcpCommand::remove_items { ids: vec![refs[1], refs[2]] }
        ]).await?;

        expect_ack(&mut rx).await?;

        Ok(())
    }
}

wcp_test! {
    focus_item,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: false})).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: refs, not_found: _ }));

        send_commands(&tx, vec![
            WcpCommand::focus_item { id: refs[1] }
        ]).await?;
        // focus_item on a Variable emits scope_changed before ack
        // (added in adf9f89 / #rtl-buddy-52).
        expect_response!(rx, WcpSCMessage::event(WcpEvent::scope_changed { scope: _ }));
        expect_ack(&mut rx).await
    }
}

wcp_test! {
    clear,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: false},
            WcpCommand::clear,
        ]).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: _, not_found: _ }));
        expect_ack(&mut rx).await
    }
}

wcp_test! {
    set_viewport_to,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: false},
            WcpCommand::set_viewport_to { timestamp: BigInt::from(70), time_unit: None },
        ]).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: _, not_found: _ }));
        expect_ack(&mut rx).await?;
        Ok(())
    }
}

wcp_test! {
    set_viewport_range_to,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::add_scope {scope: "tb".to_string(), recursive: false},
            WcpCommand::set_viewport_range { start: BigInt::from(70), end: BigInt::from(120), time_unit: None },
        ]).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_scope{ ids: _, not_found: _ }));
        expect_ack(&mut rx).await?;
        Ok(())
    }
}

wcp_test! {
    zoom_to_fit,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: false},
            WcpCommand::set_viewport_to { timestamp: BigInt::from(70), time_unit: None },
            WcpCommand::zoom_to_fit { viewport_idx: 0 }
        ]).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: _, not_found: _ }));
        expect_ack(&mut rx).await?;
        expect_ack(&mut rx).await?;

        Ok(())
    }
}

wcp_test! {
    // `set_scope` navigates surfer's active scope without mutating
    // the displayed item list. After the command surfer should still
    // have zero displayed items (in contrast to `add_scope` which
    // would populate the panel with the scope's variables).
    set_scope_navigates_without_adding_variables,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::set_scope { scope: "tb".to_string() },
            WcpCommand::get_item_list,
        ]).await?;
        expect_ack(&mut rx).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::get_item_list { ids }));
        assert!(
            ids.is_empty(),
            "set_scope must not add variables; got {} item(s)", ids.len()
        );
        Ok(())
    }
}

wcp_test! {
    // Unknown scope → structured error (not an ack), payload carries the
    // offending name so callers can debug typos.
    set_scope_unknown_scope_errors,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::set_scope { scope: "no_such_scope".to_string() },
        ]).await?;
        expect_response!(rx, WcpSCMessage::error { error, arguments, message: _ });
        assert_eq!(error, "set_scope");
        assert_eq!(arguments, vec!["no_such_scope".to_string()]);
        Ok(())
    }
}

wcp_test! {
    // Smoke test for time_unit conversion: events.vcd's `$timescale 1ps`,
    // so 1 native tick = 1 ps = 1000 fs. Send the viewport-range commands
    // in three different units (native ticks, fs, ns) and expect surfer to
    // ack all three — actual rescaling correctness is covered by the unit
    // helper's behaviour; the test here just confirms the end-to-end pipe.
    set_viewport_with_time_unit,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/events.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::add_scope { scope: "logic".to_string(), recursive: false },
            // 100 native ticks (= 100 ps)
            WcpCommand::set_viewport_range {
                start: BigInt::from(0),
                end: BigInt::from(100),
                time_unit: None,
            },
            // 100_000 fs (= 100 ps = same target)
            WcpCommand::set_viewport_range {
                start: BigInt::from(0),
                end: BigInt::from(100_000),
                time_unit: Some(WcpTimeUnit::fs),
            },
            // 100 ns (= 100_000 ps; well past the trace, but surfer still
            // acks — clamping is a viewport concern, not a unit concern).
            WcpCommand::set_viewport_to {
                timestamp: BigInt::from(100),
                time_unit: Some(WcpTimeUnit::ns),
            },
        ]).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_scope{ ids: _, not_found: _ }));
        expect_ack(&mut rx).await?;
        expect_ack(&mut rx).await?;
        expect_ack(&mut rx).await?;
        Ok(())
    }
}

wcp_test! {
    get_item_info,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: false},
        ]).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: items, not_found: _ }));

        send_commands(&tx, vec![
            WcpCommand::get_item_info { ids: items }
        ]).await?;

        expect_response!(rx, WcpSCMessage::response(WcpResponse::get_item_info{ results: info }));
        let expected = vec![
             proto::ItemInfo { name: "_tmp".to_string(),
                 t: "Variable".to_string(),
                 id: proto::DisplayedItemRef(1)
             },
             proto::ItemInfo { name: "clk".to_string(),
                 t: "Variable".to_string(),
                 id: proto::DisplayedItemRef(2)
             },
             proto::ItemInfo { name: "overflow".to_string(),
                 t: "Variable".to_string(),
                 id: proto::DisplayedItemRef(3)
             },
             proto::ItemInfo { name: "reset".to_string(),
                 t: "Variable".to_string(),
                 id: proto::DisplayedItemRef(4)
             },
        ];
        assert_eq!(info, expected);

        Ok(())
    }
}

wcp_test! {
    get_item_info_invalid_id,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        send_commands(&tx, vec![
            WcpCommand::add_items{items: vec!["tb"].into_iter().map(str::to_string).collect(), recursive: false},
        ]).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_items{ ids: _, not_found: _ }));

        send_commands(&tx, vec![
            WcpCommand::get_item_info { ids: vec![proto::DisplayedItemRef(usize::MAX)] }
        ]).await?;

        expect_response!(rx, WcpSCMessage::error{error, arguments: _, message});
        assert_eq!(error, "get_item_info");
        assert_eq!(message, "No item with id DisplayedItemRef(18446744073709551615)");

        Ok(())
    }
}

wcp_test! {
    no_greeting,
    (tx, rx) {
        send_commands(&tx, vec![
            WcpCommand::clear,
        ]).await?;
        expect_response!(rx, WcpSCMessage::error{error, ..});
        assert_eq!(error, "WCP server has not received greeting messages");

        Ok(())
    }
}

// add_variables and add_scope are deprecated
// TODO -- remove these commands and tests
wcp_test! {
    deprecated_add_variables,
    (tx, rx) {
        load_file(&tx, &mut rx, "../examples/counter.vcd").await?;

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_variables { variables: vec![
                "tb._tmp",
                "tb.clk",
                "tb.overflow",
                "tb.reset"
            ].into_iter().map(str::to_string).collect()
        })).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_variables{ ids: indices, not_found: _ }));

        assert_eq!(indices.len(), 4);

        Ok(())
    }
}

wcp_test! {
    deprecated_add_scope,
    (tx, rx) {
        greet(&tx, &mut rx).await?;

        tx.send(WcpCSMessage::command(proto::WcpCommand::load {
            source: "../examples/counter.vcd".to_string()
        })).await?;
        expect_ack(&mut rx).await?;

        expect_response!(rx, WcpSCMessage::event(WcpEvent::waveforms_loaded{source}));
        assert_eq!(source, "../examples/counter.vcd".to_string());

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_scope {scope: "tb".to_string(), recursive: false})).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_scope{ ids: indices, not_found: _ }));

        assert_eq!(indices.len(), 4);

        Ok(())
    }
}

wcp_test! {
    deprecated_add_scope_recursive,
    (tx, rx) {
        greet(&tx, &mut rx).await?;

        tx.send(WcpCSMessage::command(proto::WcpCommand::load {
            source: "../examples/counter.vcd".to_string()
        })).await?;
        expect_ack(&mut rx).await?;

        expect_response!(rx, WcpSCMessage::event(WcpEvent::waveforms_loaded{source}));
        assert_eq!(source, "../examples/counter.vcd".to_string());

        tx.send(WcpCSMessage::command(
            proto::WcpCommand::add_scope {scope: "tb".to_string(), recursive: true})).await?;
        expect_response!(rx, WcpSCMessage::response(WcpResponse::add_scope{ ids: indices, not_found: _ }));

        assert_eq!(indices.len(), 8);

        Ok(())
    }
}

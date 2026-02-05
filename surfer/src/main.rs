#![cfg_attr(not(target_arch = "wasm32"), deny(unused_crate_dependencies))]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
mod main_impl {
    use camino::Utf8PathBuf;
    use clap::Parser;
    use emath::Vec2;
    use eyre::Context;
    use eyre::Result;
    use libsurfer::{
        StartupParams, SystemState,
        EGUI_CONTEXT,
        batch_commands::read_command_file,
        file_watcher::FileWatcher,
        logs,
        message::Message,
        run_egui,
        wave_source::{WaveSource, string_to_wavesource},
    };
    use tracing::error;

    #[derive(clap::Subcommand)]
    enum Commands {
        #[cfg(not(target_arch = "wasm32"))]
        /// starts surfer in headless mode so that a user can connect to it
        Server {
            /// port on which server will listen
            #[clap(long)]
            port: Option<u16>,
            /// IP address to bind the server to
            #[clap(long)]
            bind_address: Option<String>,
            /// token used by the client to authenticate to the server
            #[clap(long)]
            token: Option<String>,
            /// waveform file that we want to serve
            #[arg(long)]
            file: String,
        },
    }

    #[derive(clap::Parser, Default)]
    #[command(version = concat!(env!("CARGO_PKG_VERSION"), " (git: ", env!("VERGEN_GIT_DESCRIBE"), ")"), about)]
    struct Args {
        /// Waveform file in VCD, FST, or GHW format.
        wave_file: Option<String>,
        /// Path to a file containing 'commands' to run after a waveform has been loaded.
        /// The commands are the same as those used in the command line interface inside the program.
        /// Commands are separated by lines or ;. Empty lines are ignored. Line comments starting with
        /// `#` are supported
        /// NOTE: This feature is not permanent, it will be removed once a solid scripting system
        /// is implemented.
        #[clap(long, short, verbatim_doc_comment)]
        command_file: Option<Utf8PathBuf>,
        /// Alias for --`command_file` to support `VUnit`
        #[clap(long)]
        script: Option<Utf8PathBuf>,

        #[clap(long, short)]
        /// Load previously saved state file
        state_file: Option<Utf8PathBuf>,

        #[clap(long, action)]
        /// Port for WCP to connect to
        wcp_initiate: Option<u16>,

        #[command(subcommand)]
        command: Option<Commands>,
    }

    impl Args {
        pub fn command_file(&self) -> Option<&Utf8PathBuf> {
            match (&self.command_file, &self.script) {
                (Some(_), Some(_)) => {
                    error!("At most one of --command_file and --script can be used");
                    None
                }
                (Some(cf), None) => Some(cf),
                (None, Some(sc)) => Some(sc),
                (None, None) => None,
            }
        }
    }

    #[allow(dead_code)] // NOTE: Only used in desktop version
    fn startup_params_from_args(args: Args) -> StartupParams {
        let startup_commands = args
            .command_file()
            .map(read_command_file)
            .unwrap_or_default();
        StartupParams {
            waves: args.wave_file.map(|s| string_to_wavesource(&s)),
            wcp_initiate: args.wcp_initiate,
            startup_commands,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn main() -> Result<()> {
        use libsurfer::state::UserState;
        #[cfg(feature = "wasm_plugins")]
        use libsurfer::translation::wasm_translator::discover_wasm_translators;
        simple_eyre::install()?;

        logs::start_logging()?;

        std::panic::set_hook(Box::new(panic_handler));

        // https://tokio.rs/tokio/topics/bridging
        // We want to run the gui in the main thread, but some long running tasks like
        // loading VCDs should be done asynchronously. We can't just use std::thread to
        // do that due to wasm support, so we'll start a tokio runtime
        let runtime = tokio::runtime::Builder::new_current_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();

        // parse arguments
        let args = Args::parse();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(Commands::Server {
            port,
            bind_address,
            token,
            file,
        }) = args.command
        {
            let config = SystemState::new()?.user.config;

            // Use CLI override if provided, otherwise use config setting
            let bind_addr = bind_address.unwrap_or(config.server.bind_address);
            let port = port.unwrap_or(config.server.port);

            let res = runtime.block_on(surver::surver_main(port, bind_addr, token, &[file], None));
            return res;
        }

        let _enter = runtime.enter();

        std::thread::spawn(move || {
            runtime.block_on(async {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                }
            });
        });

        let state_file = args.state_file.clone();
        let startup_params = startup_params_from_args(args);
        let waves = startup_params.waves.clone();

        let state = match &state_file {
            Some(file) => std::fs::read_to_string(file)
                .with_context(|| format!("Failed to read state from {file}"))
                .and_then(|content| {
                    ron::from_str::<UserState>(&content)
                        .with_context(|| format!("Failed to decode state from {file}"))
                })
                .map(SystemState::from)
                .map(|mut s| {
                    s.user.state_file = Some(file.into());
                    s
                })
                .or_else(|e| {
                    error!("Failed to read state file. Opening fresh session\n{e:#?}");
                    SystemState::new()
                })?,
            None => SystemState::new()?,
        }
        .with_params(startup_params);

        #[cfg(feature = "wasm_plugins")]
        {
            // Not using batch commands here as we want to start processing wasm plugins
            // as soon as we start up, no need to wait for the waveform to load
            let sender = state.channels.msg_sender.clone();
            for message in discover_wasm_translators() {
                if let Err(e) = sender.send(message) {
                    error!("Failed to send message: {e}");
                }
            }
        }
        // install a file watcher that emits a `SuggestReloadWaveform` message
        // whenever the user-provided file changes.
        let _watcher = match waves {
            Some(WaveSource::File(path)) => {
                let sender = state.channels.msg_sender.clone();
                FileWatcher::new(&path, move || {
                    if let Err(e) = sender.send(Message::SuggestReloadWaveform) {
                        error!("Message ReloadWaveform did not send:\n{e}");
                    }
                    // Force refresh UI to process messages. Otherwise, it is
                    // deferred until a UI event occurs (like mouseover)
                    if let Some(ctx) = EGUI_CONTEXT.read().unwrap().as_ref() {
                        ctx.request_repaint();
                    }
                })
                .inspect_err(|err| error!("Cannot set up the file watcher:\n{err}"))
                .ok()
            }
            _ => None,
        };
        let icon = image::load_from_memory_with_format(
            include_bytes!("../assets/com.gitlab.surferproject.surfer.png"),
            image::ImageFormat::Png,
        )
        .expect("Failed to open icon path")
        .to_rgba8();
        let (icon_width, icon_height) = icon.dimensions();
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_app_id("org.surfer-project.surfer")
                .with_title("Surfer")
                .with_icon(egui::viewport::IconData {
                    rgba: icon.into_raw(),
                    width: icon_width,
                    height: icon_height,
                })
                .with_inner_size(Vec2::new(
                    state.user.config.layout.window_width as f32,
                    state.user.config.layout.window_height as f32,
                )),
            ..Default::default()
        };

        eframe::run_native("Surfer", options, Box::new(|cc| Ok(run_egui(cc, state)?))).unwrap();

        Ok(())
    }

    fn panic_handler(info: &std::panic::PanicHookInfo) {
        let backtrace = std::backtrace::Backtrace::force_capture();

        eprintln!();
        eprintln!("Surfer crashed due to a panic 😞");
        eprintln!("Please report this issue at https://gitlab.com/surfer-project/surfer/-/issues");
        eprintln!();
        eprintln!("Some notes on reports:");
        eprintln!(
            "We are happy about any reports, but it makes it much easier for us to fix issues if you:",
        );
        eprintln!(" - Include the information below");
        eprintln!(" - Try to reproduce the issue to give us steps on how to reproduce the issue");
        eprintln!(" - Include (minimal) waveform file and state file you used");
        eprintln!("   (you can upload those confidentially, for the surfer team only)");
        eprintln!();

        let location = info.location().unwrap();
        let msg = if let Some(msg) = info.payload().downcast_ref::<&str>() {
            (*msg).to_string()
        } else if let Some(msg) = info.payload().downcast_ref::<String>() {
            msg.clone()
        } else {
            "<panic message not a string>".to_owned()
        };

        eprintln!(
            "Surfer version: {} (git: {})",
            env!("CARGO_PKG_VERSION"),
            env!("VERGEN_GIT_DESCRIBE"),
        );
        eprintln!(
            "thread '{}' ({:?}) panicked at {}:{}:{:?}",
            std::thread::current().name().unwrap_or("unknown"),
            std::thread::current().id(),
            location.file(),
            location.line(),
            location.column(),
        );
        eprintln!("  {msg}");
        eprintln!();
        eprintln!("backtrace:");
        eprintln!("{backtrace}");
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn command_file_prefers_single_sources() {
            // Only --command_file
            let args = Args::parse_from(["surfer", "--command-file", "C:/tmp/cmds.sucl"]);
            let cf = args.command_file().unwrap();
            assert!(cf.ends_with("cmds.sucl"));

            // Only --script
            let args = Args::parse_from(["surfer", "--script", "C:/tmp/scr.sucl"]);
            let cf = args.command_file().unwrap();
            assert!(cf.ends_with("scr.sucl"));
        }

        #[test]
        fn command_file_conflict_returns_none() {
            let args = Args::parse_from([
                "surfer",
                "--command-file",
                "C:/tmp/cmds.sucl",
                "--script",
                "C:/tmp/scr.sucl",
            ]);
            assert!(args.command_file().is_none());
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod main_impl {
    use eframe::wasm_bindgen::JsCast;
    use eframe::web_sys;
    use libsurfer::logs;
    use libsurfer::wasm_api::WebHandle;

    // Calling main is not the intended way to start surfer, instead, it should be
    // started by `wasm_api::WebHandle`
    pub(crate) fn main() -> eyre::Result<()> {
        simple_eyre::install()?;

        logs::start_logging()?;

        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        wasm_bindgen_futures::spawn_local(async {
            let wh = WebHandle::new();
            wh.start(canvas).await.expect("Failed to start surfer");
        });

        Ok(())
    }
}

fn main() -> eyre::Result<()> {
    main_impl::main()
}

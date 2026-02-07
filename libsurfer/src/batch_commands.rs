use camino::Utf8PathBuf;
use eyre::Context as _;
use futures::FutureExt as _;
use tracing::{error, info, trace};

use crate::{
    SystemState,
    async_util::perform_async_work,
    channels::checked_send,
    command_parser::get_parser,
    fzcmd::parse_command,
    message::Message,
    wave_source::{LoadProgress, LoadProgressStatus},
};

impl SystemState {
    /// After user messages are addressed, we try to execute batch commands as they are ready to run
    pub(crate) fn handle_batch_commands(&mut self) {
        let mut should_exit = false;
        // we only execute commands while we aren't waiting for background operations to complete
        while self.can_start_batch_command() {
            if let Some(cmd) = self.batch_messages.pop_front() {
                if matches!(cmd, Message::Exit) {
                    should_exit = true;
                }
                info!("Applying batch command: {cmd:?}");
                self.update(cmd);
            } else {
                break; // no more messages
            }
        }

        // if there are no messages and all operations have completed, we are done
        if !self.batch_messages_completed
            && self.batch_messages.is_empty()
            && self.can_start_batch_command()
        {
            self.batch_messages_completed = true;

            if should_exit {
                info!("Exiting due to batch command");
                let sender = self.channels.msg_sender.clone();
                checked_send(&sender, Message::Exit);
            }
        }
    }

    /// Returns whether it is OK to start a new batch command.
    pub(crate) fn can_start_batch_command(&self) -> bool {
        // if the progress tracker is none -> all operations have completed
        self.progress_tracker.is_none()
    }

    /// Returns true once all batch commands have been completed and their effects are all executed.
    pub fn batch_commands_completed(&self) -> bool {
        debug_assert!(
            self.batch_messages_completed || !self.batch_messages.is_empty(),
            "completed implies no commands"
        );
        self.batch_messages_completed
    }

    pub fn add_batch_commands<I: IntoIterator<Item = String>>(&mut self, commands: I) {
        let parsed = self.parse_batch_commands(commands);
        for msg in parsed {
            self.batch_messages.push_back(msg);
            self.batch_messages_completed = false;
        }
    }

    pub fn add_batch_messages<I: IntoIterator<Item = Message>>(&mut self, messages: I) {
        for msg in messages {
            self.batch_messages.push_back(msg);
            self.batch_messages_completed = false;
        }
    }

    pub fn add_batch_message(&mut self, msg: Message) {
        self.add_batch_messages([msg]);
    }

    pub fn parse_batch_commands<I: IntoIterator<Item = String>>(
        &mut self,
        cmds: I,
    ) -> Vec<Message> {
        trace!("Parsing batch commands");

        cmds
            .into_iter()
            // Add line numbers
            .enumerate()
            // trace
            .map(|(no, line)| {
                trace!("{no: >2} {line}");
                (no, line)
            })
            // Make the line numbers start at 1 as is tradition
            .map(|(no, line)| (no + 1, line))
            .map(|(no, line)| (no, line.trim().to_string()))
            // NOTE: Safe unwrap. Split will always return one element
            .map(|(no, line)| (no, line.split('#').next().unwrap().to_string()))
            .filter(|(_no, line)| !line.is_empty())
            .flat_map(|(no, line)| {
                line.split(';')
                    .map(|cmd| (no, cmd.trim().to_string()))
                    .collect::<Vec<_>>()
            })
            .filter_map(|(no, command)| {
                if command.starts_with("run_command_file ") {
                    // Load commands from other file in place, otherwise they will be
                    // loaded when the corresponding message is processed, leading to
                    // a different position in the processing than expected.
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if let Some(path_str) = command.split_ascii_whitespace().nth(1) {
                            match Utf8PathBuf::from_path_buf(path_str.into()) {
                                Ok(utf8_path) => {
                                    self.add_batch_commands(read_command_file(&utf8_path));
                                }
                                Err(_) => {
                                    error!("Invalid UTF-8 path in run_command_file on line {no}: {path_str}");
                                }
                            }
                        } else {
                            error!("Missing file path in run_command_file command on line {no}");
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    error!("Cannot use run_command_file in command files running on WASM");
                    None
                } else {
                    parse_command(&command, get_parser(self))
                        .map_err(|e| {
                            error!("Error on batch commands line {no}: {e:#?}");
                            e
                        })
                        .ok()
                }
            })
            .collect::<Vec<_>>()
    }

    pub fn load_commands_from_url(&mut self, url: String) {
        let sender = self.channels.msg_sender.clone();
        let url_ = url.clone();
        perform_async_work(async move {
            let maybe_response = reqwest::get(&url)
                .map(|e| e.with_context(|| format!("Failed fetch download {url}")))
                .await;
            let response: reqwest::Response = match maybe_response {
                Ok(r) => r,
                Err(e) => {
                    checked_send(&sender, Message::Error(e));
                    return;
                }
            };

            // load the body to get at the file
            let bytes = response
                .bytes()
                .map(|e| e.with_context(|| format!("Failed to download {url}")))
                .await;

            let msg = match bytes {
                Ok(b) => Message::CommandFileDownloaded(url, b),
                Err(e) => Message::Error(e),
            };
            checked_send(&sender, msg);
        });

        self.progress_tracker = Some(LoadProgress::new(LoadProgressStatus::Downloading(url_)));
    }
}

#[must_use]
pub fn read_command_file(cmd_file: &Utf8PathBuf) -> Vec<String> {
    std::fs::read_to_string(cmd_file)
        .map_err(|e| error!("Failed to read commands from {cmd_file}. {e:#?}"))
        .ok()
        .map(|file_content| file_content.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

#[must_use]
pub fn read_command_bytes(bytes: Vec<u8>) -> Vec<String> {
    String::from_utf8(bytes)
        .map_err(|e| error!("Failed to read commands from file. {e:#?}"))
        .ok()
        .map(|file_content| file_content.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

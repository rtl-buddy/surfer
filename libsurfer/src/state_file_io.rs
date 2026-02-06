use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use camino::Utf8PathBuf;
use eyre::Context;
use rfd::FileHandle;
use tracing::error;

#[cfg(not(target_arch = "wasm32"))]
use crate::async_util::perform_async_work;

use crate::{
    SystemState, async_util::AsyncJob, message::Message, wave_source::STATE_FILE_EXTENSION,
};

impl SystemState {
    #[cfg(target_arch = "wasm32")]
    pub fn load_state_file(&mut self, path: Option<PathBuf>) {
        if path.is_some() {
            return;
        }
        let message = move |bytes: Vec<u8>| match ron::de::from_bytes(&bytes)
            .context("Failed loading state file")
        {
            Ok(s) => vec![Message::LoadState(s, path)],
            Err(e) => {
                error!("Failed to load state: {e:#?}");
                vec![]
            }
        };
        self.file_dialog_open(
            "Load state",
            (
                format!("Surfer state files (*.{STATE_FILE_EXTENSION})"),
                vec![STATE_FILE_EXTENSION.to_string()],
            ),
            message,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_state_file(&mut self, path: Option<PathBuf>) {
        let messages = move |path: PathBuf| {
            let source = if let Ok(p) = Utf8PathBuf::from_path_buf(path.clone()) {
                p
            } else {
                let err = eyre::eyre!("File path '{}' contains invalid UTF-8", path.display());
                error!("{err:#?}");
                return vec![Message::Error(err)];
            };

            match std::fs::read(source.as_std_path()) {
                Ok(bytes) => match ron::de::from_bytes(&bytes)
                    .context(format!("Failed loading {}", source.as_str()))
                {
                    Ok(s) => vec![Message::LoadState(s, Some(path))],
                    Err(e) => {
                        error!("Failed to load state: {e:#?}");
                        vec![Message::Error(e)]
                    }
                },
                Err(e) => {
                    error!("Failed to load state file: {path:#?} {e:#?}");
                    vec![Message::Error(eyre::eyre!(
                        "Failed to read state file '{}': {e}",
                        path.display()
                    ))]
                }
            }
        };
        if let Some(path) = path {
            let sender = self.channels.msg_sender.clone();
            for message in messages(path) {
                if let Err(e) = sender.send(message) {
                    error!("Failed to send message: {e}");
                }
            }
        } else {
            self.file_dialog_open(
                "Load state",
                (
                    format!("Surfer state files (*.{STATE_FILE_EXTENSION})"),
                    vec![STATE_FILE_EXTENSION.to_string()],
                ),
                messages,
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_state_file(&mut self, path: Option<PathBuf>) {
        let Some(encoded) = self.encode_state() else {
            return;
        };

        let messages = async move |destination: FileHandle| {
            destination
                .write(encoded.as_bytes())
                .await
                .map_err(|e| error!("Failed to write state to {destination:#?} {e:#?}"))
                .ok();
            vec![
                Message::SetStateFile(destination.path().into()),
                Message::AsyncDone(AsyncJob::SaveState),
            ]
        };
        if let Some(path) = path {
            let sender = self.channels.msg_sender.clone();
            perform_async_work(async move {
                for message in messages(path.into()).await {
                    if let Err(e) = sender.send(message) {
                        error!("Failed to send message: {e}");
                    }
                }
            });
        } else {
            self.file_dialog_save(
                "Save state",
                (
                    format!("Surfer state files (*.{STATE_FILE_EXTENSION})"),
                    vec![STATE_FILE_EXTENSION.to_string()],
                ),
                messages,
            );
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save_state_file(&mut self, path: Option<PathBuf>) {
        if path.is_some() {
            return;
        }
        let Some(encoded) = self.encode_state() else {
            return;
        };
        let messages = async move |destination: FileHandle| {
            destination
                .write(encoded.as_bytes())
                .await
                .map_err(|e| error!("Failed to write state to {destination:#?} {e:#?}"))
                .ok();
            vec![Message::AsyncDone(AsyncJob::SaveState)]
        };
        self.file_dialog_save(
            "Save state",
            (
                format!("Surfer state files (*.{STATE_FILE_EXTENSION})"),
                vec![STATE_FILE_EXTENSION.to_string()],
            ),
            messages,
        );
    }

    pub fn encode_state(&self) -> Option<String> {
        let opt = ron::Options::default();

        opt.to_string_pretty(&self.user, ron::ser::PrettyConfig::default())
            .context("Failed to encode state")
            .map_err(|e| error!("Failed to encode state. {e:#?}"))
            .ok()
    }

    pub fn load_state_from_bytes(&mut self, bytes: Vec<u8>) {
        match ron::de::from_bytes(&bytes).context("Failed loading state from bytes") {
            Ok(s) => {
                let sender = self.channels.msg_sender.clone();
                if let Err(e) = sender.send(Message::LoadState(s, None)) {
                    error!("Failed to send message: {e}");
                }
            }
            Err(e) => {
                error!("Failed to load state: {e:#?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StartupParams;

    #[test]
    fn test_encode_state() {
        let state = SystemState::new_default_config()
            .unwrap()
            .with_params(StartupParams::default());
        let encoded = state.encode_state();
        assert!(encoded.is_some());
        let encoded = encoded.unwrap();
        assert!(encoded.contains("show_about"));
    }

    #[test]
    fn test_load_state_from_bytes() {
        let mut state = SystemState::new_default_config()
            .unwrap()
            .with_params(StartupParams::default());
        let encoded = state.encode_state().unwrap();
        let bytes = encoded.as_bytes().to_vec();

        state.load_state_from_bytes(bytes);

        let msg = state.channels.msg_receiver.try_recv().unwrap();
        match msg {
            Message::LoadState(..) => {}
            _ => panic!("Expected LoadState message, got {:?}", msg),
        }
    }
}

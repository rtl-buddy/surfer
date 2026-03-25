use std::future::Future;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use camino::Utf8PathBuf;
use rfd::{AsyncFileDialog, FileHandle};
use serde::Deserialize;

use crate::SystemState;
use crate::async_util::perform_async_work;
use crate::channels::checked_send_many;
use crate::message::Message;
use crate::transactions::TRANSACTIONS_FILE_EXTENSION;

#[derive(Debug, Deserialize)]
pub enum OpenMode {
    Open,
    Switch,
}

impl SystemState {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn file_dialog_open<F>(
        &mut self,
        title: &'static str,
        filter: (String, Vec<String>),
        messages: F,
    ) where
        F: FnOnce(PathBuf) -> Vec<Message> + Send + 'static,
    {
        let sender = self.channels.msg_sender.clone();

        perform_async_work(async move {
            if let Some(file) = create_file_dialog(filter, title, None).pick_file().await {
                checked_send_many(&sender, messages(file.path().to_path_buf()));
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn get_current_fst_dir(&self) -> Option<PathBuf> {
        self.user
            .waves
            .as_ref()
            .and_then(|waves| match &waves.source {
                crate::wave_source::WaveSource::File(path) => {
                    path.parent().map(|p| p.to_path_buf().into())
                }
                _ => None,
            })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn file_dialog_open<F>(
        &mut self,
        title: &'static str,
        filter: (String, Vec<String>),
        messages: F,
    ) where
        F: FnOnce(Vec<u8>) -> Vec<Message> + 'static,
    {
        let sender = self.channels.msg_sender.clone();

        perform_async_work(async move {
            if let Some(file) = create_file_dialog(filter, title, None).pick_file().await {
                checked_send_many(&sender, messages(file.read().await));
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn file_dialog_save<F, Fut>(
        &mut self,
        title: &'static str,
        filter: (String, Vec<String>),
        messages: F,
    ) where
        F: FnOnce(FileHandle) -> Fut + Send + 'static,
        Fut: Future<Output = Vec<Message>> + Send + 'static,
    {
        let sender = self.channels.msg_sender.clone();
        let default_dir = self.get_current_fst_dir();
        perform_async_work(async move {
            if let Some(file) = create_file_dialog(filter, title, default_dir)
                .save_file()
                .await
            {
                checked_send_many(&sender, messages(file).await);
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub fn file_dialog_save<F, Fut>(
        &mut self,
        title: &'static str,
        filter: (String, Vec<String>),
        messages: F,
    ) where
        F: FnOnce(FileHandle) -> Fut + 'static,
        Fut: Future<Output = Vec<Message>> + 'static,
    {
        let sender = self.channels.msg_sender.clone();

        perform_async_work(async move {
            if let Some(file) = create_file_dialog(filter, title).save_file().await {
                checked_send_many(&sender, messages(file).await);
            }
        });
    }

    pub fn open_file_dialog(&mut self, mode: OpenMode) {
        let load_options = (mode, self.user.config.behavior.keep_during_reload).into();

        #[cfg(not(target_arch = "wasm32"))]
        let message = move |file: PathBuf| match Utf8PathBuf::from_path_buf(file.clone()) {
            Ok(utf8_path) => vec![Message::LoadFile(utf8_path, load_options)],
            Err(_) => {
                vec![Message::Error(eyre::eyre!(
                    "File path '{}' contains invalid UTF-8",
                    file.display()
                ))]
            }
        };

        #[cfg(target_arch = "wasm32")]
        let message = move |file: Vec<u8>| vec![Message::LoadFromData(file, load_options)];

        self.file_dialog_open(
            "Open waveform file",
            (
                "Waveform/Transaction-files (*.vcd, *.fst, *.ghw, *.ftr)".to_string(),
                vec![
                    "vcd".to_string(),
                    "fst".to_string(),
                    "ghw".to_string(),
                    TRANSACTIONS_FILE_EXTENSION.to_string(),
                ],
            ),
            message,
        );
    }

    pub fn open_command_file_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        let message = move |file: PathBuf| match Utf8PathBuf::from_path_buf(file.clone()) {
            Ok(utf8_path) => vec![Message::LoadCommandFile(utf8_path)],
            Err(_) => {
                vec![Message::Error(eyre::eyre!(
                    "File path '{}' contains invalid UTF-8",
                    file.display()
                ))]
            }
        };

        #[cfg(target_arch = "wasm32")]
        let message = move |file: Vec<u8>| vec![Message::LoadCommandFromData(file)];

        self.file_dialog_open(
            "Open command file",
            (
                "Command-file (*.sucl)".to_string(),
                vec!["sucl".to_string()],
            ),
            message,
        );
    }

    #[cfg(feature = "python")]
    pub fn open_python_file_dialog(&mut self) {
        self.file_dialog_open(
            "Open Python translator file",
            ("Python files (*.py)".to_string(), vec!["py".to_string()]),
            |file| match Utf8PathBuf::from_path_buf(file.clone()) {
                Ok(utf8_path) => vec![Message::LoadPythonTranslator(utf8_path)],
                Err(_) => {
                    vec![Message::Error(eyre::eyre!(
                        "File path '{}' contains invalid UTF-8",
                        file.display()
                    ))]
                }
            },
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn create_file_dialog(
    filter: (String, Vec<String>),
    title: &'static str,
    default_dir: Option<PathBuf>,
) -> AsyncFileDialog {
    let mut dialog = AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter.0, &filter.1)
        .add_filter("All files", &["*"]);

    if let Some(dir) = default_dir {
        dialog = dialog.set_directory(dir);
    }
    dialog
}

#[cfg(target_os = "macos")]
fn create_file_dialog(
    filter: (String, Vec<String>),
    title: &'static str,
    default_dir: Option<PathBuf>,
) -> AsyncFileDialog {
    let mut dialog = AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter.0, &filter.1);
    if let Some(dir) = default_dir {
        dialog = dialog.set_directory(dir);
    }
    dialog
}

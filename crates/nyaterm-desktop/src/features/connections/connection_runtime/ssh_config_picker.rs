//! "Select a host from ssh config" picker used by the new-connection editor.
//!
//! The bulk importer (`savedConnections` import dialog) turns an entire OpenSSH
//! config into saved connections. This flow is deliberately narrower: the user
//! opens a new SSH connection, picks one host alias, and only that host's basic
//! endpoint fills the form. Nothing is written to the store here.

use std::path::PathBuf;

use gpui::{Context, PathPromptOptions, SharedString, Window};
use rust_i18n::t;

use nyaterm_core::SshConfigHost;

use crate::features::NyaTermApp;
use crate::features::runtime_jobs::await_blocking_job;
use crate::features::text_inputs::TextInputSetup;

/// Registry id of the picker's filter box. Shared with the view.
pub(in crate::features) const SSH_CONFIG_PICKER_SEARCH_INPUT: &str =
    "connection-ssh-config-picker.search";

impl NyaTermApp {
    /// Open the host picker for the editor's draft. Prefers the default
    /// `~/.ssh/config`; when it does not exist, falls back to a file picker so
    /// the user can point at another config file.
    pub(in crate::features) fn open_connection_editor_ssh_config_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = nyaterm_core::default_ssh_config_path() else {
            self.shell
                .set_status(t!("dialog.sshConfigPickerNoHome").to_string());
            cx.notify();
            return;
        };
        self.prepare_connection_editor_ssh_config_search(window, cx);
        if path.is_file() {
            self.load_connection_editor_ssh_config_picker(path, cx);
        } else {
            self.prompt_connection_editor_ssh_config_file(window, cx);
        }
    }

    /// Build (or reset) the filter box and put the caret in it so the user can
    /// start typing immediately. The registry seeds text on first use, so the
    /// previous query is forgotten before creating it again.
    fn prepare_connection_editor_ssh_config_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.forget_text_inputs("connection-ssh-config-picker.");
        let field = self.text_input(
            SSH_CONFIG_PICKER_SEARCH_INPUT,
            "",
            TextInputSetup::placeholder(t!("dialog.sshConfigPickerSearchPlaceholder")),
            cx,
        );
        window.focus(&field.read(cx).focus_handle(), cx);
    }

    fn prompt_connection_editor_ssh_config_file(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                t!("dialog.sshConfigPickerSelectFile").to_string(),
            )),
        });
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            let picked = receiver.await;
            let _ = window.update(cx, |_, _window, cx| {
                let _ = this.update(cx, |app, cx| {
                    if let Ok(Ok(Some(paths))) = picked
                        && let Some(path) = paths.into_iter().next()
                    {
                        app.load_connection_editor_ssh_config_picker(path, cx);
                    }
                });
            });
        })
        .detach();
        cx.notify();
    }

    fn load_connection_editor_ssh_config_picker(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let generation = self
            .connection_state
            .begin_editor_ssh_config_picker_load(path.clone());
        let scheduler = self.blocking_jobs.clone();
        cx.spawn(async move |this, cx| {
            let task = scheduler.submit_task("ssh-config-hosts", move |_| {
                nyaterm_core::list_ssh_config_hosts(&path).map_err(|error| error.to_string())
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if this
                    .connection_state
                    .set_editor_ssh_config_hosts(generation, result)
                {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::features) fn close_connection_editor_ssh_config_picker(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.connection_state.close_editor_ssh_config_picker() {
            self.forget_text_inputs("connection-ssh-config-picker.");
            cx.notify();
        }
    }

    /// Feed the filter box's text into the picker. Kept separate from the field
    /// so the registry subscription owns the caret and we only mirror the query.
    pub(in crate::features) fn apply_connection_editor_ssh_config_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if self.connection_state.set_editor_ssh_config_search(text) {
            cx.notify();
        }
    }

    /// Apply the picked host to the draft and close the picker.
    pub(in crate::features) fn apply_connection_editor_ssh_config_host(
        &mut self,
        host: SshConfigHost,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_state
            .apply_editor_ssh_config_host(&host, cx)
        {
            self.close_connection_editor_ssh_config_picker(cx);
            self.shell.set_status(
                t!("dialog.sshConfigPickerApplied", host = host.alias.clone()).to_string(),
            );
            cx.notify();
        }
    }
}

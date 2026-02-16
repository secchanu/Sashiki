use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use lsp_types::GotoDefinitionResponse;
use url::Url;

use crate::lsp::client::LspClient;

#[derive(Clone, Hash, Eq, PartialEq)]
pub struct WorkspaceId {
    pub root: PathBuf,
    pub server_id: String,
}

pub struct LspManager {
    pub servers: HashMap<WorkspaceId, LspClient>,
    pub opened_docs: HashMap<WorkspaceId, HashMap<Url, u64>>,
}

impl LspManager {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            opened_docs: HashMap::new(),
        }
    }

    pub async fn ensure_server(
        &mut self,
        id: &WorkspaceId,
        command: &str,
        args: &[&str],
    ) -> Result<&mut LspClient> {
        if !self.servers.contains_key(id) {
            let mut client = LspClient::start(command, args, &id.root).await?;
            let root_uri = Url::from_directory_path(&id.root).map_err(|_| {
                anyhow!(
                    "failed to convert workspace root to file URI: {}",
                    id.root.display()
                )
            })?;
            if let Err(e) = client.initialize(root_uri).await {
                let stderr = client.stderr_output();
                let exit_status = client.server_process.try_status().ok().flatten();
                let mut context = format!("failed to initialize LSP server '{}'", id.server_id);
                if let Some(status) = exit_status {
                    context.push_str(&format!(" (process exited with {status})"));
                }
                if !stderr.is_empty() {
                    context.push_str(&format!("\nServer stderr:\n{}", stderr.trim()));
                }
                // 初期化失敗時にサーバープロセスが残留しないようshutdown/killする
                let _ = client.shutdown().await;
                return Err(e.context(context));
            }
            self.servers.insert(id.clone(), client);
        }

        self.servers
            .get_mut(id)
            .ok_or_else(|| anyhow!("LSP server not found after ensure_server"))
    }

    pub async fn sync_document(
        &mut self,
        id: &WorkspaceId,
        uri: Url,
        language_id: &str,
        text: &str,
    ) -> Result<()> {
        let new_hash = content_hash(text);

        let existing_hash = self
            .opened_docs
            .get(id)
            .and_then(|opened| opened.get(&uri))
            .copied();

        let client = self.servers.get_mut(id).ok_or_else(|| {
            anyhow!(
                "LSP server is not running for workspace '{}'; call ensure_server first",
                id.server_id
            )
        })?;

        match existing_hash {
            Some(hash) if hash == new_hash => return Ok(()),
            Some(_) => {
                client
                    .did_close(uri.clone())
                    .await
                    .context("failed to send didClose")?;
                // didClose成功後にトラッキングを削除。
                // didOpenが失敗しても次回呼び出しで再openされる。
                if let Some(opened) = self.opened_docs.get_mut(id) {
                    opened.remove(&uri);
                }
                client
                    .did_open(uri.clone(), language_id, text)
                    .await
                    .context("failed to send didOpen")?;
            }
            None => {
                client
                    .did_open(uri.clone(), language_id, text)
                    .await
                    .context("failed to send didOpen")?;
            }
        }

        self.opened_docs
            .entry(id.clone())
            .or_default()
            .insert(uri, new_hash);
        Ok(())
    }

    pub async fn definition(
        &mut self,
        id: &WorkspaceId,
        uri: Url,
        line: u32,
        character: u32,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let client = self.servers.get_mut(id).ok_or_else(|| {
            anyhow!(
                "LSP server is not running for workspace '{}'; call ensure_server first",
                id.server_id
            )
        })?;
        client.definition(uri, line, character).await
    }

    /// Drain servers and opened_docs, returning the old servers for caller to shut down.
    /// This allows the caller to release the lock before performing slow shutdown I/O.
    pub fn take_servers(&mut self) -> HashMap<WorkspaceId, LspClient> {
        self.opened_docs.clear();
        std::mem::take(&mut self.servers)
    }
}

fn content_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

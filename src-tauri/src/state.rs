use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::config::Config;
use crate::history::TranslationEntry;
use crate::translator::{self, Translator};

const HISTORY_LIMIT: usize = 200;

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub config: Mutex<Config>,
    pub translator: Mutex<Arc<dyn Translator>>,
    pub history: Mutex<VecDeque<TranslationEntry>>,
    /// 設定変更時にウォッチャーへ再起動を通知する
    pub watcher_restart: Arc<Notify>,
    /// 進行中の翻訳をキャンセルするための oneshot Sender
    pub cancel_sender: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// 進行中の OSC チャンク送信をキャンセルするための oneshot Sender
    pub osc_cancel_sender: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let translator = translator::build_translator(&config);
        Self {
            translator: Mutex::new(translator),
            history: Mutex::new(VecDeque::with_capacity(HISTORY_LIMIT)),
            watcher_restart: Arc::new(Notify::new()),
            cancel_sender: Mutex::new(None),
            osc_cancel_sender: Mutex::new(None),
            config: Mutex::new(config),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.lock().expect("config lock poisoned").is_enabled
    }

    pub fn osc_enabled(&self) -> bool {
        self.config.lock().expect("config lock poisoned").osc_enabled
    }

    pub fn get_translator(&self) -> Arc<dyn Translator> {
        Arc::clone(&self.translator.lock().expect("translator lock poisoned"))
    }

    pub fn push_history(&self, entry: TranslationEntry) {
        let mut history = self.history.lock().expect("history lock poisoned");
        if history.len() >= HISTORY_LIMIT {
            history.pop_front();
        }
        history.push_back(entry);
    }

    pub fn get_history(&self) -> Vec<TranslationEntry> {
        let history = self.history.lock().expect("history lock poisoned");
        history.iter().cloned().rev().collect()
    }

    pub fn clear_history(&self) {
        let mut history = self.history.lock().expect("history lock poisoned");
        history.clear();
    }

    /// 設定を更新し、翻訳エンジンを差し替え、ウォッチャーに再起動を通知する。
    pub fn apply_config(&self, new_config: Config) {
        let new_translator = translator::build_translator(&new_config);
        {
            let mut t = self.translator.lock().expect("translator lock poisoned");
            *t = new_translator;
        }
        {
            let mut c = self.config.lock().expect("config lock poisoned");
            *c = new_config;
        }
        self.watcher_restart.notify_one();
    }
}

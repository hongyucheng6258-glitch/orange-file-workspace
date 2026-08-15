use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::events::EVENT_RESOURCE_CHANGED;

/// 监听 managed-files 目录的文件变化，向前端广播刷新事件。
/// 事件先做 500ms 去抖合并，避免高频写文件时刷屏。
pub fn start_managed_watcher(app: AppHandle) {
    let state = app.state::<AppState>();
    let watch_dir = state.data_dir.join("managed-files");

    if !watch_dir.exists() {
        return;
    }

    std::thread::spawn(move || {
        use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let tx2 = tx.clone();

        let mut watcher = match RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    // 只关心内容相关的变化，忽略元数据/访问类事件
                    let relevant = matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    );
                    if relevant {
                        let _ = tx.send(());
                    }
                }
            },
            Config::default(),
        ) {
            Ok(w) => w,
            Err(_) => return,
        };

        if watcher
            .watch(&watch_dir, RecursiveMode::Recursive)
            .is_err()
        {
            return;
        }

        // 去抖：收集信号，500ms 无新事件则广播一次
        let mut pending = false;
        loop {
            match rx.recv() {
                Ok(()) => pending = true,
                Err(_) => break,
            }
            while let Ok(()) = rx.try_recv() {
                pending = true;
            }
            if pending {
                std::thread::sleep(Duration::from_millis(500));
                while let Ok(()) = rx.try_recv() {
                    // 合并窗口内的新事件
                }
                let _ = app.emit(EVENT_RESOURCE_CHANGED, serde_json::json!({ "source": "watcher" }));
                pending = false;
            }
        }
    });
}

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use notify::Watcher;
use tracing::{debug, error, info, warn};

use super::model::Config;
use super::store::ConfigError;

const DEBOUNCE: Duration = Duration::from_millis(200);

enum WatchMessage {
    Event(notify::Result<notify::Event>),
    Shutdown,
}

/// Owns the config watcher worker so the application can stop and join it.
///
/// Dropping the watcher also stops and joins the worker. A worker that panicked
/// is reported as an error through the tracing log because `Drop` cannot return.
pub struct ConfigWatcher {
    control: Option<Sender<WatchMessage>>,
    worker: Option<JoinHandle<()>>,
}

impl ConfigWatcher {
    /// Stops the worker and waits for it to exit. Safe to call more than once.
    pub fn shutdown(&mut self) -> Result<(), ConfigError> {
        if let Some(control) = self.control.take()
            && control.send(WatchMessage::Shutdown).is_err()
        {
            debug!("config watcher worker already stopped");
        }

        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| ConfigError::WatcherPanicked)?;
        }

        Ok(())
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            error!(%error, "failed to stop the config watcher");
        }
    }
}

/// Watches the config file's parent directory and sends each successfully
/// reloaded config. Atomic replacements keep working because the directory,
/// not the file, is watched.
///
/// The path is made absolute without canonicalization, so relative paths match
/// the absolute paths reported by `notify` and a missing file can still be created.
pub fn watch(
    path: &Path,
    initial: Config,
) -> Result<(ConfigWatcher, UnboundedReceiver<Config>), ConfigError> {
    let watch_path = std::path::absolute(path).map_err(|source| ConfigError::Resolve {
        path: path.to_path_buf(),
        source,
    })?;
    let watch_dir = watch_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let (sender, receiver) = mpsc::channel();
    let event_sender = sender.clone();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = event_sender.send(WatchMessage::Event(event));
    })?;
    watcher.watch(watch_dir, notify::RecursiveMode::NonRecursive)?;

    let (config_sender, config_receiver) = unbounded();
    let worker = std::thread::Builder::new()
        .name("config-watch".to_string())
        .spawn(move || {
            let _watcher = watcher;
            run(watch_path, initial, receiver, config_sender);
        })?;

    Ok((
        ConfigWatcher {
            control: Some(sender),
            worker: Some(worker),
        },
        config_receiver,
    ))
}

fn run(
    path: PathBuf,
    mut last_valid: Config,
    receiver: Receiver<WatchMessage>,
    config_sender: UnboundedSender<Config>,
) {
    while let Ok(message) = receiver.recv() {
        let event = match message {
            WatchMessage::Shutdown => return,
            WatchMessage::Event(event) => event,
        };

        match event {
            Ok(event) if event_targets_config(&event, &path) => {}
            Ok(_) => continue,
            Err(event_error) => {
                warn!(%event_error, "config watcher event failed");
                continue;
            }
        }

        let mut deadline = Instant::now() + DEBOUNCE;
        loop {
            if config_sender.is_closed() {
                return;
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            match receiver.recv_timeout(remaining) {
                Ok(WatchMessage::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(WatchMessage::Event(Ok(event))) => {
                    deadline = next_deadline(deadline, Instant::now(), &event, &path);
                }
                Ok(WatchMessage::Event(Err(event_error))) => {
                    warn!(%event_error, "config watcher event failed");
                }
                Err(RecvTimeoutError::Timeout) => break,
            }
        }

        if !publish(&path, &mut last_valid, &config_sender) {
            return;
        }
    }
}

fn event_targets_config(event: &notify::Event, path: &Path) -> bool {
    event.paths.iter().any(|event_path| event_path == path)
}

fn next_deadline(deadline: Instant, now: Instant, event: &notify::Event, path: &Path) -> Instant {
    if event_targets_config(event, path) {
        now + DEBOUNCE
    } else {
        deadline
    }
}

fn publish(path: &Path, last_valid: &mut Config, sender: &UnboundedSender<Config>) -> bool {
    match Config::load_existing(path) {
        Ok(loaded) => {
            for warning in &loaded.warnings {
                warn!("{warning}");
            }
            *last_valid = loaded.config;
            info!(config = ?*last_valid, "config reloaded");
            sender.unbounded_send(last_valid.clone()).is_ok()
        }
        Err(load_error) => {
            error!(
                %load_error,
                config = ?last_valid,
                "config reload failed; keeping the last valid config"
            );
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc::TryRecvError;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct Producer {
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl Producer {
        fn stop_and_join(mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = self.handle.take() {
                handle.join().expect("config writer thread");
            }
        }
    }

    impl Drop for Producer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn replace(path: &Path, background: &str) {
        let mut config = Config::default();
        config.appearance.background = background.to_string();
        let temporary = path.with_extension("toml.tmp");
        fs::write(
            &temporary,
            toml::to_string_pretty(&config).expect("serialize config"),
        )
        .expect("write temporary config");
        fs::rename(&temporary, path).expect("replace config atomically");
    }

    fn wait_for_config(receiver: &mut UnboundedReceiver<Config>) -> Config {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match receiver.try_recv() {
                Ok(config) => return config,
                Err(TryRecvError::Closed) => panic!("config channel closed"),
                Err(TryRecvError::Empty) => {}
            }

            assert!(
                Instant::now() < deadline,
                "timed out waiting for a config reload"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn assert_channel_closed(receiver: &mut UnboundedReceiver<Config>) {
        while receiver.try_recv().is_ok() {}
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Closed));
    }

    fn wait_for_background(receiver: &mut UnboundedReceiver<Config>, background: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match receiver.try_recv() {
                Ok(config) if config.appearance.background == background => return,
                Ok(_) => {}
                Err(TryRecvError::Closed) => panic!("config channel closed before {background}"),
                Err(TryRecvError::Empty) => {}
            }

            assert!(
                Instant::now() < deadline,
                "timed out waiting for background {background}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn malformed_reload_keeps_the_last_valid_config_and_sends_nothing() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let mut config = Config::default();
        config.appearance.background = "#112233".to_string();
        fs::write(&path, toml::to_string_pretty(&config).expect("serialize")).expect("write");
        let (sender, mut receiver) = unbounded();

        let mut last_valid = Config::default();
        assert!(publish(&path, &mut last_valid, &sender));
        assert_eq!(last_valid.appearance.background, "#112233");
        assert_eq!(
            receiver
                .try_recv()
                .expect("first reload")
                .appearance
                .background,
            "#112233"
        );

        fs::write(&path, "not valid toml = [").expect("write malformed");
        assert!(publish(&path, &mut last_valid, &sender));
        assert_eq!(last_valid.appearance.background, "#112233");
        assert_eq!(
            receiver.try_recv(),
            Err(TryRecvError::Empty),
            "a malformed reload must not emit a config"
        );
    }

    #[test]
    fn watcher_reloads_after_repeated_atomic_replacements_and_shuts_down() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        Config::load_or_create(&path).expect("create default");
        let (mut watcher, mut receiver) = watch(&path, Config::default()).expect("watch config");

        replace(&path, "#112233");
        assert_eq!(
            wait_for_config(&mut receiver).appearance.background,
            "#112233"
        );

        replace(&path, "#445566");
        assert_eq!(
            wait_for_config(&mut receiver).appearance.background,
            "#445566"
        );

        watcher.shutdown().expect("stop watcher");
        watcher.shutdown().expect("stop watcher again");
    }

    #[test]
    fn watcher_starts_before_the_config_file_exists() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        let (mut watcher, mut receiver) =
            watch(&path, Config::default()).expect("watch missing config");

        replace(&path, "#ABCDEF");

        wait_for_background(&mut receiver, "#ABCDEF");
        watcher.shutdown().expect("stop watcher");
    }

    #[test]
    fn watcher_resolves_a_relative_config_path() {
        let current = std::env::current_dir().expect("current dir");
        let directory = tempfile::tempdir_in(&current).expect("tempdir below current dir");
        let path = directory.path().join("config.toml");
        Config::load_or_create(&path).expect("create default");
        let relative = path.strip_prefix(&current).expect("path below current dir");
        let (mut watcher, mut receiver) =
            watch(relative, Config::default()).expect("watch relative path");

        replace(&path, "#778899");
        assert_eq!(
            wait_for_config(&mut receiver).appearance.background,
            "#778899"
        );

        watcher.shutdown().expect("stop watcher");
        assert_channel_closed(&mut receiver);
    }

    #[test]
    fn watcher_ignores_unrelated_files_in_the_config_directory() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        Config::load_or_create(&path).expect("create default");
        let (mut watcher, mut receiver) = watch(&path, Config::default()).expect("watch config");

        fs::write(directory.path().join("other.toml"), "unrelated = true")
            .expect("write unrelated file");

        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            assert_eq!(
                receiver.try_recv(),
                Err(TryRecvError::Empty),
                "an unrelated file must not emit a config"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        watcher.shutdown().expect("stop watcher");
    }

    #[test]
    fn watcher_debounces_until_matching_writes_settle() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        Config::load_or_create(&path).expect("create default");
        let (mut watcher, mut receiver) = watch(&path, Config::default()).expect("watch config");

        for index in 0..20 {
            let background = if index == 19 { "#444444" } else { "#111111" };
            replace(&path, background);
            std::thread::sleep(Duration::from_millis(40));

            match receiver.try_recv() {
                Err(TryRecvError::Empty) => {}
                Ok(config) => panic!(
                    "emitted {} while matching writes were still unsettled",
                    config.appearance.background
                ),
                Err(TryRecvError::Closed) => {
                    panic!("config channel closed during the matching write burst")
                }
            }
        }

        assert_eq!(
            wait_for_config(&mut receiver).appearance.background,
            "#444444",
            "the first emitted config must be the settled config"
        );

        watcher.shutdown().expect("stop watcher");
    }

    #[test]
    fn matching_events_extend_the_debounce_deadline() {
        let start = Instant::now();
        let path = Path::new("C:/Dockwind/config.toml");
        let deadline = start + DEBOUNCE;
        let event = notify::Event::new(notify::EventKind::Any).add_path(path.to_path_buf());

        let extended = next_deadline(deadline, start + Duration::from_millis(120), &event, path);

        assert_eq!(extended, start + Duration::from_millis(320));
    }

    #[test]
    fn unrelated_events_keep_the_debounce_deadline() {
        let start = Instant::now();
        let path = Path::new("C:/Dockwind/config.toml");
        let deadline = start + DEBOUNCE;
        let event = notify::Event::new(notify::EventKind::Any)
            .add_path(PathBuf::from("C:/Dockwind/other.toml"));

        let kept = next_deadline(deadline, start + Duration::from_millis(120), &event, path);

        assert_eq!(kept, deadline);
    }

    #[test]
    fn shutdown_terminates_during_sustained_matching_events() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        Config::load_or_create(&path).expect("create default");
        let (mut watcher, mut receiver) = watch(&path, Config::default()).expect("watch config");

        let stop = Arc::new(AtomicBool::new(false));
        let writes = Arc::new(AtomicUsize::new(0));
        let producer_stop = Arc::clone(&stop);
        let producer_writes = Arc::clone(&writes);
        let writer_path = path.clone();
        let handle = std::thread::spawn(move || {
            while !producer_stop.load(Ordering::SeqCst) {
                replace(&writer_path, "#102030");
                producer_writes.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let producer = Producer {
            stop,
            handle: Some(handle),
        };
        let writer_deadline = Instant::now() + Duration::from_secs(2);
        while writes.load(Ordering::SeqCst) == 0 {
            assert!(
                Instant::now() < writer_deadline,
                "the config writer did not start"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        let (finished_sender, finished_receiver) = std::sync::mpsc::channel();
        let shutdown = std::thread::spawn(move || {
            watcher
                .shutdown()
                .expect("stop watcher during sustained matching events");
            let _ = finished_sender.send(());
        });

        match finished_receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(()) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("the config watcher did not stop within 1s while matching events continued")
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("the config watcher shutdown thread panicked")
            }
        }

        producer.stop_and_join();
        shutdown.join().expect("shutdown thread");
        assert_channel_closed(&mut receiver);
    }

    #[test]
    fn watcher_stops_when_the_config_receiver_is_dropped() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("config.toml");
        Config::load_or_create(&path).expect("create default");
        let (mut watcher, receiver) = watch(&path, Config::default()).expect("watch config");
        drop(receiver);

        replace(&path, "#0A0B0C");

        let worker = watcher.worker.take().expect("config watcher worker");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.is_finished() {
            assert!(
                Instant::now() < deadline,
                "the config watcher worker did not stop after its receiver was dropped"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        worker.join().expect("config watcher worker panicked");
        watcher
            .shutdown()
            .expect("clean up the stopped config watcher");
    }
}

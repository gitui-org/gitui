use anyhow::Result;
use crossbeam_channel::{unbounded, Sender};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_mini::{
	new_debouncer, DebounceEventResult, Debouncer,
};
use std::{path::Path, thread, time::Duration};

pub struct RepoWatcher {
	receiver: crossbeam_channel::Receiver<()>,
	_watcher: Debouncer<RecommendedWatcher>,
}

impl RepoWatcher {
	pub fn new(workdir: &str) -> Self {
		log::trace!(
			"recommended watcher: {:?}",
			RecommendedWatcher::kind()
		);

		let (tx, rx) = std::sync::mpsc::channel();

		let timeout = Duration::from_secs(2);
		let mut bouncer =
			new_debouncer(timeout, tx).expect("Watch create error");
		bouncer
			.watcher()
			.watch(Path::new(workdir), RecursiveMode::Recursive)
			.expect("Watch error");

		let (out_tx, out_rx) = unbounded();

		thread::spawn(move || {
			if let Err(e) = Self::forwarder(&rx, &out_tx) {
				//maybe we need to restart the forwarder now?
				log::error!("notify receive error: {e}");
			}
		});

		Self {
			receiver: out_rx,
			_watcher: bouncer,
		}
	}

	///
	pub fn receiver(&self) -> crossbeam_channel::Receiver<()> {
		self.receiver.clone()
	}

	fn forwarder(
		receiver: &std::sync::mpsc::Receiver<DebounceEventResult>,
		sender: &Sender<()>,
	) -> Result<()> {
		loop {
			let ev = receiver.recv()?;

			if let Ok(ev) = ev {
				log::debug!("notify events: {}", ev.len());

				for (idx, ev) in ev.iter().enumerate() {
					log::debug!("notify [{idx}]: {ev:?}");
				}

				if !ev.is_empty() {
					sender.send(())?;
				}
			}
		}
	}
}

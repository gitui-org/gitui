//!

use crate::{
	error::Result,
	progress::ProgressPercent,
	sync::remotes::push::{AsyncProgress, ProgressNotification},
	AsyncGitNotification,
};
use crossbeam_channel::{Receiver, Sender};
use git2::PackBuilderStage;
use std::{
	sync::{Arc, Mutex},
	thread::{self, JoinHandle},
};

/// used for push/pull
#[derive(Clone, Debug)]
pub enum RemoteProgressState {
	///
	PackingAddingObject,
	///
	PackingDeltafiction,
	///
	Pushing,
	/// fetch progress
	Transfer,
	/// remote progress done
	Done,
}

///
#[derive(Clone, Debug)]
pub struct RemoteProgress {
	///
	pub state: RemoteProgressState,
	///
	pub progress: ProgressPercent,
}

impl RemoteProgress {
	///
	pub fn new(
		state: RemoteProgressState,
		current: usize,
		total: usize,
	) -> Self {
		Self {
			state,
			progress: ProgressPercent::new(current, total),
		}
	}

	///
	pub const fn get_progress_percent(&self) -> u8 {
		self.progress.progress
	}

	pub(crate) fn set_progress<T>(
		progress: &Arc<Mutex<Option<T>>>,
		state: Option<T>,
	) -> Result<()> {
		let mut progress = progress.lock()?;

		*progress = state;

		Ok(())
	}

	/// spawn thread to listen to progress notifications coming in from blocking remote git method (fetch/push)
	pub(crate) fn spawn_receiver_thread<
		T: 'static + AsyncProgress,
	>(
		notification_type: AsyncGitNotification,
		sender: Sender<AsyncGitNotification>,
		receiver: Receiver<T>,
		progress: Arc<Mutex<Option<T>>>,
	) -> JoinHandle<()> {
		thread::spawn(move || loop {
			let incoming = receiver.recv();
			match incoming {
				Ok(update) => {
					Self::set_progress(
						&progress,
						Some(update.clone()),
					)
					.expect("set progress failed");
					sender
						.send(notification_type)
						.expect("Notification error");

					thread::yield_now();

					if update.is_done() {
						break;
					}
				}
				Err(e) => {
					log::error!(
						"remote progress receiver error: {e}",
					);
					break;
				}
			}
		})
	}
}

impl From<ProgressNotification> for RemoteProgress {
	fn from(progress: ProgressNotification) -> Self {
		match progress {
			ProgressNotification::Packing {
				stage,
				current,
				total,
			} => match stage {
				PackBuilderStage::AddingObjects => Self::new(
					RemoteProgressState::PackingAddingObject,
					current,
					total,
				),
				PackBuilderStage::Deltafication => Self::new(
					RemoteProgressState::PackingDeltafiction,
					current,
					total,
				),
			},
			ProgressNotification::PushTransfer {
				current,
				total,
				..
			} => Self::new(
				RemoteProgressState::Pushing,
				current,
				total,
			),
			ProgressNotification::Transfer {
				objects,
				total_objects,
				..
			} => Self::new(
				RemoteProgressState::Transfer,
				objects,
				total_objects,
			),
			_ => Self::new(RemoteProgressState::Done, 1, 1),
		}
	}
}

use crate::notify_mutex::NotifyableMutex;
use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossterm::event::{self, Event, Event::Key, KeyEventKind};
use std::{
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	thread,
	time::Duration,
};

static FAST_POLL_DURATION: Duration = Duration::from_millis(100);
static SLOW_POLL_DURATION: Duration = Duration::from_millis(10000);

///
#[derive(Clone, Copy, Debug)]
pub enum InputState {
	Paused,
	Polling,
}

///
#[derive(Clone, Debug)]
pub enum InputEvent {
	Input(Event),
	State(InputState),
}

///
#[derive(Clone)]
pub struct Input {
	desired_state: Arc<NotifyableMutex<bool>>,
	current_state: Arc<AtomicBool>,
	receiver: Receiver<InputEvent>,
	aborted: Arc<AtomicBool>,
}

impl Input {
	///
	pub fn new() -> Self {
		let (tx, rx) = unbounded();

		let desired_state = Arc::new(NotifyableMutex::new(true));
		let current_state = Arc::new(AtomicBool::new(true));
		let aborted = Arc::new(AtomicBool::new(false));

		let arc_desired = Arc::clone(&desired_state);
		let arc_current = Arc::clone(&current_state);
		let arc_aborted = Arc::clone(&aborted);

		thread::spawn(move || {
			if let Err(e) =
				Self::input_loop(&arc_desired, &arc_current, &tx)
			{
				log::error!("input thread error: {}", e);
				arc_aborted.store(true, Ordering::SeqCst);
			}
		});

		Self {
			receiver: rx,
			desired_state,
			current_state,
			aborted,
		}
	}

	///
	pub fn receiver(&self) -> Receiver<InputEvent> {
		self.receiver.clone()
	}

	///
	pub fn set_polling(&self, enabled: bool) {
		self.desired_state.set_and_notify(enabled);
	}

	fn shall_poll(&self) -> bool {
		self.desired_state.get()
	}

	///
	pub fn is_state_changing(&self) -> bool {
		self.shall_poll()
			!= self.current_state.load(Ordering::Relaxed)
	}

	pub fn is_aborted(&self) -> bool {
		self.aborted.load(Ordering::SeqCst)
	}

	fn poll(dur: Duration) -> anyhow::Result<Option<Event>> {
		if event::poll(dur)? {
			Ok(Some(event::read()?))
		} else {
			Ok(None)
		}
	}

	fn input_loop(
		arc_desired: &Arc<NotifyableMutex<bool>>,
		arc_current: &Arc<AtomicBool>,
		tx: &Sender<InputEvent>,
	) -> Result<()> {
		let mut poll_duration = SLOW_POLL_DURATION;
		loop {
			if arc_desired.get() {
				if !arc_current.load(Ordering::Relaxed) {
					log::info!("input polling resumed");

					tx.send(InputEvent::State(InputState::Polling))?;
				}
				arc_current.store(true, Ordering::Relaxed);

				if let Some(e) = Self::poll(poll_duration)? {
					// windows send key release too, only process key press
					if let Key(key) = e {
						if key.kind != KeyEventKind::Press {
							continue;
						}
					}

					tx.send(InputEvent::Input(e))?;
					//Note: right after an input event we might have a reason to stop
					// polling (external editor opening) so lets do a quick poll until the next input
					// this fixes https://github.com/gitui-org/gitui/issues/1506
					poll_duration = FAST_POLL_DURATION;
				} else {
					poll_duration = SLOW_POLL_DURATION;
				}
			} else {
				if arc_current.load(Ordering::Relaxed) {
					log::info!("input polling suspended");

					tx.send(InputEvent::State(InputState::Paused))?;
				}

				arc_current.store(false, Ordering::Relaxed);

				arc_desired.wait(true);
			}
		}
	}
}

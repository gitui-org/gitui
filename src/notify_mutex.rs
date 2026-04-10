use std::sync::{Arc, Condvar, Mutex};

/// combines a `Mutex` and `Condvar` to allow waiting for a change in the variable protected by the `Mutex`
#[derive(Clone, Debug)]
pub struct NotifiableMutex<T>
where
	T: Send + Sync,
{
	data: Arc<(Mutex<T>, Condvar)>,
}

impl<T> NotifiableMutex<T>
where
	T: Send + Sync,
{
	///
	pub fn new(start_value: T) -> Self {
		Self {
			data: Arc::new((Mutex::new(start_value), Condvar::new())),
		}
	}

	///
	pub fn wait(&self, condition: T)
	where
		T: PartialEq + Copy,
	{
		let mut data = self.data.0.lock().expect("lock err");
		while *data != condition {
			data = self.data.1.wait(data).expect("wait err");
		}
		drop(data);
	}

	///
	pub fn set_and_notify(&self, value: T) {
		*self.data.0.lock().expect("set err") = value;
		self.data.1.notify_one();
	}

	///
	pub fn get(&self) -> T
	where
		T: Copy,
	{
		*self.data.0.lock().expect("get err")
	}
}

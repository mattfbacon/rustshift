use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

/// `t` should be in the range `0.0..=1.0` for a typical lerp,
/// but does not strictly have to be.
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
	from * (1.0 - t) + to * t
}

/// Returns the proxy along with its "name" (as given by `wl_registry::Event::Global`) if it was found.
///
/// Any events from the proxy will be ignored.
pub fn get_proxy<T: Proxy + 'static>(
	connection: &Connection,
	minimum_version: u32,
) -> Option<(u32, T)> {
	struct Helper<T> {
		slot: Option<(u32, T)>,
		ignored_handle: QueueHandle<Ignored>,
		minimum_version: u32,
	}

	impl<T: Proxy + 'static> Dispatch<wl_registry::WlRegistry, ()> for Helper<T> {
		fn event(
			state: &mut Self,
			registry: &wl_registry::WlRegistry,
			event: wl_registry::Event,
			_data: &(),
			_conn: &Connection,
			_handle: &QueueHandle<Self>,
		) {
			match event {
				wl_registry::Event::Global {
					name,
					interface,
					version: _,
				} => {
					if interface == T::interface().name {
						let proxy = registry.bind(name, state.minimum_version, &state.ignored_handle, ());
						state.slot = Some((name, proxy));
					}
				}
				wl_registry::Event::GlobalRemove { name: removed_name } => {
					if state
						.slot
						.as_ref()
						.is_some_and(|(name, _proxy)| *name == removed_name)
					{
						state.slot = None;
					}
				}
				_ => todo!(),
			}
		}
	}

	let mut queue = connection.new_event_queue();
	let handle = queue.handle();

	let _registry = connection.display().get_registry(&handle, ());

	let mut helper = Helper {
		slot: None,
		ignored_handle: connection.new_event_queue().handle(),
		minimum_version,
	};
	queue.roundtrip(&mut helper).unwrap();
	helper.slot
}

pub struct Ignored;

impl<T: Proxy> Dispatch<T, ()> for Ignored {
	fn event(
		_state: &mut Self,
		_proxy: &T,
		_event: <T as Proxy>::Event,
		_data: &(),
		_connection: &Connection,
		_handle: &QueueHandle<Self>,
	) {
	}
}

pub trait TakeIfExt: Sized {
	type Item;

	fn take_if(&mut self, cond: impl FnMut(&Self::Item) -> bool) -> Option<Self::Item>;
}

impl<T> TakeIfExt for Vec<T> {
	type Item = T;

	fn take_if(&mut self, cond: impl FnMut(&T) -> bool) -> Option<T> {
		self
			.iter()
			.position(cond)
			.map(|index| self.swap_remove(index))
	}
}

macro_rules! cstr {
	($x:expr) => {
		std::ffi::CStr::from_bytes_with_nul(concat!($x, "\0").as_bytes()).unwrap()
	};
}
pub(crate) use cstr;

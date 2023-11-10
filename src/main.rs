#![deny(
	absolute_paths_not_starting_with_crate,
	keyword_idents,
	macro_use_extern_crate,
	meta_variable_misuse,
	missing_abi,
	missing_copy_implementations,
	non_ascii_idents,
	nonstandard_style,
	noop_method_call,
	pointer_structural_match,
	private_in_public,
	rust_2018_idioms,
	unused_qualifications
)]
#![warn(clippy::pedantic)]
// We do a lot of conversions between floats and integers and precision is not really important.
#![allow(
	clippy::cast_precision_loss,
	clippy::cast_sign_loss,
	clippy::cast_possible_truncation
)]
#![forbid(unsafe_code)]

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsFd;
use std::sync::mpsc::SyncSender;

use nix::sys::memfd::{memfd_create, MemFdCreateFlag};
use signal_hook::consts::signal;
use signal_hook::iterator::Signals;
use time::ext::NumericalDuration;
use time::{Duration, Time, UtcOffset};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_wlr::gamma_control::v1::client::{
	zwlr_gamma_control_manager_v1, zwlr_gamma_control_v1,
};
use zbus::{dbus_proxy, fdo};

use crate::color::{Config, Ramps};
use crate::util::lerp;

mod color;
mod util;

macro_rules! cstr {
	($x:expr) => {
		std::ffi::CStr::from_bytes_with_nul(concat!($x, "\0").as_bytes()).unwrap()
	};
}

#[derive(Default)]
struct Proxies {
	gamma_manager: Option<zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1>,
	outputs: Vec<wl_output::WlOutput>,
}

const ZWLR_GAMMA_CONTROL_MANAGER_V1_VERSION: u32 = 1;
const WL_OUTPUT_VERSION: u32 = 4;

impl Dispatch<wl_registry::WlRegistry, ()> for Proxies {
	fn event(
		state: &mut Self,
		registry: &wl_registry::WlRegistry,
		event: wl_registry::Event,
		_data: &(),
		_connection: &Connection,
		handle: &QueueHandle<Self>,
	) {
		let wl_registry::Event::Global {
			name, interface, ..
		} = event
		else {
			return;
		};

		match interface.as_str() {
			"zwlr_gamma_control_manager_v1" => {
				let proxy = registry.bind(name, ZWLR_GAMMA_CONTROL_MANAGER_V1_VERSION, handle, ());
				state.gamma_manager = Some(proxy);
			}
			"wl_output" => {
				let proxy = registry.bind(name, WL_OUTPUT_VERSION, handle, ());
				state.outputs.push(proxy);
			}
			_ => {}
		}
	}
}

delegate_noop!(Proxies: zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1);
delegate_noop!(Proxies: wl_output::WlOutput);

#[derive(Debug)]
struct GammaControlIntermediate {
	proxy: zwlr_gamma_control_v1::ZwlrGammaControlV1,
	ramp_size: Option<u32>,
}

#[derive(Debug)]
struct AppIntermediate {
	gamma_controls: Vec<GammaControlIntermediate>,
}

impl GammaControlIntermediate {
	fn new(proxy: zwlr_gamma_control_v1::ZwlrGammaControlV1) -> Self {
		Self {
			proxy,
			ramp_size: None,
		}
	}
}

impl Dispatch<zwlr_gamma_control_v1::ZwlrGammaControlV1, ()> for AppIntermediate {
	fn event(
		state: &mut Self,
		proxy: &zwlr_gamma_control_v1::ZwlrGammaControlV1,
		event: zwlr_gamma_control_v1::Event,
		_data: &(),
		_connection: &Connection,
		_handle: &QueueHandle<Self>,
	) {
		match event {
			zwlr_gamma_control_v1::Event::GammaSize { size } => {
				let control = state
					.gamma_controls
					.iter_mut()
					.find(|control| &control.proxy == proxy)
					.expect("received event for gamma control proxy which we never created");
				control.ramp_size = Some(size);
			}
			zwlr_gamma_control_v1::Event::Failed => {
				state
					.gamma_controls
					.retain(|control| &control.proxy != proxy);
			}
			_ => {}
		}
	}
}

struct GammaControl {
	proxy: zwlr_gamma_control_v1::ZwlrGammaControlV1,
	ramps: Ramps,
}

struct App {
	gamma_controls: Vec<GammaControl>,
	event_queue: EventQueue<Ignored>,
}

impl App {
	fn set_gamma(&mut self, config: Config) {
		tracing::debug!(?config, "setting gamma");
		for control in &mut self.gamma_controls {
			let mut ramps_fd: File = memfd_create(cstr!("gamma-ramps"), MemFdCreateFlag::MFD_CLOEXEC)
				.unwrap()
				.into();
			config.generate_ramps(&mut control.ramps);
			ramps_fd.write_all(control.ramps.as_bytes()).unwrap();
			ramps_fd.seek(SeekFrom::Start(0)).unwrap();
			control.proxy.set_gamma(ramps_fd.as_fd());
		}
		self.event_queue.roundtrip(&mut Ignored).unwrap();
	}
}

struct Ignored;

impl<T: wayland_client::Proxy> Dispatch<T, ()> for Ignored {
	fn event(
		_state: &mut Self,
		_proxy: &T,
		_event: <T as wayland_client::Proxy>::Event,
		_data: &(),
		_connection: &Connection,
		_queue_handle: &QueueHandle<Self>,
	) {
	}
}

enum Event {
	Update,
	SetDimmed(bool),
}

#[dbus_proxy(
	interface = "org.freedesktop.timedate1",
	default_service = "org.freedesktop.timedate1",
	default_path = "/org/freedesktop/timedate1",
	gen_async = false
)]
trait TimeDate {
	#[dbus_proxy(property)]
	fn timezone(&self) -> fdo::Result<String>;
}

fn get_gamma_controls(connection: &Connection) -> Vec<GammaControl> {
	let (gamma_manager, outputs) = {
		let mut proxies = Proxies::default();

		let mut event_queue = connection.new_event_queue();
		let handle = event_queue.handle();

		let _registry = connection.display().get_registry(&handle, ());

		event_queue.roundtrip(&mut proxies).unwrap();

		(proxies.gamma_manager.unwrap(), proxies.outputs)
	};

	let gamma_controls = {
		let mut event_queue = connection.new_event_queue();
		let handle = event_queue.handle();

		let gamma_controls: Vec<_> = outputs
			.into_iter()
			.map(|output| gamma_manager.get_gamma_control(&output, &handle, ()))
			.map(GammaControlIntermediate::new)
			.collect();
		let mut app_intermediate = AppIntermediate { gamma_controls };

		event_queue.roundtrip(&mut app_intermediate).unwrap();

		app_intermediate.gamma_controls
	};

	gamma_controls
		.into_iter()
		.map(|control| {
			let ramp_size = control
				.ramp_size
				.expect("did not receive ramp size for output. is there another gamma manager running?");
			GammaControl {
				proxy: control.proxy,
				ramps: Ramps::new(ramp_size.try_into().unwrap()),
			}
		})
		.collect()
}

fn update_regularly(event_send: &SyncSender<Event>) {
	let interval = std::time::Duration::from_secs(60);
	loop {
		std::thread::sleep(interval);
		if event_send.send(Event::Update).is_err() {
			break;
		}
	}
}

fn handle_timezone_updates(event_send: &SyncSender<Event>, dbus_time_proxy: &TimeDateProxy<'_>) {
	let mut changes = dbus_time_proxy.receive_property_changed::<String>("Timezone");
	// Ignore the first change, which isn't really a change at all.
	_ = changes.next();
	for change in changes {
		tracing::debug!(new_timezone = change.get().unwrap(), "got timezone update");
		if event_send.send(Event::Update).is_err() {
			break;
		}
	}
}

fn signal_handler(event_send: &SyncSender<Event>) {
	let mut signals = Signals::new([signal::SIGUSR1, signal::SIGUSR2]).unwrap();
	for signal in &mut signals {
		let event = match signal {
			signal::SIGUSR1 => Event::SetDimmed(true),
			signal::SIGUSR2 => Event::SetDimmed(false),
			_ => continue,
		};
		if event_send.send(event).is_err() {
			break;
		}
	}
}

fn get_config(dbus_time_proxy: &TimeDateProxy<'_>, dimmed: bool) -> Config {
	let time = {
		let time_zone_name = dbus_time_proxy.timezone().unwrap();
		let time_zone = tz::TimeZone::from_posix_tz(&time_zone_name).unwrap_or_else(|error| {
			panic!("error resolving time zone name {time_zone_name:?} to a UTC offset: {error}")
		});
		let datetime_utc = time::OffsetDateTime::now_utc();
		let tz_info = time_zone
			.find_local_time_type(datetime_utc.unix_timestamp())
			.unwrap();
		let mut utc_offset_seconds = tz_info.ut_offset();
		// Cancel out daylight savings time in the following temperature calculations.
		if !tz_info.is_dst() {
			utc_offset_seconds += 3600;
		}
		let utc_offset = UtcOffset::from_whole_seconds(utc_offset_seconds).unwrap();
		let datetime_local = datetime_utc.to_offset(utc_offset);
		datetime_local.time()
	};
	tracing::debug!(?time, "got time");

	let day_temp = 6500;
	let night_temp = 3500;

	let temperature = {
		let daytime_start = Time::from_hms(7, 45, 0).unwrap();
		let daytime_end = Time::from_hms(19, 45, 0).unwrap();
		let fade_time = 30.minutes();

		let daytime_diff = time - daytime_start;
		let nighttime_diff = time - daytime_end;
		if daytime_diff > Duration::ZERO && daytime_diff < fade_time {
			lerp(
				night_temp as f32,
				day_temp as f32,
				daytime_diff.as_seconds_f32() / fade_time.as_seconds_f32(),
			) as u32
		} else if nighttime_diff > Duration::ZERO && nighttime_diff < fade_time {
			lerp(
				day_temp as f32,
				night_temp as f32,
				nighttime_diff.as_seconds_f32() / fade_time.as_seconds_f32(),
			) as u32
		} else if time >= daytime_start && time <= daytime_end {
			day_temp
		} else {
			night_temp
		}
	};

	let brightness = if dimmed { 0.4 } else { 1.0 };

	Config::new(temperature, brightness).unwrap()
}

fn main() {
	tracing_subscriber::fmt::init();

	let dbus = zbus::blocking::Connection::system().expect("connecting to dbus system bus");
	let dbus_time_proxy = TimeDateProxy::new(&dbus).expect("connecting to dbus timedate protocol");

	let connection = Connection::connect_to_env().expect("connecting to wayland from env");

	let mut app = App {
		gamma_controls: get_gamma_controls(&connection),
		event_queue: connection.new_event_queue(),
	};

	let (event_send, event_recv) = std::sync::mpsc::sync_channel::<Event>(4);

	std::thread::spawn({
		let event_send = event_send.clone();
		move || update_regularly(&event_send)
	});
	std::thread::spawn({
		let event_send = event_send.clone();
		move || signal_handler(&event_send)
	});
	std::thread::spawn({
		let event_send = event_send.clone();
		let dbus_time_proxy = dbus_time_proxy.clone();
		move || handle_timezone_updates(&event_send, &dbus_time_proxy)
	});

	let mut dimmed = false;

	let mut last_config = get_config(&dbus_time_proxy, dimmed);
	app.set_gamma(last_config);

	while let Ok(event) = event_recv.recv() {
		match event {
			Event::Update => {}
			Event::SetDimmed(new) => {
				dimmed = new;
			}
		}
		let new_config = get_config(&dbus_time_proxy, dimmed);
		if new_config.different_from(last_config) {
			last_config = new_config;
			app.set_gamma(new_config);
		} else {
			tracing::debug!(
				?new_config,
				?last_config,
				"new config is not different enough from last config",
			);
		}
	}

	// When the gamma control object is destroyed, the gamma table is restored.
}

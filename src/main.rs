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

use std::sync::mpsc::SyncSender;

use signal_hook::consts::signal;
use signal_hook::iterator::Signals;
use time::ext::NumericalDuration;
use time::{Duration, Time};
use wayland_client::Connection;

use crate::color::Config;
use crate::util::{lerp, Ignored};
use crate::wayland::GammaControl;

mod color;
mod dbus_time;
mod util;
mod wayland;

#[derive(Debug)]
pub enum Event {
	AddOutput(GammaControl),
	RemoveOutput { output_registry_name: u32 },
	Update,
	SetDimmed(bool),
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

fn get_config(time: Time, dimmed: bool) -> Config {
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

	let dbus_time = crate::dbus_time::DbusTime::connect();

	// Application state
	let mut dimmed = false;
	let mut gamma_controls = Vec::new();

	let connection = Connection::connect_to_env().expect("connecting to wayland from env");

	// Event sources
	let (event_send, event_recv) = std::sync::mpsc::sync_channel::<Event>(4);

	std::thread::spawn({
		let event_send = event_send.clone();
		let connection = connection.clone();
		move || wayland::monitor_outputs(event_send, &connection)
	});
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
		let dbus_time = dbus_time.clone();
		move || dbus_time.handle_timezone_updates(&event_send)
	});

	// Main loop
	let mut ignored_queue = connection.new_event_queue();
	while let Ok(event) = event_recv.recv() {
		tracing::debug!(?event, "got event");
		match event {
			Event::AddOutput(output) => gamma_controls.push(output),
			Event::RemoveOutput {
				output_registry_name: output_id,
			} => {
				gamma_controls.retain(|control| control.is_for_output(output_id));
				// No need to update the other outputs.
				continue;
			}
			Event::Update => {}
			Event::SetDimmed(new) => {
				dimmed = new;
			}
		}
		let config = get_config(dbus_time.get_time(), dimmed);
		for control in &mut gamma_controls {
			control.set_gamma(config);
		}
		ignored_queue.roundtrip(&mut Ignored).unwrap();
	}

	// When a gamma control object is destroyed, its gamma table is restored.
}

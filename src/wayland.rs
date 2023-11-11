use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsFd;
use std::sync::mpsc::SyncSender;

use nix::sys::memfd::{memfd_create, MemFdCreateFlag};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::gamma_control::v1::client::{
	zwlr_gamma_control_manager_v1, zwlr_gamma_control_v1,
};

use crate::color::{Config, Ramps};
use crate::util::{cstr, get_proxy, TakeIfExt};
use crate::Event;

#[derive(Debug)]
struct GammaControlIntermediate {
	output: wl_output::WlOutput,
	output_registry_name: u32,
	output_description: Option<Box<str>>,
	control: zwlr_gamma_control_v1::ZwlrGammaControlV1,
}

struct Helper {
	gamma_control_manager: zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1,
	event_send: SyncSender<Event>,
	intermediates: Vec<GammaControlIntermediate>,
	done: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Helper {
	fn event(
		state: &mut Self,
		registry: &wl_registry::WlRegistry,
		event: wl_registry::Event,
		_data: &(),
		_conn: &Connection,
		handle: &QueueHandle<Self>,
	) {
		match event {
			wl_registry::Event::Global {
				name,
				interface,
				version: _,
			} => {
				if interface == "wl_output" {
					let output = registry.bind(name, wl_output::WlOutput::interface().version, handle, ());
					let control = state
						.gamma_control_manager
						.get_gamma_control(&output, handle, ());
					let intermediate = GammaControlIntermediate {
						output,
						output_registry_name: name,
						output_description: None,
						control,
					};
					state.intermediates.push(intermediate);
				}
			}
			wl_registry::Event::GlobalRemove { name } => {
				state
					.intermediates
					.retain(|intermediate| intermediate.output_registry_name != name);
				state.done |= state
					.event_send
					.send(Event::RemoveOutput {
						output_registry_name: name,
					})
					.is_err();
			}
			_ => todo!(),
		}
	}
}

impl Dispatch<wl_output::WlOutput, ()> for Helper {
	fn event(
		state: &mut Self,
		proxy: &wl_output::WlOutput,
		event: wl_output::Event,
		_data: &(),
		_conn: &Connection,
		_handle: &QueueHandle<Self>,
	) {
		if let wl_output::Event::Description { description } = event {
			if let Some(intermediate) = state
				.intermediates
				.iter_mut()
				.find(|intermediate| intermediate.output == *proxy)
			{
				intermediate.output_description = Some(description.into());
			}
		}
	}
}

delegate_noop!(Helper: ignore zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1);

impl Dispatch<zwlr_gamma_control_v1::ZwlrGammaControlV1, ()> for Helper {
	fn event(
		state: &mut Self,
		proxy: &zwlr_gamma_control_v1::ZwlrGammaControlV1,
		event: zwlr_gamma_control_v1::Event,
		_data: &(),
		_conn: &Connection,
		_handle: &QueueHandle<Self>,
	) {
		let Some(intermediate) = state
			.intermediates
			.take_if(|intermediate| intermediate.control == *proxy)
		else {
			return;
		};
		match event {
			zwlr_gamma_control_v1::Event::GammaSize { size: ramp_size } => {
				let control = GammaControl {
					output_registry_name: intermediate.output_registry_name,
					output_description: intermediate.output_description.unwrap(),
					proxy: intermediate.control,
					ramps: Ramps::new(ramp_size.try_into().unwrap()),
					last_config: None,
				};
				state.done |= state.event_send.send(Event::AddOutput(control)).is_err();
			}
			zwlr_gamma_control_v1::Event::Failed => {
				let description = intermediate.output_description.map_or_else(
					|| "(not received)".into(),
					|description| format!("{description:?}"),
				);
				panic!("gamma control failed for output with description {description}");
			}
			_ => {}
		}
	}
}

pub fn monitor_outputs(event_send: SyncSender<Event>, connection: &Connection) {
	let mut queue = connection.new_event_queue();
	let handle = queue.handle();
	let _registry = connection.display().get_registry(&handle, ());

	let mut helper = Helper {
		gamma_control_manager: get_proxy(connection).unwrap().1,
		event_send,
		intermediates: Vec::new(),
		done: false,
	};
	while !helper.done {
		queue.blocking_dispatch(&mut helper).unwrap();
	}
}

pub struct GammaControl {
	output_registry_name: u32,
	output_description: Box<str>,
	proxy: zwlr_gamma_control_v1::ZwlrGammaControlV1,
	ramps: Ramps,
	last_config: Option<Config>,
}

impl std::fmt::Debug for GammaControl {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// Ensure exhaustiveness.
		let Self {
			output_registry_name,
			output_description,
			proxy: _,
			ramps: _,
			last_config,
		} = self;

		f.debug_struct("GammaControl")
			.field("output_registry_name", output_registry_name)
			.field("output_description", output_description)
			.field("last_config", last_config)
			.finish_non_exhaustive()
	}
}

impl GammaControl {
	pub fn set_gamma(&mut self, config: Config) {
		tracing::trace!(?self.output_description, ?config, "setting gamma");

		let last_config = self.last_config.replace(config);
		if last_config.is_some_and(|last_config| !config.different_from(last_config)) {
			tracing::trace!(?self.output_description, new_config=?config, ?last_config, "new config is not different enough from last config");
		}

		let mut ramps_fd: File = memfd_create(cstr!("gamma-ramps"), MemFdCreateFlag::MFD_CLOEXEC)
			.unwrap()
			.into();
		config.generate_ramps(&mut self.ramps);
		ramps_fd.write_all(self.ramps.as_bytes()).unwrap();
		ramps_fd.seek(SeekFrom::Start(0)).unwrap();
		self.proxy.set_gamma(ramps_fd.as_fd());
	}

	#[inline]
	#[must_use]
	pub fn is_for_output(&self, id: u32) -> bool {
		self.output_registry_name == id
	}
}

use std::sync::mpsc::SyncSender;

use time::UtcOffset;
use zbus::{dbus_proxy, fdo};

use crate::Event;

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

#[derive(Clone)]
pub struct DbusTime {
	proxy: TimeDateProxy<'static>,
}

impl DbusTime {
	pub fn connect() -> Self {
		let dbus = zbus::blocking::Connection::system().expect("connecting to dbus system bus");
		let proxy = TimeDateProxy::new(&dbus).expect("connecting to dbus timedate protocol");

		Self { proxy }
	}

	pub fn handle_timezone_updates(&self, event_send: &SyncSender<Event>) {
		let mut changes = self.proxy.receive_property_changed::<String>("Timezone");
		// Ignore the first change, which isn't really a change at all.
		_ = changes.next();
		for change in changes {
			tracing::trace!(new_timezone = change.get().unwrap(), "got timezone update");
			if event_send.send(Event::Update).is_err() {
				break;
			}
		}
	}

	/// Note that the returned time intentionally does not respect daylight savings time in the local timezone.
	pub fn get_time(&self) -> time::Time {
		let time_zone_name = self.proxy.timezone().unwrap();
		let time_zone = tz::TimeZone::from_posix_tz(&time_zone_name).unwrap_or_else(|error| {
			panic!("error resolving time zone name {time_zone_name:?} to a UTC offset: {error}")
		});
		let datetime_utc = time::OffsetDateTime::now_utc();
		let tz_info = time_zone
			.find_local_time_type(datetime_utc.unix_timestamp())
			.unwrap();
		let mut utc_offset_seconds = tz_info.ut_offset();
		// Cancel out daylight savings time.
		if !tz_info.is_dst() {
			utc_offset_seconds += 3600;
		}
		let utc_offset = UtcOffset::from_whole_seconds(utc_offset_seconds).unwrap();
		let datetime_local = datetime_utc.to_offset(utc_offset);
		let time = datetime_local.time();

		tracing::trace!(?time, "got time");

		time
	}
}

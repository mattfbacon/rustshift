use std::num::NonZeroU32;

use crate::util::lerp;

pub type Temperature = NonZeroU32;

#[derive(Debug, Clone, Copy)]
pub struct Config {
	/// Kelvins. Must be in the range 1000..=25_000 (invariant).
	temperature: Temperature,
	/// 0.0..=1.0 (invariant) where 0.0 is black and 1.0 is full brightness.
	brightness: f32,
}

impl Config {
	pub fn new(temperature: u32, brightness: f32) -> Option<Self> {
		if (1000..=25000).contains(&temperature) && (0.0..=1.0).contains(&brightness) {
			Some(Self {
				temperature: temperature.try_into().ok()?,
				brightness,
			})
		} else {
			None
		}
	}

	pub fn different_from(self, other: Self) -> bool {
		self.temperature.get().abs_diff(other.temperature.get()) > 10
			|| (self.brightness - other.brightness).abs() > 0.01
	}
}

macro_rules! const_unwrap {
	($x:expr) => {
		match $x {
			Some(x) => x,
			None => panic!("const unwrap failed"),
		}
	};
}

const NEUTRAL_TEMPERATURE: Temperature = const_unwrap!(NonZeroU32::new(6500));

impl Default for Config {
	fn default() -> Self {
		Self {
			temperature: NEUTRAL_TEMPERATURE,
			brightness: 1.0,
		}
	}
}

pub struct Ramps {
	/// Invariant: data.len() == num_ramps * 3
	/// The data is segmented into three sections: red, green, and blue (SOA).
	data: Box<[u16]>,
}

impl Ramps {
	pub fn new(num_ramps: usize) -> Self {
		Self {
			data: vec![0; num_ramps * 3].into(),
		}
	}

	fn rgb_slices(&mut self) -> [&mut [u16]; 3] {
		let ramp_size = self.ramp_size();
		let (red, rest) = self.data.split_at_mut(ramp_size);
		let (green, blue) = rest.split_at_mut(ramp_size);
		[red, green, blue]
	}

	fn iter_rgb_mut(&mut self) -> impl Iterator<Item = [&mut u16; 3]> {
		let [r, g, b] = self.rgb_slices();
		r.iter_mut()
			.zip(g.iter_mut())
			.zip(b.iter_mut())
			.map(|((r, g), b)| [r, g, b])
	}

	fn ramp_size(&self) -> usize {
		self.data.len() / 3
	}

	pub fn as_bytes(&self) -> &[u8] {
		bytemuck::cast_slice(&self.data)
	}
}

impl Config {
	pub fn generate_ramps(self, ramps: &mut Ramps) {
		// We have already checked that `self.temperature` is in the valid range.
		let white_point = get_white_point(self.temperature.get()).unwrap();
		let pure_step = 1.0 / ramps.ramp_size() as f32;
		for (i, [r, g, b]) in ramps.iter_rgb_mut().enumerate() {
			let pure = i as f32 * pure_step * self.brightness;
			*r = f32_to_u16_full(pure * white_point.red);
			*g = f32_to_u16_full(pure * white_point.green);
			*b = f32_to_u16_full(pure * white_point.blue);
		}
	}
}

#[derive(Debug, Clone, Copy)]
struct ColorF32 {
	red: f32,
	green: f32,
	blue: f32,
}

impl ColorF32 {
	fn lerp(from: Self, to: Self, t: f32) -> Self {
		Self {
			red: lerp(from.red, to.red, t),
			green: lerp(from.green, to.green, t),
			blue: lerp(from.blue, to.blue, t),
		}
	}
}

/// Translates from the f32 range `0.0..=1.0` to the full range of `u16`.
fn f32_to_u16_full(f: f32) -> u16 {
	let scaled = f * (f32::from(u16::MAX) + 1.0);
	// This cast is saturating.
	scaled as u16
}

/// White-point values for temperatures at 100K intervals.
/// From gammastep's colorramp.c.
#[allow(
	clippy::unreadable_literal, // More readable.
	clippy::excessive_precision, // Consistency.
)]
#[rustfmt::skip] // Single-line form.
const BLACK_BODY_COLOR: &[ColorF32] = &[
	ColorF32 { red: 1.00000000, green: 0.18172716, blue: 0.00000000 }, // 1000K
	ColorF32 { red: 1.00000000, green: 0.25503671, blue: 0.00000000 }, // 1100K
	ColorF32 { red: 1.00000000, green: 0.30942099, blue: 0.00000000 }, // 1200K
	ColorF32 { red: 1.00000000, green: 0.35357379, blue: 0.00000000 },
	ColorF32 { red: 1.00000000, green: 0.39091524, blue: 0.00000000 },
	ColorF32 { red: 1.00000000, green: 0.42322816, blue: 0.00000000 },
	ColorF32 { red: 1.00000000, green: 0.45159884, blue: 0.00000000 },
	ColorF32 { red: 1.00000000, green: 0.47675916, blue: 0.00000000 },
	ColorF32 { red: 1.00000000, green: 0.49923747, blue: 0.00000000 },
	ColorF32 { red: 1.00000000, green: 0.51943421, blue: 0.00000000 },
	ColorF32 { red: 1.00000000, green: 0.54360078, blue: 0.08679949 },
	ColorF32 { red: 1.00000000, green: 0.56618736, blue: 0.14065513 },
	ColorF32 { red: 1.00000000, green: 0.58734976, blue: 0.18362641 },
	ColorF32 { red: 1.00000000, green: 0.60724493, blue: 0.22137978 },
	ColorF32 { red: 1.00000000, green: 0.62600248, blue: 0.25591950 },
	ColorF32 { red: 1.00000000, green: 0.64373109, blue: 0.28819679 },
	ColorF32 { red: 1.00000000, green: 0.66052319, blue: 0.31873863 },
	ColorF32 { red: 1.00000000, green: 0.67645822, blue: 0.34786758 },
	ColorF32 { red: 1.00000000, green: 0.69160518, blue: 0.37579588 },
	ColorF32 { red: 1.00000000, green: 0.70602449, blue: 0.40267128 },
	ColorF32 { red: 1.00000000, green: 0.71976951, blue: 0.42860152 },
	ColorF32 { red: 1.00000000, green: 0.73288760, blue: 0.45366838 },
	ColorF32 { red: 1.00000000, green: 0.74542112, blue: 0.47793608 },
	ColorF32 { red: 1.00000000, green: 0.75740814, blue: 0.50145662 },
	ColorF32 { red: 1.00000000, green: 0.76888303, blue: 0.52427322 },
	ColorF32 { red: 1.00000000, green: 0.77987699, blue: 0.54642268 },
	ColorF32 { red: 1.00000000, green: 0.79041843, blue: 0.56793692 },
	ColorF32 { red: 1.00000000, green: 0.80053332, blue: 0.58884417 },
	ColorF32 { red: 1.00000000, green: 0.81024551, blue: 0.60916971 },
	ColorF32 { red: 1.00000000, green: 0.81957693, blue: 0.62893653 },
	ColorF32 { red: 1.00000000, green: 0.82854786, blue: 0.64816570 },
	ColorF32 { red: 1.00000000, green: 0.83717703, blue: 0.66687674 },
	ColorF32 { red: 1.00000000, green: 0.84548188, blue: 0.68508786 },
	ColorF32 { red: 1.00000000, green: 0.85347859, blue: 0.70281616 },
	ColorF32 { red: 1.00000000, green: 0.86118227, blue: 0.72007777 },
	ColorF32 { red: 1.00000000, green: 0.86860704, blue: 0.73688797 },
	ColorF32 { red: 1.00000000, green: 0.87576611, blue: 0.75326132 },
	ColorF32 { red: 1.00000000, green: 0.88267187, blue: 0.76921169 },
	ColorF32 { red: 1.00000000, green: 0.88933596, blue: 0.78475236 },
	ColorF32 { red: 1.00000000, green: 0.89576933, blue: 0.79989606 },
	ColorF32 { red: 1.00000000, green: 0.90198230, blue: 0.81465502 },
	ColorF32 { red: 1.00000000, green: 0.90963069, blue: 0.82838210 },
	ColorF32 { red: 1.00000000, green: 0.91710889, blue: 0.84190889 },
	ColorF32 { red: 1.00000000, green: 0.92441842, blue: 0.85523742 },
	ColorF32 { red: 1.00000000, green: 0.93156127, blue: 0.86836903 },
	ColorF32 { red: 1.00000000, green: 0.93853986, blue: 0.88130458 },
	ColorF32 { red: 1.00000000, green: 0.94535695, blue: 0.89404470 },
	ColorF32 { red: 1.00000000, green: 0.95201559, blue: 0.90658983 },
	ColorF32 { red: 1.00000000, green: 0.95851906, blue: 0.91894041 },
	ColorF32 { red: 1.00000000, green: 0.96487079, blue: 0.93109690 },
	ColorF32 { red: 1.00000000, green: 0.97107439, blue: 0.94305985 },
	ColorF32 { red: 1.00000000, green: 0.97713351, blue: 0.95482993 },
	ColorF32 { red: 1.00000000, green: 0.98305189, blue: 0.96640795 },
	ColorF32 { red: 1.00000000, green: 0.98883326, blue: 0.97779486 },
	ColorF32 { red: 1.00000000, green: 0.99448139, blue: 0.98899179 },
	ColorF32 { red: 1.00000000, green: 1.00000000, blue: 1.00000000 }, // 6500K
	ColorF32 { red: 0.98947904, green: 0.99348723, blue: 1.00000000 },
	ColorF32 { red: 0.97940448, green: 0.98722715, blue: 1.00000000 },
	ColorF32 { red: 0.96975025, green: 0.98120637, blue: 1.00000000 },
	ColorF32 { red: 0.96049223, green: 0.97541240, blue: 1.00000000 },
	ColorF32 { red: 0.95160805, green: 0.96983355, blue: 1.00000000 },
	ColorF32 { red: 0.94303638, green: 0.96443333, blue: 1.00000000 },
	ColorF32 { red: 0.93480451, green: 0.95923080, blue: 1.00000000 },
	ColorF32 { red: 0.92689056, green: 0.95421394, blue: 1.00000000 },
	ColorF32 { red: 0.91927697, green: 0.94937330, blue: 1.00000000 },
	ColorF32 { red: 0.91194747, green: 0.94470005, blue: 1.00000000 },
	ColorF32 { red: 0.90488690, green: 0.94018594, blue: 1.00000000 },
	ColorF32 { red: 0.89808115, green: 0.93582323, blue: 1.00000000 },
	ColorF32 { red: 0.89151710, green: 0.93160469, blue: 1.00000000 },
	ColorF32 { red: 0.88518247, green: 0.92752354, blue: 1.00000000 },
	ColorF32 { red: 0.87906581, green: 0.92357340, blue: 1.00000000 },
	ColorF32 { red: 0.87315640, green: 0.91974827, blue: 1.00000000 },
	ColorF32 { red: 0.86744421, green: 0.91604254, blue: 1.00000000 },
	ColorF32 { red: 0.86191983, green: 0.91245088, blue: 1.00000000 },
	ColorF32 { red: 0.85657444, green: 0.90896831, blue: 1.00000000 },
	ColorF32 { red: 0.85139976, green: 0.90559011, blue: 1.00000000 },
	ColorF32 { red: 0.84638799, green: 0.90231183, blue: 1.00000000 },
	ColorF32 { red: 0.84153180, green: 0.89912926, blue: 1.00000000 },
	ColorF32 { red: 0.83682430, green: 0.89603843, blue: 1.00000000 },
	ColorF32 { red: 0.83225897, green: 0.89303558, blue: 1.00000000 },
	ColorF32 { red: 0.82782969, green: 0.89011714, blue: 1.00000000 },
	ColorF32 { red: 0.82353066, green: 0.88727974, blue: 1.00000000 },
	ColorF32 { red: 0.81935641, green: 0.88452017, blue: 1.00000000 },
	ColorF32 { red: 0.81530175, green: 0.88183541, blue: 1.00000000 },
	ColorF32 { red: 0.81136180, green: 0.87922257, blue: 1.00000000 },
	ColorF32 { red: 0.80753191, green: 0.87667891, blue: 1.00000000 },
	ColorF32 { red: 0.80380769, green: 0.87420182, blue: 1.00000000 },
	ColorF32 { red: 0.80018497, green: 0.87178882, blue: 1.00000000 },
	ColorF32 { red: 0.79665980, green: 0.86943756, blue: 1.00000000 },
	ColorF32 { red: 0.79322843, green: 0.86714579, blue: 1.00000000 },
	ColorF32 { red: 0.78988728, green: 0.86491137, blue: 1.00000000 }, // 10_000K
	ColorF32 { red: 0.78663296, green: 0.86273225, blue: 1.00000000 },
	ColorF32 { red: 0.78346225, green: 0.86060650, blue: 1.00000000 },
	ColorF32 { red: 0.78037207, green: 0.85853224, blue: 1.00000000 },
	ColorF32 { red: 0.77735950, green: 0.85650771, blue: 1.00000000 },
	ColorF32 { red: 0.77442176, green: 0.85453121, blue: 1.00000000 },
	ColorF32 { red: 0.77155617, green: 0.85260112, blue: 1.00000000 },
	ColorF32 { red: 0.76876022, green: 0.85071588, blue: 1.00000000 },
	ColorF32 { red: 0.76603147, green: 0.84887402, blue: 1.00000000 },
	ColorF32 { red: 0.76336762, green: 0.84707411, blue: 1.00000000 },
	ColorF32 { red: 0.76076645, green: 0.84531479, blue: 1.00000000 },
	ColorF32 { red: 0.75822586, green: 0.84359476, blue: 1.00000000 },
	ColorF32 { red: 0.75574383, green: 0.84191277, blue: 1.00000000 },
	ColorF32 { red: 0.75331843, green: 0.84026762, blue: 1.00000000 },
	ColorF32 { red: 0.75094780, green: 0.83865816, blue: 1.00000000 },
	ColorF32 { red: 0.74863017, green: 0.83708329, blue: 1.00000000 },
	ColorF32 { red: 0.74636386, green: 0.83554194, blue: 1.00000000 },
	ColorF32 { red: 0.74414722, green: 0.83403311, blue: 1.00000000 },
	ColorF32 { red: 0.74197871, green: 0.83255582, blue: 1.00000000 },
	ColorF32 { red: 0.73985682, green: 0.83110912, blue: 1.00000000 },
	ColorF32 { red: 0.73778012, green: 0.82969211, blue: 1.00000000 },
	ColorF32 { red: 0.73574723, green: 0.82830393, blue: 1.00000000 },
	ColorF32 { red: 0.73375683, green: 0.82694373, blue: 1.00000000 },
	ColorF32 { red: 0.73180765, green: 0.82561071, blue: 1.00000000 },
	ColorF32 { red: 0.72989845, green: 0.82430410, blue: 1.00000000 },
	ColorF32 { red: 0.72802807, green: 0.82302316, blue: 1.00000000 },
	ColorF32 { red: 0.72619537, green: 0.82176715, blue: 1.00000000 },
	ColorF32 { red: 0.72439927, green: 0.82053539, blue: 1.00000000 },
	ColorF32 { red: 0.72263872, green: 0.81932722, blue: 1.00000000 },
	ColorF32 { red: 0.72091270, green: 0.81814197, blue: 1.00000000 },
	ColorF32 { red: 0.71922025, green: 0.81697905, blue: 1.00000000 },
	ColorF32 { red: 0.71756043, green: 0.81583783, blue: 1.00000000 },
	ColorF32 { red: 0.71593234, green: 0.81471775, blue: 1.00000000 },
	ColorF32 { red: 0.71433510, green: 0.81361825, blue: 1.00000000 },
	ColorF32 { red: 0.71276788, green: 0.81253878, blue: 1.00000000 },
	ColorF32 { red: 0.71122987, green: 0.81147883, blue: 1.00000000 },
	ColorF32 { red: 0.70972029, green: 0.81043789, blue: 1.00000000 },
	ColorF32 { red: 0.70823838, green: 0.80941546, blue: 1.00000000 },
	ColorF32 { red: 0.70678342, green: 0.80841109, blue: 1.00000000 },
	ColorF32 { red: 0.70535469, green: 0.80742432, blue: 1.00000000 },
	ColorF32 { red: 0.70395153, green: 0.80645469, blue: 1.00000000 },
	ColorF32 { red: 0.70257327, green: 0.80550180, blue: 1.00000000 },
	ColorF32 { red: 0.70121928, green: 0.80456522, blue: 1.00000000 },
	ColorF32 { red: 0.69988894, green: 0.80364455, blue: 1.00000000 },
	ColorF32 { red: 0.69858167, green: 0.80273941, blue: 1.00000000 },
	ColorF32 { red: 0.69729688, green: 0.80184943, blue: 1.00000000 },
	ColorF32 { red: 0.69603402, green: 0.80097423, blue: 1.00000000 },
	ColorF32 { red: 0.69479255, green: 0.80011347, blue: 1.00000000 },
	ColorF32 { red: 0.69357196, green: 0.79926681, blue: 1.00000000 },
	ColorF32 { red: 0.69237173, green: 0.79843391, blue: 1.00000000 },
	ColorF32 { red: 0.69119138, green: 0.79761446, blue: 1.00000000 }, // 15_000K
	ColorF32 { red: 0.69003044, green: 0.79680814, blue: 1.00000000 },
	ColorF32 { red: 0.68888844, green: 0.79601466, blue: 1.00000000 },
	ColorF32 { red: 0.68776494, green: 0.79523371, blue: 1.00000000 },
	ColorF32 { red: 0.68665951, green: 0.79446502, blue: 1.00000000 },
	ColorF32 { red: 0.68557173, green: 0.79370830, blue: 1.00000000 },
	ColorF32 { red: 0.68450119, green: 0.79296330, blue: 1.00000000 },
	ColorF32 { red: 0.68344751, green: 0.79222975, blue: 1.00000000 },
	ColorF32 { red: 0.68241029, green: 0.79150740, blue: 1.00000000 },
	ColorF32 { red: 0.68138918, green: 0.79079600, blue: 1.00000000 },
	ColorF32 { red: 0.68038380, green: 0.79009531, blue: 1.00000000 },
	ColorF32 { red: 0.67939381, green: 0.78940511, blue: 1.00000000 },
	ColorF32 { red: 0.67841888, green: 0.78872517, blue: 1.00000000 },
	ColorF32 { red: 0.67745866, green: 0.78805526, blue: 1.00000000 },
	ColorF32 { red: 0.67651284, green: 0.78739518, blue: 1.00000000 },
	ColorF32 { red: 0.67558112, green: 0.78674472, blue: 1.00000000 },
	ColorF32 { red: 0.67466317, green: 0.78610368, blue: 1.00000000 },
	ColorF32 { red: 0.67375872, green: 0.78547186, blue: 1.00000000 },
	ColorF32 { red: 0.67286748, green: 0.78484907, blue: 1.00000000 },
	ColorF32 { red: 0.67198916, green: 0.78423512, blue: 1.00000000 },
	ColorF32 { red: 0.67112350, green: 0.78362984, blue: 1.00000000 },
	ColorF32 { red: 0.67027024, green: 0.78303305, blue: 1.00000000 },
	ColorF32 { red: 0.66942911, green: 0.78244457, blue: 1.00000000 },
	ColorF32 { red: 0.66859988, green: 0.78186425, blue: 1.00000000 },
	ColorF32 { red: 0.66778228, green: 0.78129191, blue: 1.00000000 },
	ColorF32 { red: 0.66697610, green: 0.78072740, blue: 1.00000000 },
	ColorF32 { red: 0.66618110, green: 0.78017057, blue: 1.00000000 },
	ColorF32 { red: 0.66539706, green: 0.77962127, blue: 1.00000000 },
	ColorF32 { red: 0.66462376, green: 0.77907934, blue: 1.00000000 },
	ColorF32 { red: 0.66386098, green: 0.77854465, blue: 1.00000000 },
	ColorF32 { red: 0.66310852, green: 0.77801705, blue: 1.00000000 },
	ColorF32 { red: 0.66236618, green: 0.77749642, blue: 1.00000000 },
	ColorF32 { red: 0.66163375, green: 0.77698261, blue: 1.00000000 },
	ColorF32 { red: 0.66091106, green: 0.77647551, blue: 1.00000000 },
	ColorF32 { red: 0.66019791, green: 0.77597498, blue: 1.00000000 },
	ColorF32 { red: 0.65949412, green: 0.77548090, blue: 1.00000000 },
	ColorF32 { red: 0.65879952, green: 0.77499315, blue: 1.00000000 },
	ColorF32 { red: 0.65811392, green: 0.77451161, blue: 1.00000000 },
	ColorF32 { red: 0.65743716, green: 0.77403618, blue: 1.00000000 },
	ColorF32 { red: 0.65676908, green: 0.77356673, blue: 1.00000000 },
	ColorF32 { red: 0.65610952, green: 0.77310316, blue: 1.00000000 },
	ColorF32 { red: 0.65545831, green: 0.77264537, blue: 1.00000000 },
	ColorF32 { red: 0.65481530, green: 0.77219324, blue: 1.00000000 },
	ColorF32 { red: 0.65418036, green: 0.77174669, blue: 1.00000000 },
	ColorF32 { red: 0.65355332, green: 0.77130560, blue: 1.00000000 },
	ColorF32 { red: 0.65293404, green: 0.77086988, blue: 1.00000000 },
	ColorF32 { red: 0.65232240, green: 0.77043944, blue: 1.00000000 },
	ColorF32 { red: 0.65171824, green: 0.77001419, blue: 1.00000000 },
	ColorF32 { red: 0.65112144, green: 0.76959404, blue: 1.00000000 },
	ColorF32 { red: 0.65053187, green: 0.76917889, blue: 1.00000000 },
	ColorF32 { red: 0.64994941, green: 0.76876866, blue: 1.00000000 }, // 20_000K
	ColorF32 { red: 0.64937392, green: 0.76836326, blue: 1.00000000 },
	ColorF32 { red: 0.64880528, green: 0.76796263, blue: 1.00000000 },
	ColorF32 { red: 0.64824339, green: 0.76756666, blue: 1.00000000 },
	ColorF32 { red: 0.64768812, green: 0.76717529, blue: 1.00000000 },
	ColorF32 { red: 0.64713935, green: 0.76678844, blue: 1.00000000 },
	ColorF32 { red: 0.64659699, green: 0.76640603, blue: 1.00000000 },
	ColorF32 { red: 0.64606092, green: 0.76602798, blue: 1.00000000 },
	ColorF32 { red: 0.64553103, green: 0.76565424, blue: 1.00000000 },
	ColorF32 { red: 0.64500722, green: 0.76528472, blue: 1.00000000 },
	ColorF32 { red: 0.64448939, green: 0.76491935, blue: 1.00000000 },
	ColorF32 { red: 0.64397745, green: 0.76455808, blue: 1.00000000 },
	ColorF32 { red: 0.64347129, green: 0.76420082, blue: 1.00000000 },
	ColorF32 { red: 0.64297081, green: 0.76384753, blue: 1.00000000 },
	ColorF32 { red: 0.64247594, green: 0.76349813, blue: 1.00000000 },
	ColorF32 { red: 0.64198657, green: 0.76315256, blue: 1.00000000 },
	ColorF32 { red: 0.64150261, green: 0.76281076, blue: 1.00000000 },
	ColorF32 { red: 0.64102399, green: 0.76247267, blue: 1.00000000 },
	ColorF32 { red: 0.64055061, green: 0.76213824, blue: 1.00000000 },
	ColorF32 { red: 0.64008239, green: 0.76180740, blue: 1.00000000 },
	ColorF32 { red: 0.63961926, green: 0.76148010, blue: 1.00000000 },
	ColorF32 { red: 0.63916112, green: 0.76115628, blue: 1.00000000 },
	ColorF32 { red: 0.63870790, green: 0.76083590, blue: 1.00000000 },
	ColorF32 { red: 0.63825953, green: 0.76051890, blue: 1.00000000 },
	ColorF32 { red: 0.63781592, green: 0.76020522, blue: 1.00000000 },
	ColorF32 { red: 0.63737701, green: 0.75989482, blue: 1.00000000 },
	ColorF32 { red: 0.63694273, green: 0.75958764, blue: 1.00000000 },
	ColorF32 { red: 0.63651299, green: 0.75928365, blue: 1.00000000 },
	ColorF32 { red: 0.63608774, green: 0.75898278, blue: 1.00000000 },
	ColorF32 { red: 0.63566691, green: 0.75868499, blue: 1.00000000 },
	ColorF32 { red: 0.63525042, green: 0.75839025, blue: 1.00000000 },
	ColorF32 { red: 0.63483822, green: 0.75809849, blue: 1.00000000 },
	ColorF32 { red: 0.63443023, green: 0.75780969, blue: 1.00000000 },
	ColorF32 { red: 0.63402641, green: 0.75752379, blue: 1.00000000 },
	ColorF32 { red: 0.63362667, green: 0.75724075, blue: 1.00000000 },
	ColorF32 { red: 0.63323097, green: 0.75696053, blue: 1.00000000 },
	ColorF32 { red: 0.63283925, green: 0.75668310, blue: 1.00000000 },
	ColorF32 { red: 0.63245144, green: 0.75640840, blue: 1.00000000 },
	ColorF32 { red: 0.63206749, green: 0.75613641, blue: 1.00000000 },
	ColorF32 { red: 0.63168735, green: 0.75586707, blue: 1.00000000 },
	ColorF32 { red: 0.63131096, green: 0.75560036, blue: 1.00000000 },
	ColorF32 { red: 0.63093826, green: 0.75533624, blue: 1.00000000 },
	ColorF32 { red: 0.63056920, green: 0.75507467, blue: 1.00000000 },
	ColorF32 { red: 0.63020374, green: 0.75481562, blue: 1.00000000 },
	ColorF32 { red: 0.62984181, green: 0.75455904, blue: 1.00000000 },
	ColorF32 { red: 0.62948337, green: 0.75430491, blue: 1.00000000 },
	ColorF32 { red: 0.62912838, green: 0.75405319, blue: 1.00000000 },
	ColorF32 { red: 0.62877678, green: 0.75380385, blue: 1.00000000 },
	ColorF32 { red: 0.62842852, green: 0.75355685, blue: 1.00000000 },
	ColorF32 { red: 0.62808356, green: 0.75331217, blue: 1.00000000 },
	ColorF32 { red: 0.62774186, green: 0.75306977, blue: 1.00000000 }, // 25_000K
	ColorF32 { red: 0.62740336, green: 0.75282962, blue: 1.00000000 }, // 25_100K
];

/// Returns `None` if the temperature is out of the bounds that we can calculate for.
fn get_white_point(temperature: u32) -> Option<ColorF32> {
	let from_index = usize::try_from((temperature - 1000) / 100).unwrap();
	let t = (temperature % 100) as f32 / 100.0;
	Some(ColorF32::lerp(
		*BLACK_BODY_COLOR.get(from_index)?,
		*BLACK_BODY_COLOR.get(from_index + 1)?,
		t,
	))
}

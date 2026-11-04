use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Bounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}
impl Bounds {
    pub fn valid(&self) -> bool {
        [self.west, self.south, self.east, self.north]
            .iter()
            .all(|x| x.is_finite())
            && self.west >= -180.
            && self.east <= 180.
            && self.south >= -85.
            && self.north <= 85.
            && self.east > self.west
            && self.north > self.south
    }
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.south && lat <= self.north && lon >= self.west && lon <= self.east
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start_unix_s: f64,
    pub end_unix_s: f64,
    #[serde(default = "one")]
    pub weight: f64,
}
fn one() -> f64 {
    1.
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TimePrior {
    #[serde(default)]
    pub windows: Vec<TimeWindow>,
    pub estimate_unix_s: Option<f64>,
    pub sigma_s: Option<f64>,
}
impl TimePrior {
    pub fn log_weight(&self, t: f64) -> Option<f64> {
        let mut w = 0.;
        for x in &self.windows {
            if t >= x.start_unix_s && t <= x.end_unix_s {
                w += x.weight;
            }
        }
        if !self.windows.is_empty() && w == 0. {
            return None;
        }
        let mut p = if self.windows.is_empty() { 0. } else { w.ln() };
        if let (Some(mu), Some(sd)) = (self.estimate_unix_s, self.sigma_s) {
            p -= 0.5 * ((t - mu) / sd).powi(2);
        }
        Some(p)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub coherent_ridges: bool,
    pub robust_shape: bool,
    pub spectral_refine: bool,
    pub spectral_max_cells: usize,
    pub min_hz: f64,
    pub max_hz: f64,
    pub window_s: f64,
    pub hop_s: f64,
    pub multiresolution: bool,
    pub max_audio_s: f64,
    pub min_event_s: f64,
    pub min_drop_hz: f64,
    pub peak_threshold_db: f64,
    pub peak_candidates: usize,
    pub max_events: usize,
    pub bounds: Option<Bounds>,
    pub time_prior: TimePrior,
    pub grid_spacing_m: f64,
    pub mode_separation_m: f64,
    pub refinement_levels: usize,
    pub max_cells: usize,
    pub max_hypotheses: usize,
    pub clock_step_s: f64,
    pub clock_radius_s: f64,
    pub sound_speed_m_s: f64,
    pub wind_east_m_s: f64,
    pub wind_north_m_s: f64,
    pub wind_up_m_s: f64,
    pub observer_height_ellipsoid_m: f64,
    pub max_range_m: f64,
    pub frequency_sigma_hz: f64,
    pub max_track_gap_s: f64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            coherent_ridges: false,
            robust_shape: false,
            spectral_refine: false,
            spectral_max_cells: 512,
            min_hz: 35.,
            max_hz: 1000.,
            window_s: 2.,
            hop_s: 0.25,
            multiresolution: true,
            max_audio_s: 900.,
            min_event_s: 8.,
            min_drop_hz: 2.,
            peak_threshold_db: 9.,
            peak_candidates: 24,
            max_events: 6,
            bounds: None,
            time_prior: TimePrior::default(),
            grid_spacing_m: 400.,
            mode_separation_m: 300.,
            refinement_levels: 2,
            max_cells: 2500,
            max_hypotheses: 100_000,
            clock_step_s: 2.,
            clock_radius_s: 20.,
            sound_speed_m_s: 343.,
            wind_east_m_s: 0.,
            wind_north_m_s: 0.,
            wind_up_m_s: 0.,
            observer_height_ellipsoid_m: 0.,
            max_range_m: 20_000.,
            frequency_sigma_hz: 1.,
            max_track_gap_s: 30.,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        let pairs = [
            ("min_hz", self.min_hz, 10., 5000.),
            ("max_hz", self.max_hz, 20., 8000.),
            ("window_s", self.window_s, 0.25, 8.),
            ("hop_s", self.hop_s, 0.05, 2.),
            ("max_audio_s", self.max_audio_s, 1., 3600.),
            ("min_event_s", self.min_event_s, 0.5, 120.),
            ("min_drop_hz", self.min_drop_hz, 0.1, 100.),
            ("peak_threshold_db", self.peak_threshold_db, 1., 60.),
            ("grid_spacing_m", self.grid_spacing_m, 25., 20_000.),
            ("mode_separation_m", self.mode_separation_m, 1., 50_000.),
            ("clock_step_s", self.clock_step_s, 0.1, 20.),
            ("clock_radius_s", self.clock_radius_s, 0., 60.),
            ("sound_speed_m_s", self.sound_speed_m_s, 300., 370.),
            ("wind_east_m_s", self.wind_east_m_s, -60., 60.),
            ("wind_north_m_s", self.wind_north_m_s, -60., 60.),
            ("wind_up_m_s", self.wind_up_m_s, -30., 30.),
            ("max_range_m", self.max_range_m, 100., 50_000.),
            ("frequency_sigma_hz", self.frequency_sigma_hz, 0.1, 20.),
            ("max_track_gap_s", self.max_track_gap_s, 0.1, 120.),
            (
                "observer_height_ellipsoid_m",
                self.observer_height_ellipsoid_m,
                -500.,
                9000.,
            ),
        ];
        for (n, v, lo, hi) in pairs {
            if !v.is_finite() || v < lo || v > hi {
                return Err(format!("{n} must be between {lo} and {hi}"));
            }
        }
        if self.max_hz <= self.min_hz || self.hop_s > self.window_s {
            return Err("Invalid spectral band or hop".into());
        }
        if self.max_events == 0
            || self.spectral_max_cells == 0
            || self.spectral_max_cells > 5000
            || self.peak_candidates < 4
            || self.peak_candidates > 128
            || self.max_events > 16
            || self.max_cells == 0
            || self.max_cells > 100_000
            || self.max_hypotheses == 0
            || self.max_hypotheses > 2_000_000
            || self.refinement_levels > 4
        {
            return Err("Resource limits are out of range".into());
        }
        if self.bounds.is_some_and(|b| !b.valid()) {
            return Err("Invalid bounds (antimeridian crossing not supported)".into());
        }
        for w in &self.time_prior.windows {
            if !w.start_unix_s.is_finite()
                || !w.end_unix_s.is_finite()
                || w.start_unix_s.abs() > 1e12
                || w.end_unix_s.abs() > 1e12
                || w.end_unix_s < w.start_unix_s
                || !w.weight.is_finite()
                || w.weight <= 0.
                || w.weight > 1e6
            {
                return Err("Invalid time window".into());
            }
        }
        match (self.time_prior.estimate_unix_s, self.time_prior.sigma_s) {
            (Some(m), Some(s))
                if m.is_finite()
                    && m.abs() <= 1e12
                    && s.is_finite()
                    && (0.001..=1e9).contains(&s) => {}
            (None, None) => {}
            _ => {
                return Err("A time estimate requires sigma_s between 0.001 and 1e9 seconds".into())
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackPoint {
    pub time_unix_s: f64,
    pub lat: f64,
    pub lon: f64,
    pub height_ellipsoid_m: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flight {
    pub id: String,
    #[serde(default)]
    pub aircraft_type: String,
    pub points: Vec<TrackPoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataPackage {
    pub schema_version: u32,
    pub name: String,
    pub source: String,
    pub license: String,
    #[serde(default = "wgs84_frame")]
    pub coordinate_frame: String,
    #[serde(default = "utc_clock")]
    pub clock_domain: String,
    pub coverage_start_unix_s: f64,
    pub coverage_end_unix_s: f64,
    pub search_bounds: Bounds,
    #[serde(default)]
    pub synthetic: bool,
    #[serde(default)]
    pub coverage_notes: Vec<String>,
    pub flights: Vec<Flight>,
}
fn wgs84_frame() -> String {
    "wgs84".into()
}
fn utc_clock() -> String {
    "utc".into()
}
impl DataPackage {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !self.search_bounds.valid()
            || !self.coverage_start_unix_s.is_finite()
            || !self.coverage_end_unix_s.is_finite()
            || self.coverage_end_unix_s <= self.coverage_start_unix_s
        {
            return Err("Invalid package schema or coverage".into());
        }
        if self.flights.len() > 10_000 {
            return Err("Package exceeds 10,000 flight limit; split into tiles".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut total = 0;
        for f in &self.flights {
            if f.id.is_empty() || !ids.insert(&f.id) || f.points.len() < 2 {
                return Err("Flights require unique IDs and at least two points".into());
            }
            total += f.points.len();
            if total > 2_000_000 {
                return Err("Package exceeds two million points".into());
            }
            for p in &f.points {
                if ![p.time_unix_s, p.lat, p.lon, p.height_ellipsoid_m]
                    .iter()
                    .all(|v| v.is_finite())
                    || p.lat.abs() > 90.
                    || p.lon.abs() > 180.
                    || p.height_ellipsoid_m < -500.
                    || p.height_ellipsoid_m > 30_000.
                    || p.time_unix_s < self.coverage_start_unix_s
                    || p.time_unix_s > self.coverage_end_unix_s
                {
                    return Err(format!("Invalid or out-of-coverage point in {}", f.id));
                }
            }
            if f.points
                .windows(2)
                .any(|p| p[1].time_unix_s <= p[0].time_unix_s)
            {
                return Err(format!("Non-increasing track times in {}", f.id));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RidgePoint {
    pub time_s: f64,
    pub frequency_hz: f64,
    pub strength_db: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub id: usize,
    pub start_s: f64,
    pub end_s: f64,
    pub center_s: f64,
    pub drop_hz: f64,
    pub harmonics: usize,
    pub points: Vec<RidgePoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spectrogram {
    pub times_s: Vec<f64>,
    pub min_hz: f64,
    pub bin_hz: f64,
    pub bins: usize,
    pub power_db: Vec<f32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioSummary {
    pub sample_rate: u32,
    pub channels: usize,
    pub analyzed_channel: usize,
    pub duration_s: f64,
    pub analyzed_duration_s: f64,
    pub clipped_fraction: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventMatch {
    #[serde(default = "one")]
    pub event_coverage: f64,
    #[serde(default = "one")]
    pub inlier_fraction: f64,
    pub event_id: usize,
    pub flight_id: String,
    pub tone_hz: f64,
    pub rms_hz: f64,
    pub shape_gain: f64,
    pub predicted: Vec<RidgePoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub lat: f64,
    pub lon: f64,
    pub start_unix_s: f64,
    pub score: f64,
    pub matched_events: usize,
    pub cell_size_m: f64,
    pub matches: Vec<EventMatch>,
    #[serde(default)]
    pub ridge_score: Option<f64>,
    #[serde(default)]
    pub spectral_matches: Vec<SpectralMatch>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpectralMatch {
    pub flight_id: String,
    pub tone_hz: f64,
    pub contrast_db: f64,
    #[serde(default)]
    pub window_start_s: f64,
    #[serde(default)]
    pub window_end_s: f64,
    pub predicted: Vec<RidgePoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchSummary {
    #[serde(default)]
    pub anchor_fits_evaluated: usize,
    pub bounds: Bounds,
    pub hypotheses_evaluated: usize,
    pub cells_evaluated: usize,
    pub budget_exhausted: bool,
    pub method: String,
    pub time_prior: TimePrior,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub schema_version: u32,
    pub engine_version: String,
    pub status: String,
    pub synthetic: bool,
    pub input_audio_sha256: String,
    #[serde(default = "wgs84_frame")]
    pub coordinate_frame: String,
    #[serde(default = "utc_clock")]
    pub clock_domain: String,
    pub config: Config,
    pub audio: AudioSummary,
    pub events: Vec<Event>,
    pub spectrogram: Spectrogram,
    pub candidates: Vec<Candidate>,
    pub heatmap: Vec<Candidate>,
    pub search: Option<SearchSummary>,
    pub warnings: Vec<String>,
    pub data_source: Option<String>,
    pub flights: Vec<Flight>,
    pub score_kind: String,
}

use crate::model::{Flight, TrackPoint};
#[derive(Clone, Copy, Debug, Default)]
pub struct Vec3(pub f64, pub f64, pub f64);
impl Vec3 {
    pub fn sub(self, b: Self) -> Self {
        Self(self.0 - b.0, self.1 - b.1, self.2 - b.2)
    }
    pub fn add(self, b: Self) -> Self {
        Self(self.0 + b.0, self.1 + b.1, self.2 + b.2)
    }
    pub fn mul(self, k: f64) -> Self {
        Self(self.0 * k, self.1 * k, self.2 * k)
    }
    pub fn dot(self, b: Self) -> f64 {
        self.0 * b.0 + self.1 * b.1 + self.2 * b.2
    }
    pub fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }
}
pub fn ecef(lat: f64, lon: f64, h: f64) -> Vec3 {
    let (sl, cl) = lat.to_radians().sin_cos();
    let (so, co) = lon.to_radians().sin_cos();
    let n = 6_378_137. / (1. - 6.69437999014e-3 * sl * sl).sqrt();
    Vec3(
        (n + h) * cl * co,
        (n + h) * cl * so,
        (n * (1. - 6.69437999014e-3) + h) * sl,
    )
}
#[derive(Clone)]
pub struct Trajectory {
    pub id: String,
    pub t: Vec<f64>,
    pub p: Vec<Vec3>,
}
impl Trajectory {
    pub fn new(f: &Flight) -> Self {
        Self {
            id: f.id.clone(),
            t: f.points.iter().map(|p| p.time_unix_s).collect(),
            p: f.points
                .iter()
                .map(|p| ecef(p.lat, p.lon, p.height_ellipsoid_m))
                .collect(),
        }
    }
    /// Piecewise linear interpolation with its exact derivative. Never bridge gaps or extrapolate.
    pub fn state(&self, t: f64, max_gap: f64) -> Option<(Vec3, Vec3)> {
        if t < self.t[0] || t > *self.t.last()? {
            return None;
        }
        let j = self
            .t
            .partition_point(|v| *v <= t)
            .saturating_sub(1)
            .min(self.t.len() - 2);
        let dt = self.t[j + 1] - self.t[j];
        if dt > max_gap {
            return None;
        }
        let v = self.p[j + 1].sub(self.p[j]).mul(1. / dt);
        if v.norm() > 350. {
            return None;
        }
        Some((self.p[j].add(v.mul(t - self.t[j])), v))
    }
    pub fn cpa(&self, x: Vec3, max_gap: f64) -> Option<(f64, f64)> {
        self.t
            .windows(2)
            .enumerate()
            .filter_map(|(i, t)| {
                let dt = t[1] - t[0];
                if dt > max_gap {
                    return None;
                }
                let d = self.p[i + 1].sub(self.p[i]);
                let a = (x.sub(self.p[i]).dot(d) / d.dot(d).max(1e-12)).clamp(0., 1.);
                let r = self.p[i].add(d.mul(a)).sub(x).norm();
                Some((t[0] + a * dt, r))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
    pub fn doppler(&self, x: Vec3, t_receive: f64, c: f64, max_gap: f64) -> Option<(f64, f64)> {
        self.doppler_with_wind(x, t_receive, c, max_gap, Vec3::default())
    }
    pub fn doppler_with_wind(
        &self,
        x: Vec3,
        t_receive: f64,
        c: f64,
        max_gap: f64,
        wind: Vec3,
    ) -> Option<(f64, f64)> {
        let mut tau = t_receive
            - (self
                .state(t_receive.clamp(self.t[0], *self.t.last()?), max_gap)?
                .0
                .sub(x)
                .norm()
                / c);
        for _ in 0..12 {
            let (p, v) = self.state(tau, max_gap)?;
            let d = x.sub(p).sub(wind.mul(t_receive - tau));
            let r = d.norm();
            if r < 1. {
                return None;
            }
            let k = d.mul(1. / r);
            let deriv = 1. + k.dot(wind.sub(v)) / c;
            if deriv < 0.1 {
                return None;
            }
            let residual = (tau - t_receive) + r / c;
            if residual.abs() < 1e-5 {
                let c_eff = c + wind.dot(k);
                return Some((c_eff / (c_eff - v.dot(k)), p.sub(x).norm()));
            }
            tau -= residual / deriv;
        }
        None
    }
}
pub fn enu_vector(lat: f64, lon: f64, east: f64, north: f64, up: f64) -> Vec3 {
    let (sl, cl) = lat.to_radians().sin_cos();
    let (so, co) = lon.to_radians().sin_cos();
    Vec3(
        -so * east - sl * co * north + cl * co * up,
        co * east - sl * so * north + cl * so * up,
        cl * north + sl * up,
    )
}
pub fn distance_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dlat = (b.0 - a.0).to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let h = (dlat / 2.).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.).sin().powi(2);
    12_742_000. * h.sqrt().min(1.).asin()
}
pub fn point(t: f64, lat: f64, lon: f64, h: f64) -> TrackPoint {
    TrackPoint {
        time_unix_s: t,
        lat,
        lon,
        height_ellipsoid_m: h,
    }
}

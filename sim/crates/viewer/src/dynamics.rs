//! Longitudinal train dynamics: a Rust stand-in for what Source's VPhysics did to the bogeys.
//!
//! The force law is the one in `gmod_train_bogey` (motor force x motor power, brake-cylinder
//! force, sign handling for `Reversed`). Source's own friction and mass scaling are replaced
//! by two calibration constants chosen so the 81-717 reaches roughly its real performance
//! (about 1 m/s^2 under full traction, about 1 m/s^2 service braking).

/// Effective mass the bogey forces of ONE car act on, kg-equivalent (a consist scales it with its car count).
const MASS: f64 = 40_000.0;
/// Brake forces in the Lua are tuned for Source's friction model; scale to real-world deceleration.
const BRAKE_SCALE: f64 = 0.85;
/// Rolling plus air resistance: a = R0 + R2 * v_kmh^2 (m/s^2).
const RES_0: f64 = 0.005;
const RES_2: f64 = 1.0e-5;

/// What the train's Lua sets on one bogey each tick.
#[derive(Clone, Copy, Debug)]
pub struct BogeyIn {
    pub motor_force: f64,
    pub motor_power: f64,
    pub reversed: bool,
    pub pneumatic_force: f64,
    pub brake_cylinder: f64,
    pub parking: f64,
    pub pneumatic_pow: f64,
}

impl Default for BogeyIn {
    fn default() -> Self {
        BogeyIn { motor_force: 30_000.0, motor_power: 0.0, reversed: false, pneumatic_force: 100_000.0, brake_cylinder: 0.0, parking: 0.0, pneumatic_pow: 1.0 }
    }
}

/// What the Lua reads back from one bogey.
#[derive(Clone, Copy, Debug, Default)]
pub struct BogeyOut {
    /// km/h, absolute.
    pub speed: f64,
    pub speed_sign: f64,
    /// m/s^2
    pub acceleration: f64,
}

#[derive(Default)]
pub struct Dynamics {
    /// Signed speed along the car, m/s (positive towards the cab end).
    pub v: f64,
    /// Distance travelled, m.
    pub pos: f64,
    /// Track gradient, per mille, positive uphill in the +x direction.
    pub grade: f64,
    /// Curve radius under the train, m (0 or infinity = straight).
    pub radius: f64,
    prev_speed: Vec<f64>,
    /// Tension in each coupling after the last step, N (positive = the cars pull on each other). One entry per coupling.
    pub coupler: Vec<f64>,
}

impl Dynamics {
    /// Direction (+1 / -1 along the car) in which a bogey's positive force pushes.
    /// The front bogey faces backwards and the rear forwards, `Reversed` flips each.
    fn push_dir(front: bool, reversed: bool) -> f64 {
        let r = if reversed { -1.0 } else { 1.0 };
        if front {
            r
        } else {
            -r
        }
    }

    /// `bogeys` holds front, rear of the head car, then front, rear of each coupled car.
    pub fn step(&mut self, dt: f64, bogeys: &[BogeyIn]) -> Vec<BogeyOut> {
        let dt = dt.clamp(1e-4, 0.1);
        let mut force = 0.0;
        let mut hold = false;
        let mut traction = 0.0f64;
        let mut out = vec![BogeyOut::default(); bogeys.len()];
        let mut car_force = vec![0.0f64; bogeys.len().div_ceil(2)];
        self.prev_speed.resize(bogeys.len(), 0.0);
        let cars = (bogeys.len() / 2).max(1) as f64;

        for (i, b) in bogeys.iter().enumerate() {
            let e = Self::push_dir(i % 2 == 0, b.reversed);
            let local = self.v * e * 3.6;
            let sign = if local < 0.0 { -1.0 } else { 1.0 };
            let abs = local.abs();

            // motor: positive power pushes along `e`, negative power resists the motion
            let mp = if b.motor_power > 0.0 { b.motor_power } else { b.motor_power * sign };
            let mp = mp.clamp(-1.0, 1.0);
            // gmod_train_bogey: forces grow on slopes steeper than 3 degrees
            let pitch = (self.grade / 1000.0).atan().to_degrees() * sign;
            let mut slopemul = 1.0;
            if mp < 0.0 && pitch > 3.0 {
                slopemul += ((pitch.abs() - 3.0) / 3.0).clamp(0.0, 1.0) * 1.5;
            } else {
                slopemul += ((pitch - 3.0) / 3.0).clamp(0.0, 1.0) * 1.5;
            }
            let slopemulb = if pitch.abs() > 3.0 { 1.0 + ((pitch.abs() - 3.0) / 3.0).clamp(0.0, 1.0) * 0.7 } else { 1.0 };
            let motor = b.motor_force * mp * slopemul;
            traction += motor.abs();

            // friction brakes
            let bcp = (((b.brake_cylinder / 2.7 + b.parking / 1.6).max(0.0)).powf(b.pneumatic_pow) * 2.7) / 4.5;
            let factor = (0.5 * abs).clamp(0.0, 1.0) * (1.0 + ((2.0 - abs) / 2.0).clamp(0.0, 1.0) * 0.5);
            let pneumatic = if bcp >= 0.05 { -sign * factor * b.pneumatic_force * bcp * BRAKE_SCALE * slopemulb } else { 0.0 };
            if bcp * 4.5 > 1.5 - pitch.abs().clamp(0.0, 1.0) && abs < 1.0 {
                hold = true;
            }

            force += e * (motor + pneumatic);
            car_force[i / 2] += e * (motor + pneumatic);

        }

        let v_kmh = self.v.abs() * 3.6;
        let resist = (RES_0 + RES_2 * v_kmh * v_kmh) * if self.v < 0.0 { -1.0 } else { 1.0 };
        // curve resistance (Röckl): 700 / (R - 55) kgf per tonne
        let curve = if self.radius > 60.0 && self.radius.is_finite() { 700.0 / (self.radius - 55.0) * 0.00981 } else { 0.0 };
        let a = force / (MASS * cars) - 9.81 * self.grade / 1000.0 - if self.v == 0.0 { 0.0 } else { resist + curve * self.v.signum() };
        // a rigid consist: each car would accelerate on its own by `own`, the couplings carry the difference
        // (along the car axis, from the head car backwards)
        let per_car = MASS;
        let drag = if self.v == 0.0 { 0.0 } else { resist + curve * self.v.signum() };
        let mut carry = 0.0;
        self.coupler.clear();
        for f in car_force.iter().take(car_force.len().saturating_sub(1)) {
            let own = f / per_car - 9.81 * self.grade / 1000.0 - drag;
            carry += per_car * (own - a);
            self.coupler.push(carry);
        }
        let mut v = self.v + a * dt;
        if self.v != 0.0 && v * self.v < 0.0 {
            v = 0.0; // brakes and drag stop the car, they never reverse it
        }
        if hold && v.abs() * 3.6 < 1.0 && traction < 3_000.0 {
            v = 0.0; // parked on the brakes
        }
        self.pos += 0.5 * (self.v + v) * dt;
        self.v = v;
        for (i, b) in bogeys.iter().enumerate() {
            let local = v * Self::push_dir(i % 2 == 0, b.reversed) * 3.6;
            let abs = local.abs();
            let sign = if local < 0.0 { -1.0 } else { 1.0 };
            out[i] = BogeyOut { speed: abs, speed_sign: if b.reversed { -sign } else { sign }, acceleration: 0.277778 * (abs - self.prev_speed[i]) / dt };
            self.prev_speed[i] = abs;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward(power: f64, bcp: f64) -> [BogeyIn; 2] {
        // reverser forward: front bogey not reversed, rear reversed, both push towards +x
        let mut f = BogeyIn { motor_force: 22_500.0, motor_power: power, pneumatic_force: 48_000.0, brake_cylinder: bcp, ..Default::default() };
        let mut r = f;
        f.reversed = false;
        r.reversed = true;
        [f, r]
    }

    #[test]
    fn full_traction_accelerates_about_one_ms2() {
        let mut d = Dynamics::default();
        for _ in 0..60 {
            d.step(1.0 / 60.0, &forward(0.5, 0.0));
        }
        // 1 s of full traction (each bogey at 0.5) from rest
        assert!(d.v > 0.4 && d.v < 1.5, "v = {}", d.v);
    }

    #[test]
    fn reaches_sensible_top_speed_and_brakes_to_a_stop() {
        let mut d = Dynamics::default();
        for _ in 0..(60 * 30) {
            d.step(1.0 / 60.0, &forward(0.5, 0.0));
        }
        let kmh = d.v * 3.6;
        assert!(kmh > 50.0 && kmh < 200.0, "speed after 30 s {kmh}");
        let mut t = 0.0;
        while d.v > 0.0 && t < 60.0 {
            d.step(1.0 / 60.0, &forward(0.0, 3.5));
            t += 1.0 / 60.0;
        }
        assert!(d.v == 0.0, "never stopped");
        assert!(t > 5.0 && t < 30.0, "stopping time {t}");
    }

    #[test]
    fn downhill_rolls_and_uphill_slows() {
        let mut d = Dynamics { grade: -40.0, ..Default::default() };
        for _ in 0..600 {
            d.step(1.0 / 60.0, &forward(0.0, 0.0));
        }
        assert!(d.v > 2.0, "rolled only to {}", d.v);
        d.grade = 40.0;
        let v0 = d.v;
        for _ in 0..120 {
            d.step(1.0 / 60.0, &forward(0.0, 0.0));
        }
        assert!(d.v < v0);
    }

    #[test]
    fn curves_slow_a_coasting_train() {
        let coast = |radius: f64| {
            let mut d = Dynamics { v: 20.0, radius, ..Default::default() };
            for _ in 0..600 {
                d.step(1.0 / 60.0, &forward(0.0, 0.0));
            }
            d.v
        };
        assert!(coast(150.0) < coast(f64::INFINITY) - 0.1);
        assert!(coast(1500.0) <= coast(f64::INFINITY));
    }

    #[test]
    fn couplings_carry_the_difference_between_cars() {
        // four cars, only the head car has motors: every coupling pulls the unpowered cars along
        let mut d = Dynamics::default();
        let mut b = Vec::new();
        for car in 0..4 {
            let p = if car == 0 { 1.0 } else { 0.0 };
            b.extend(forward(p, 0.0));
        }
        d.step(0.05, &b);
        assert_eq!(d.coupler.len(), 3);
        // the first coupling pulls three cars, the last one car: 3/4 and 1/4 of the head car's push
        assert!(d.coupler[0] > d.coupler[1] && d.coupler[1] > d.coupler[2] && d.coupler[2] > 0.0, "{:?}", d.coupler);
        // motors on every car: nothing is left for the couplings
        let mut e = Dynamics::default();
        let all: Vec<BogeyIn> = (0..4).flat_map(|_| forward(1.0, 0.0)).collect();
        e.step(0.05, &all);
        assert!(e.coupler.iter().all(|c| c.abs() < 1.0), "{:?}", e.coupler);
    }

    #[test]
    fn parked_train_with_brakes_does_not_creep() {
        let mut d = Dynamics::default();
        for _ in 0..120 {
            d.step(1.0 / 60.0, &forward(0.0, 4.0));
        }
        assert_eq!(d.v, 0.0);
    }

    #[test]
    fn bogeys_report_matching_speed() {
        let mut d = Dynamics::default();
        let mut o = vec![];
        for _ in 0..120 {
            o = d.step(1.0 / 60.0, &forward(0.5, 0.0));
        }
        assert!((o[0].speed - o[1].speed).abs() < 1e-9);
        assert!((o[0].speed - d.v * 3.6).abs() < 1e-6);
        // a reversed bogey reports the opposite sign
        assert_eq!(o[0].speed_sign, -o[1].speed_sign);
    }
}

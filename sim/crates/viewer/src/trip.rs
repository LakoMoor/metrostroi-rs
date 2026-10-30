//! The driver's trip record: how accurately each station was stopped at and which rules were broken.

/// One stop at a platform.
#[derive(Clone, Debug)]
pub struct Stop {
    pub station: String,
    /// Metres left to the platform end when the train came to rest (negative = overran it).
    pub short: f32,
}

#[derive(Default, Clone, Debug)]
pub struct Trip {
    pub stops: Vec<Stop>,
    /// ARS autostop applications.
    pub autostops: u32,
    /// Doors opened while the train was moving.
    pub doors_moving: u32,
    /// Doors opened on the side without a platform.
    pub doors_wrong_side: u32,
    pub seconds: f32,
    /// Largest coupling tension of the trip, N (a rough ride shows as a high number).
    pub coupler_peak: f32,
    was_moving_open: bool,
    was_wrong_side: bool,
    was_tripped: bool,
}

impl Trip {
    /// Points taken off for one stop: nothing within -1..3 m of the platform end, growing with the error.
    pub fn stop_penalty(short: f32) -> f32 {
        if short > 3.0 {
            ((short - 3.0) * 0.8).min(10.0)
        } else if short < -1.0 {
            ((-1.0 - short) * 3.0).min(15.0)
        } else {
            0.0
        }
    }

    pub fn score(&self) -> i32 {
        let stops: f32 = self.stops.iter().map(|s| Self::stop_penalty(s.short)).sum();
        let rules = 15.0 * self.autostops as f32 + 10.0 * (self.doors_moving + self.doors_wrong_side) as f32;
        (100.0 - stops - rules).clamp(0.0, 100.0).round() as i32
    }

    pub fn stopped(&mut self, station: String, short: f32) {
        self.stops.push(Stop { station, short });
    }

    /// Feed the per-tick facts; counts each violation once, when it starts.
    pub fn observe(&mut self, dt: f32, speed_kmh: f32, doors_open: bool, doors_wrong_side: bool, tripped: bool) {
        self.seconds += dt;
        let moving_open = doors_open && speed_kmh > 3.0;
        if moving_open && !self.was_moving_open {
            self.doors_moving += 1;
        }
        self.was_moving_open = moving_open;
        let wrong = doors_open && doors_wrong_side && speed_kmh <= 3.0;
        if wrong && !self.was_wrong_side {
            self.doors_wrong_side += 1;
        }
        self.was_wrong_side = wrong;
        if tripped && !self.was_tripped {
            self.autostops += 1;
        }
        self.was_tripped = tripped;
    }

    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!("Оценка: {} / 100 · время {}:{:02}", self.score(), self.seconds as u32 / 60, self.seconds as u32 % 60)];
        out.push(format!("Автостопов АРС: {} · двери в движении: {} · двери не с той стороны: {}", self.autostops, self.doors_moving, self.doors_wrong_side));
        if self.coupler_peak > 0.0 {
            out.push(format!("Макс. усилие в сцепках: {:.0} кН", self.coupler_peak / 1000.0));
        }
        if self.stops.is_empty() {
            out.push("Остановок у платформ пока не было".into());
        }
        for s in self.stops.iter().rev().take(8) {
            let how = if s.short > 3.0 {
                format!("недотяг {:.1} м", s.short)
            } else if s.short < -1.0 {
                format!("перебег {:.1} м", -s.short)
            } else {
                "точно".to_string()
            };
            out.push(format!("{} — {how}", s.station));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_stops_score_full_marks() {
        let mut t = Trip::default();
        t.stopped("A".into(), 1.0);
        t.stopped("B".into(), 0.0);
        assert_eq!(t.score(), 100);
    }

    #[test]
    fn overrun_costs_more_than_undershoot() {
        assert!(Trip::stop_penalty(-4.0) > Trip::stop_penalty(8.0));
        assert_eq!(Trip::stop_penalty(2.0), 0.0);
    }

    #[test]
    fn violations_count_once_per_event() {
        let mut t = Trip::default();
        for _ in 0..100 {
            t.observe(0.1, 40.0, true, false, false); // one long open-while-moving event
        }
        t.observe(0.1, 0.0, false, false, false);
        t.observe(0.1, 30.0, true, false, false); // a second one
        assert_eq!(t.doors_moving, 2);
        for _ in 0..50 {
            t.observe(0.1, 0.0, true, true, true); // wrong side + autostop
        }
        assert_eq!((t.doors_wrong_side, t.autostops), (1, 1));
        assert_eq!(t.score(), 100 - 20 - 10 - 15);
    }
}

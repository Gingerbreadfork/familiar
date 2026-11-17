//! Turns presses, releases and pointer movement of one button into gestures.
//!
//! It has no clock of its own: the caller passes the time with each input and
//! calls [`Recognizer::tick`] at [`Recognizer::deadline`].

use std::time::{Duration, Instant};

use crate::actions::{Direction, Gestures};

#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// A press held this long without moving is a hold.
    pub hold: Duration,
    /// A second press within this time of the first release is a double tap.
    pub double: Duration,
    /// Movement, in sensor counts, that turns a press into a swipe.
    pub swipe: i32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { hold: Duration::from_millis(500), double: Duration::from_millis(300), swipe: 100 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Tap,
    Double,
    Hold,
    Swipe(Direction),
}

#[derive(Clone, Copy, Debug)]
enum State {
    Idle,
    Down {
        since: Instant,
        dx: i32,
        dy: i32,
        first: bool,
        fired: bool,
    },
    /// Released once; waiting to see whether a second press follows.
    Waiting {
        since: Instant,
    },
    SecondDown,
}

pub struct Recognizer {
    timing: Timing,
    double: bool,
    hold: bool,
    directions: bool,
    diagonals: bool,
    state: State,
}

/// The direction of a movement, screen coordinates with y growing downwards.
pub fn direction(dx: i32, dy: i32, diagonals: bool) -> Direction {
    if diagonals {
        let angle = (dy as f64).atan2(dx as f64).to_degrees();
        let sector = (((angle + 360.0 + 22.5) % 360.0) / 45.0) as u8;
        return match sector {
            0 => Direction::Right,
            1 => Direction::DownRight,
            2 => Direction::Down,
            3 => Direction::DownLeft,
            4 => Direction::Left,
            5 => Direction::UpLeft,
            6 => Direction::Up,
            _ => Direction::UpRight,
        };
    }
    if dx.abs() > dy.abs() {
        if dx > 0 { Direction::Right } else { Direction::Left }
    } else if dy > 0 {
        Direction::Down
    } else {
        Direction::Up
    }
}

impl Recognizer {
    pub fn new(g: &Gestures, timing: Timing) -> Self {
        Recognizer {
            timing,
            double: g.double.is_some(),
            hold: g.hold.is_some(),
            directions: g.has_directions(),
            diagonals: g.has_diagonals(),
            state: State::Idle,
        }
    }

    fn moved(&self, dx: i32, dy: i32) -> bool {
        self.directions && dx.abs().max(dy.abs()) >= self.timing.swipe
    }

    /// A press. Returns the pending tap when this press comes too late to make
    /// it a double and [`Recognizer::tick`] has not reported it yet.
    pub fn press(&mut self, now: Instant) -> Option<Trigger> {
        let down = State::Down { since: now, dx: 0, dy: 0, first: true, fired: false };
        let (next, fire) = match self.state {
            State::Waiting { since } if now.duration_since(since) < self.timing.double => (State::SecondDown, None),
            State::Waiting { .. } => (down, Some(Trigger::Tap)),
            _ => (down, None),
        };
        self.state = next;
        fire
    }

    /// Forgets a press in progress, for when the device goes away mid-press.
    pub fn cancel(&mut self) {
        if matches!(self.state, State::Down { .. } | State::SecondDown) {
            self.state = State::Idle;
        }
    }

    /// Pointer movement while held. The first report after a press is not a
    /// real movement on MX mice and is dropped.
    pub fn motion(&mut self, x: i16, y: i16) {
        if let State::Down { dx, dy, first, .. } = &mut self.state {
            if *first {
                *first = false;
            } else {
                *dx += x as i32;
                *dy += y as i32;
            }
        }
    }

    pub fn release(&mut self, now: Instant) -> Option<Trigger> {
        let (next, fire) = match self.state {
            State::Down { fired: true, .. } => (State::Idle, None),
            State::Down { dx, dy, .. } if self.moved(dx, dy) => {
                (State::Idle, Some(Trigger::Swipe(direction(dx, dy, self.diagonals))))
            }
            State::Down { .. } if self.double => (State::Waiting { since: now }, None),
            State::Down { .. } => (State::Idle, Some(Trigger::Tap)),
            State::SecondDown => (State::Idle, Some(Trigger::Double)),
            s => (s, None),
        };
        self.state = next;
        fire
    }

    pub fn tick(&mut self, now: Instant) -> Option<Trigger> {
        let moved = matches!(self.state, State::Down { dx, dy, .. } if self.moved(dx, dy));
        match &mut self.state {
            State::Down { since, fired, .. }
                if self.hold && !*fired && !moved && now.duration_since(*since) >= self.timing.hold =>
            {
                *fired = true;
                Some(Trigger::Hold)
            }
            State::Waiting { since } if now.duration_since(*since) >= self.timing.double => {
                self.state = State::Idle;
                Some(Trigger::Tap)
            }
            _ => None,
        }
    }

    /// When [`Recognizer::tick`] next needs calling, if at all. A press that has
    /// moved far enough to be a swipe cannot become a hold, so it needs none.
    pub fn deadline(&self) -> Option<Instant> {
        match self.state {
            State::Down { since, dx, dy, fired: false, .. } if self.hold && !self.moved(dx, dy) => {
                Some(since + self.timing.hold)
            }
            State::Waiting { since } => Some(since + self.timing.double),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;

    fn gestures(tap: bool, double: bool, hold: bool, dirs: bool, diag: bool) -> Gestures {
        let a = || Some(Action::None);
        Gestures {
            toggle: None,
            tap: tap.then(a).flatten(),
            double: double.then(a).flatten(),
            hold: hold.then(a).flatten(),
            up: dirs.then(a).flatten(),
            down: dirs.then(a).flatten(),
            left: dirs.then(a).flatten(),
            right: dirs.then(a).flatten(),
            up_left: diag.then(a).flatten(),
            up_right: diag.then(a).flatten(),
            down_left: diag.then(a).flatten(),
            down_right: diag.then(a).flatten(),
        }
    }

    fn ms(t: Instant, n: u64) -> Instant {
        t + Duration::from_millis(n)
    }

    #[test]
    fn tap_fires_on_release_without_double() {
        let mut r = Recognizer::new(&gestures(true, false, true, false, false), Timing::default());
        let t = Instant::now();
        r.press(t);
        assert_eq!(r.tick(ms(t, 100)), None);
        assert_eq!(r.release(ms(t, 120)), Some(Trigger::Tap));
        assert_eq!(r.deadline(), None);
    }

    #[test]
    fn hold_fires_once_and_swallows_release() {
        let mut r = Recognizer::new(&gestures(true, false, true, false, false), Timing::default());
        let t = Instant::now();
        r.press(t);
        assert_eq!(r.deadline(), Some(ms(t, 500)));
        assert_eq!(r.tick(ms(t, 500)), Some(Trigger::Hold));
        assert_eq!(r.tick(ms(t, 900)), None);
        assert_eq!(r.release(ms(t, 1000)), None);
    }

    #[test]
    fn double_tap_and_delayed_single_tap() {
        let mut r = Recognizer::new(&gestures(true, true, false, false, false), Timing::default());
        let t = Instant::now();
        r.press(t);
        assert_eq!(r.release(ms(t, 80)), None);
        r.press(ms(t, 200));
        assert_eq!(r.release(ms(t, 260)), Some(Trigger::Double));

        r.press(ms(t, 1000));
        assert_eq!(r.release(ms(t, 1080)), None);
        assert_eq!(r.deadline(), Some(ms(t, 1380)));
        assert_eq!(r.tick(ms(t, 1200)), None);
        assert_eq!(r.tick(ms(t, 1380)), Some(Trigger::Tap));
    }

    #[test]
    fn late_second_press_keeps_the_first_tap() {
        let mut r = Recognizer::new(&gestures(true, true, false, false, false), Timing::default());
        let t = Instant::now();
        r.press(t);
        assert_eq!(r.release(ms(t, 80)), None);
        assert_eq!(r.press(ms(t, 400)), Some(Trigger::Tap), "the wait ran out before tick was called");
        assert_eq!(r.release(ms(t, 460)), None);
        assert_eq!(r.tick(ms(t, 760)), Some(Trigger::Tap));
    }

    #[test]
    fn cancelled_press_never_fires() {
        let mut r = Recognizer::new(&gestures(true, false, true, false, false), Timing::default());
        let t = Instant::now();
        r.press(t);
        r.cancel();
        assert_eq!(r.deadline(), None);
        assert_eq!(r.tick(ms(t, 600)), None);
        assert_eq!(r.release(ms(t, 700)), None);
    }

    #[test]
    fn swipes_ignore_the_first_motion_report() {
        let mut r = Recognizer::new(&gestures(true, false, false, true, false), Timing::default());
        let t = Instant::now();
        r.press(t);
        r.motion(-6125, -7389);
        for _ in 0..10 {
            r.motion(0, -15);
        }
        assert_eq!(r.release(ms(t, 300)), Some(Trigger::Swipe(Direction::Up)));

        r.press(ms(t, 1000));
        r.motion(-6125, -7389);
        r.motion(3, 2);
        assert_eq!(r.release(ms(t, 1100)), Some(Trigger::Tap), "a small wobble is still a tap");
    }

    #[test]
    fn moving_suppresses_hold() {
        let mut r = Recognizer::new(&gestures(true, false, true, true, false), Timing::default());
        let t = Instant::now();
        r.press(t);
        r.motion(0, 0);
        r.motion(150, 0);
        assert_eq!(r.deadline(), None, "nothing is due, so the caller can sleep");
        assert_eq!(r.tick(ms(t, 600)), None);
        assert_eq!(r.release(ms(t, 700)), Some(Trigger::Swipe(Direction::Right)));
    }

    #[test]
    fn eight_directions() {
        assert_eq!(direction(100, 0, true), Direction::Right);
        assert_eq!(direction(100, 100, true), Direction::DownRight);
        assert_eq!(direction(0, 100, true), Direction::Down);
        assert_eq!(direction(-100, 100, true), Direction::DownLeft);
        assert_eq!(direction(-100, 0, true), Direction::Left);
        assert_eq!(direction(-100, -100, true), Direction::UpLeft);
        assert_eq!(direction(0, -100, true), Direction::Up);
        assert_eq!(direction(100, -100, true), Direction::UpRight);
        assert_eq!(direction(100, -100, false), Direction::Up);
        assert_eq!(direction(101, -100, false), Direction::Right);
    }
}

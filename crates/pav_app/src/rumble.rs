//! Controller rumble from a hook game's `FrameEffects::rumble` (PLAN §9.13, haptics). The
//! simulation host collects each tick's cues once (`Shared::rumble`); the front end plays them
//! while the game is being played and the Vibration setting is on: natively through gilrs force
//! feedback, in the browser through the Gamepad API's
//! `vibrationActuator.playEffect("dual-rumble", ...)` (Chromium; elsewhere it does nothing).

use pav_core::play::RumbleCue;
use web_time::{Duration, Instant};

/// The strongest of a batch of cues (motors and length each at their maximum).
pub fn merge(cues: &[RumbleCue]) -> Option<RumbleCue> {
    cues.iter().copied().reduce(|a, b| RumbleCue { low: a.low.max(b.low), high: a.high.max(b.high), ms: a.ms.max(b.ms) })
}

fn strength(c: &RumbleCue) -> f32 {
    c.low.max(c.high)
}

/// What is rumbling now.
#[derive(Default)]
pub struct Rumble {
    /// The cue playing and when it ends.
    current: Option<(RumbleCue, Instant)>,
    /// gilrs keeps an effect only while its handle lives.
    #[cfg(not(target_arch = "wasm32"))]
    effect: Option<gilrs::ff::Effect>,
}

impl Rumble {
    /// Starts the strongest of `cues`, unless something stronger is still playing.
    pub fn play(&mut self, gilrs: Option<&mut gilrs::Gilrs>, cues: &[RumbleCue]) {
        let now = Instant::now();
        if self.current.is_some_and(|(_, end)| now >= end) {
            self.stop();
        }
        let Some(c) = merge(cues) else { return };
        let c = RumbleCue { low: c.low.clamp(0.0, 1.0), high: c.high.clamp(0.0, 1.0), ms: c.ms.clamp(20, 2000) };
        if strength(&c) <= 0.01 {
            return;
        }
        if let Some((playing, _)) = self.current {
            if strength(&playing) > strength(&c) + 0.15 {
                return;
            }
        }
        self.current = Some((c, now + Duration::from_millis(c.ms as u64)));
        self.start(gilrs, c);
    }

    /// Stops whatever is playing (pausing, Vibration turned off).
    pub fn stop(&mut self) {
        self.current = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(e) = self.effect.take() {
                let _ = e.stop();
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start(&mut self, gilrs: Option<&mut gilrs::Gilrs>, c: RumbleCue) {
        use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Repeat, Replay, Ticks};
        let Some(g) = gilrs else { return };
        let pads: Vec<gilrs::GamepadId> =
            g.gamepads().filter(|(_, p)| p.is_connected() && p.is_ff_supported()).map(|(id, _)| id).collect();
        if pads.is_empty() {
            return;
        }
        let mag = |x: f32| (x * u16::MAX as f32) as u16;
        let play_for = Ticks::from_ms(c.ms);
        let once = Replay { play_for, ..Default::default() };
        let built = EffectBuilder::new()
            .add_effect(BaseEffect {
                kind: BaseEffectType::Strong { magnitude: mag(c.low) },
                scheduling: once,
                ..Default::default()
            })
            .add_effect(BaseEffect {
                kind: BaseEffectType::Weak { magnitude: mag(c.high) },
                scheduling: once,
                ..Default::default()
            })
            .gamepads(&pads)
            .repeat(Repeat::For(play_for))
            .finish(g);
        match built {
            Ok(e) => {
                if let Err(err) = e.play() {
                    log::debug!("rumble: {err}");
                }
                // The previous effect stops as its handle drops.
                self.effect = Some(e);
            }
            Err(err) => log::debug!("rumble: {err}"),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn start(&mut self, _gilrs: Option<&mut gilrs::Gilrs>, c: RumbleCue) {
        browser::dual_rumble(c);
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use pav_core::play::RumbleCue;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen::closure::Closure;
    use web_sys::js_sys::{Function, Object, Promise, Reflect};

    thread_local! {
        /// Swallows the promise's rejection (a pad without the effect): no console noise.
        static IGNORE: Closure<dyn FnMut(JsValue)> = Closure::new(|_: JsValue| {});
    }

    /// `pad.vibrationActuator.playEffect("dual-rumble", {...})` on every pad that has one. The
    /// actuator is looked up by name: web-sys only binds it behind its unstable-API flag.
    pub fn dual_rumble(c: RumbleCue) {
        let Some(win) = web_sys::window() else { return };
        let Ok(pads) = win.navigator().get_gamepads() else { return };
        for pad in pads.iter() {
            if pad.is_null() || pad.is_undefined() {
                continue;
            }
            let Ok(actuator) = Reflect::get(&pad, &JsValue::from_str("vibrationActuator")) else { continue };
            if actuator.is_null() || actuator.is_undefined() {
                continue;
            }
            let Some(play) =
                Reflect::get(&actuator, &JsValue::from_str("playEffect")).ok().and_then(|f| f.dyn_into::<Function>().ok())
            else {
                continue;
            };
            let params = Object::new();
            for (k, v) in [
                ("startDelay", 0.0),
                ("duration", c.ms as f64),
                ("strongMagnitude", c.low as f64),
                ("weakMagnitude", c.high as f64),
            ] {
                let _ = Reflect::set(&params, &JsValue::from_str(k), &JsValue::from_f64(v));
            }
            if let Ok(p) = play.call2(&actuator, &JsValue::from_str("dual-rumble"), &params) {
                if let Ok(p) = p.dyn_into::<Promise>() {
                    IGNORE.with(|f| {
                        let _ = p.catch(f);
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_rumbles_as_its_strongest() {
        let a = RumbleCue { low: 0.9, high: 0.2, ms: 120 };
        let b = RumbleCue { low: 0.3, high: 0.7, ms: 300 };
        assert_eq!(merge(&[a, b]), Some(RumbleCue { low: 0.9, high: 0.7, ms: 300 }));
        assert_eq!(merge(&[]), None);
    }

    #[test]
    fn a_weak_cue_waits_for_a_strong_one_to_finish() {
        let mut r = Rumble::default();
        r.play(None, &[RumbleCue { low: 1.0, high: 0.8, ms: 400 }]);
        r.play(None, &[RumbleCue { low: 0.2, high: 0.2, ms: 50 }]);
        assert_eq!(r.current.map(|(c, _)| c.low), Some(1.0));
        r.stop();
        assert!(r.current.is_none());
    }
}

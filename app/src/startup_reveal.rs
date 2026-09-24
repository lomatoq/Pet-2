//! Short launch presentation, independent of the once-per-life birth sequence.
use std::time::Instant;

pub struct StartupReveal { started: Option<Instant>, enabled: bool }
#[derive(Clone, Copy)]
pub struct Frame { pub active: bool, pub den_alpha: f32, pub den_drop: f32, pub pet_alpha: f32, pub pet_scale: f32, pub orb_drop: f32, pub orb_visible: bool, pub release: f32 }
fn smooth(t: f32) -> f32 { let x = t.clamp(0.0, 1.0); x*x*x*(x*(x*6.0-15.0)+10.0) }
impl Frame {
    pub fn at(t: f32) -> Self {
        let den = smooth(t / 0.85);
        let p = ((t-0.48)/0.82).clamp(0.0,1.0);
        let scale = if p >= 1.0 { 1.0 } else { 1.0 + 1.65*(p-1.0).powi(3) + 0.65*(p-1.0).powi(2) };
        let fall = ((t-0.35)/0.65).clamp(0.0,1.0);
        let bounce = ((t-1.0)/0.38).clamp(0.0,1.0);
        Self { active: t < 2.0, den_alpha: den, den_drop: 1.0-den,
            pet_alpha: smooth((t-0.48)/0.32), pet_scale: scale.max(0.001),
            orb_drop: -190.0*(1.0-fall*fall)-12.0*(std::f32::consts::PI*bounce).sin().powi(2),
            orb_visible: t >= 0.35, release: smooth((t-1.4)/0.6) }
    }
}
impl StartupReveal {
    pub fn new(enabled: bool) -> Self { Self { started: None, enabled } }
    pub fn frame(&mut self, ready: bool, birth: bool) -> Frame {
        if birth { self.enabled = false; }
        if !self.enabled { return Frame::at(2.0); }
        let t = if ready { self.started.get_or_insert_with(Instant::now).elapsed().as_secs_f32() } else { 0.0 };
        let frame = Frame::at(t);
        if !frame.active { self.enabled = false; }
        frame
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn startup_reveal_is_bounded_and_finishes_without_a_jump() {
        assert_eq!(Frame::at(0.0).den_alpha, 0.0);
        assert_eq!(Frame::at(0.0).pet_alpha, 0.0);
        assert!(!Frame::at(0.0).orb_visible);
        let mut overshoot = false;
        for i in 0..=240 { let f=Frame::at(i as f32/120.0); assert!((0.001..=1.04).contains(&f.pet_scale)); overshoot |= f.pet_scale > 1.01; assert!((-202.0..=0.001).contains(&f.orb_drop)); }
        assert!(overshoot);
        let end=Frame::at(2.0); assert!(!end.active); assert_eq!(end.den_drop,0.0); assert_eq!(end.pet_scale,1.0); assert_eq!(end.release,1.0); assert!(end.orb_drop.abs()<0.001);
    }
    #[test] fn startup_waits_for_ready_and_never_replaces_birth() {
        let mut s=StartupReveal::new(true); assert_eq!(s.frame(false,false).pet_alpha,0.0); assert!(s.started.is_none()); assert!(!s.frame(true,true).active); assert!(!s.frame(true,false).active);
    }
}

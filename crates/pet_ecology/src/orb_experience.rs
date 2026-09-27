//! Bounded experience selects one motor tactic per bout; observations update
//! prediction only across uninterrupted free flight, never across a pickup.
use glam::Vec2;
use serde::{Deserialize, Serialize};
use crate::{EcologyBehaviorFrame, EcologyState, ObjectKind, ObjectLifecycle, ObjectMemory};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrbExperience {
    pub value: [f32; 24],
    pub evidence: [f32; 24],
    pub recent: [f32; 24],
    /// Signed displacement correction in desktop-height units at a 150ms horizon.
    pub prediction_bias: Vec2,
}
impl OrbExperience {
    pub fn predict(&self, position: Vec2, velocity: Vec2, aspect: f32, lead: f32) -> Vec2 {
        let scale=Vec2::new(aspect.clamp(0.25,8.0),1.0);
        (position+(velocity*lead+self.prediction_bias*(lead/0.15))/scale)
            .clamp(Vec2::splat(0.025),Vec2::splat(0.975))
    }
    pub fn valid(&self) -> bool {
        self.value.iter().all(|x| x.is_finite() && (-1.0..=1.0).contains(x))
            && self.evidence.iter().all(|x| x.is_finite() && (0.0..=32.0).contains(x))
            && self.recent.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            && self.prediction_bias.is_finite() && self.prediction_bias.length() <= 0.03001
    }
    pub fn observe_contact(&mut self, variant: u8, success: bool) {
        let i=usize::from(variant.min(23));
        self.value[i] += ((if success {1.0} else {-0.4})-self.value[i])*0.18;
        self.evidence[i]=(self.evidence[i]+1.0).min(32.0);
    }
    pub fn choose(&mut self, frame: EcologyBehaviorFrame, orb: Vec2, velocity: Vec2, seed: u64) -> u8 {
        let energy=(frame.play_drive*(1.0-frame.social_contact.fatigue)).clamp(0.0,1.0);
        let moving=(velocity.length()/0.35).clamp(0.0,1.0);
        let wall=(orb.x.min(1.0-orb.x)*frame.desktop_aspect/0.12).clamp(0.0,1.0);
        let index=(0..24).max_by(|&a,&b| {
            let score=|i:usize| {
                let approach=i/4; let tactic=i%4;
                let fit=match tactic {0=>energy*0.32,1=>0.12+wall*0.12,2=>energy*0.3, _=>(1.0-energy)*0.38};
                let route=match approach {4=>moving*0.4,1|2=>wall*0.12,5=>(1.0-energy)*0.1,_=>0.08};
                fit+route+self.value[i]*0.23+0.12/(1.0+self.evidence[i]).sqrt()
                    -self.recent[i]*0.35
                    +(((seed.wrapping_add(i as u64*7919)%997)as f32)/997.0)*0.03
            };
            score(a).total_cmp(&score(b))
        }).unwrap_or(0);
        for value in &mut self.recent {*value*=0.72;}
        self.recent[index]=1.0;
        index as u8
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct OrbPredictionObserver { sample: Option<(u64,Vec2,Vec2,f32)> }
impl OrbPredictionObserver {
    pub fn observe(&mut self, state:&mut EcologyState, aspect:f32, dt:f32, timestamp:f64) {
        let Some(orb)=state.objects.iter().find(|o|o.kind==ObjectKind::Orb && o.lifecycle==ObjectLifecycle::Free) else {self.sample=None;return;};
        let scale=Vec2::new(aspect.clamp(0.25,8.0),1.0);
        let (id,point,velocity)=(orb.id,orb.position*scale,orb.velocity);
        if let Some((old_id,origin,initial_velocity,age))=&mut self.sample {
            *age+=dt;
            if id!=*old_id || point.distance(*origin)>0.3 || velocity.distance(*initial_velocity)>0.45 {
                self.sample=None;
            } else if *age>=0.15 {
                let error=point-(*origin+*initial_velocity * *age);
                let calibrated=(error* (0.15 / *age)).clamp_length_max(0.03);
                state.orb_experience.prediction_bias=state.orb_experience.prediction_bias.lerp(calibrated,0.12).clamp_length_max(0.03);
                let normalized=(error.length()/0.06).clamp(0.0,1.0);
                if let Some(m)=state.object_memories.iter_mut().find(|m|m.object_id==id) {
                    m.prediction_error_ema+=(normalized-m.prediction_error_ema)*0.12;
                    m.last_seen_seconds=timestamp.max(0.0);
                } else if state.object_memories.len()<crate::MAX_OBJECT_MEMORIES {
                    state.object_memories.push(ObjectMemory{object_id:id,interaction_count:0,positive_outcomes:0,negative_outcomes:0,prediction_error_ema:normalized,last_seen_seconds:timestamp.max(0.0)});
                }
                self.sample=None;
            }
        }
        if self.sample.is_none() {self.sample=Some((id,point,velocity,0.0));}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prediction_uses_actual_free_flight_and_resets_on_user_pickup() {
        let mut s=EcologyState::default();let mut observer=OrbPredictionObserver::default();
        s.objects[0].lifecycle=ObjectLifecycle::Free;s.objects[0].velocity=Vec2::new(0.2,0.0);
        for i in 0..20 {s.objects[0].position.x+=0.0025;observer.observe(&mut s,2.0,0.05,i as f64*0.05);}
        assert!(s.orb_experience.prediction_bias.x<0.0);
        let p=Vec2::splat(0.5);
        let actual=p+Vec2::new(0.1*0.15/2.0,0.0);
        let naive=OrbExperience::default().predict(p,Vec2::new(0.2,0.0),2.0,0.15);
        let learned=s.orb_experience.predict(p,Vec2::new(0.2,0.0),2.0,0.15);
        assert!(learned.distance(actual)<naive.distance(actual)*0.6);
        let before=s.orb_experience.prediction_bias;
        s.objects[0].lifecycle=ObjectLifecycle::GrabbedByUser;
        s.objects[0].position=Vec2::ZERO;observer.observe(&mut s,2.0,0.05,2.0);
        assert_eq!(before,s.orb_experience.prediction_bias);
        assert!(s.validate().is_ok());
    }
    #[test]
    fn learned_contact_evidence_is_bounded_reversible_and_persistent() {
        let mut x=OrbExperience::default();
        for _ in 0..100 {x.observe_contact(5,true);}
        for _ in 0..30 {x.observe_contact(5,false);}
        assert!(x.value[5]<0.0 && x.valid());
        let r:OrbExperience=serde_json::from_str(&serde_json::to_string(&x).unwrap()).unwrap();
        assert_eq!(x,r);
    }
}

//! Conserved food transit and persistent, jointed soft waste. All lengths use
//! desktop-height units; randomness is a stable individual phase, not a timer
//! that invents bodily events without food or gas.
use crate::MorselProfile;
use glam::Vec2;
use serde::{Deserialize, Serialize};

pub const MAX_WASTE_NODES: usize = 2048;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DigestiveTract {
    pub esophagus: f32,
    pub stomach: f32,
    pub intestine: f32,
    pub bowel: f32,
    pub hydration: f32,
    pub fiber: f32,
    pub fermentation: f32,
    pub swallowed_air: f32,
    pub gas: f32,
    pub phase: f32,
    pub ingested: f64,
    pub absorbed: f64,
    pub expelled: f64,
    pub burp: f32,
    pub fart: f32,
    pub strain: f32,
    pub effort: f32,
    pub bowel_age: f32,
}
impl DigestiveTract {
    pub fn swallow(&mut self, food: &MorselProfile, amount: f32, hunger: f32) {
        if !food.is_valid() || !amount.is_finite() || amount <= 0.0 {
            return;
        }
        let amount = amount.min(1.0);
        let total = self.esophagus + self.stomach + self.intestine + self.bowel;
        let mix = amount / (total + amount).max(0.001);
        self.hydration +=
            ((0.25 + food.warmth * 0.5 + (1.0 - food.cohesion_bias) * 0.2) - self.hydration) * mix;
        self.fiber += (food.cohesion_bias - self.fiber) * mix;
        self.fermentation +=
            (food.stimulation * 0.65 + food.novelty * 0.35 - self.fermentation) * mix;
        self.esophagus += amount;
        self.ingested += f64::from(amount);
        self.swallowed_air +=
            amount * (0.04 + hunger.clamp(0.0, 1.0) * 0.13 + self.stomach * 0.025);
    }
    pub fn advance(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.25);
        self.phase =
            (self.phase + dt * (0.7 + self.hydration * 0.6)).rem_euclid(std::f32::consts::TAU);
        let swallowed = self.esophagus * (1.0 - (-dt * 1.8).exp());
        self.esophagus -= swallowed;
        self.stomach += swallowed;
        let digested = (self.stomach * dt / (32.0 + self.fiber * 55.0)).min(self.stomach);
        self.stomach -= digested;
        self.intestine += digested;
        let transit = (self.intestine * dt / (48.0 + self.fiber * 65.0)).min(self.intestine);
        self.intestine -= transit;
        let residue = transit * (0.28 + self.fiber * 0.28);
        self.bowel += residue;
        self.absorbed += f64::from(transit - residue);
        self.gas += transit * (0.18 + self.fermentation * 0.5);
        self.bowel_age = if self.bowel > 0.015 {
            self.bowel_age + dt
        } else {
            0.0
        };
        self.burp = (self.burp - dt * 1.8).max(0.0);
        self.fart = (self.fart - dt * 1.2).max(0.0);
        // Air has its own pressure relief, distinct from slower intestinal gas.
        if self.burp == 0.0 && self.swallowed_air > 0.13 + 0.025 * self.phase.sin() {
            self.burp = (self.swallowed_air * 3.0).clamp(0.25, 1.0);
            self.swallowed_air *= 0.25;
        }
        if self.fart == 0.0 && self.gas > 0.075 + 0.02 * self.phase.cos() {
            self.fart = (self.gas * 5.0).clamp(0.25, 1.0);
            self.gas *= 0.22;
        }
    }
    pub fn hardness(&self) -> f32 {
        (1.0 - self.hydration + self.fiber * 0.25 + self.bowel_age / 3600.0).clamp(0.05, 0.95)
    }
    pub fn needs_to_go(&self) -> bool {
        self.bowel > 0.12 || (self.bowel > 0.025 && self.bowel_age > 180.0)
    }
    pub fn valid(&self) -> bool {
        [
            self.esophagus,
            self.stomach,
            self.intestine,
            self.bowel,
            self.hydration,
            self.fiber,
            self.fermentation,
            self.swallowed_air,
            self.gas,
            self.phase,
            self.burp,
            self.fart,
            self.strain,
            self.effort,
            self.bowel_age,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0)
            && [self.ingested, self.absorbed, self.expelled]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WasteNode {
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WasteChain {
    pub nodes: Vec<WasteNode>,
    pub hardness: f32,
    pub rest_length: f32,
    #[serde(default)]
    pub rest_turn: f32,
    /// Per-joint rest angles. Empty only in saves predating compact soft traces.
    #[serde(default)]
    pub rest_turns: Vec<f32>,
    pub remaining: f32,
    pub next_segment: f32,
    pub mass: f32,
    pub floor: f32,
    #[serde(default)]
    pub resting_seconds: f32,
    #[serde(skip)]
    pub suction: Option<f32>,
}
impl WasteChain {
    fn distance_to(&self, cursor: Vec2, scale: Vec2) -> f32 {
        let p = cursor * scale;
        let mut distance = self.nodes.iter().map(|n|
            p.distance(n.position * scale) - n.radius).fold(f32::INFINITY, f32::min);
        for pair in self.nodes.windows(2) {
            let a = pair[0].position * scale;
            let d = (pair[1].position - pair[0].position) * scale;
            let t = ((p-a).dot(d) / d.length_squared().max(1e-9)).clamp(0.0,1.0);
            distance = distance.min(p.distance(a+d*t) - pair[0].radius.max(pair[1].radius));
        }
        distance
    }

    fn migrate_compact_shape(&mut self, scale: Vec2) {
        if !self.rest_turns.is_empty() { return; }
        let count = (self.nodes.len() + self.remaining as usize).max(3);
        self.rest_turns = rest_profile(count, self.mass * 37.0 + self.rest_turn * 11.0);
        // Preserve the trace, mass and location. Old oversized arches shrink
        // once, then settle under the same compliant constraints as new ones.
        let center = self.nodes.iter().map(|n|n.position).sum::<Vec2>() / self.nodes.len() as f32;
        let anchor = Vec2::new(center.x, self.floor);
        self.rest_length *= 0.56;
        for n in &mut self.nodes {
            n.position = anchor + (n.position-anchor)*0.56;
            n.radius *= 0.70;
            n.velocity = Vec2::ZERO;
            n.position.x = n.position.x.clamp(n.radius/scale.x,1.0-n.radius/scale.x);
        }
        self.resting_seconds = 0.0;
    }
}

// Differenced headings keep total curvature bounded: no repeated equal-angle
// joints accumulating into a semicircle. Two incommensurate wavelengths produce
// gentle, asymmetric local bends, fixed for the lifetime of each deposited piece.
fn rest_profile(count: usize, phase: f32) -> Vec<f32> {
    let heading = |i: usize| {
        let x=i as f32;
        0.42*(x*0.83+phase).sin()+0.19*(x*1.71+phase*1.37).sin()
    };
    (0..count.saturating_sub(2).max(1)).map(|i|heading(i+1)-heading(i)).collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct DigestiveBubble {
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
    pub life: f32,
    pub burp: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WasteWorld {
    pub chains: Vec<WasteChain>,
    #[serde(skip)]
    pub bubbles: Vec<DigestiveBubble>,
    #[serde(skip)]
    pub burst_age: f32,
    #[serde(skip)]
    pub last_burp: f32,
    #[serde(skip)]
    pub last_fart: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct DigestionFrame {
    pub outlet: Vec2,
    pub mouth: Vec2,
    pub gas_outlet: Vec2,
    pub body_velocity: Vec2,
    pub floor: f32,
    pub settled: bool,
    pub aspect: f32,
    pub cursor: Option<Vec2>,
}
impl Default for DigestionFrame {
    fn default() -> Self {
        Self {
            outlet: Vec2::splat(0.5),
            mouth: Vec2::splat(0.5),
            gas_outlet: Vec2::splat(0.5),
            body_velocity: Vec2::ZERO,
            floor: 1.0,
            settled: false,
            aspect: 1.0,
            cursor: None,
        }
    }
}
impl WasteWorld {
    pub fn active(&self) -> bool {
        self.chains.last().is_some_and(|c| c.remaining > 0.0)
    }
    pub fn valid(&self) -> bool {
        self.chains.iter().map(|c| c.nodes.len()).sum::<usize>() <= MAX_WASTE_NODES
            && self.chains.iter().all(|c| {
                !c.nodes.is_empty()
                    && c.nodes.len() <= 24
                    && c.hardness.is_finite()
                    && (0.0..=1.0).contains(&c.hardness)
                    && c.rest_turn.is_finite()
                    && c.rest_turns.len() <= 24
                    && c.rest_turns.iter().all(|v|v.is_finite() && v.abs() <= std::f32::consts::PI)
                    && c.resting_seconds.is_finite()
                    && c.rest_length.is_finite()
                    && c.rest_length > 0.0
                    && c.remaining.is_finite()
                    && c.remaining >= 0.0
                    && c.remaining <= 24.0
                    && c.floor.is_finite()
                    && c.floor > 0.05
                    && c.floor <= 1.0
                    && c.mass.is_finite()
                    && c.mass >= 0.0
                    && c.next_segment.is_finite()
                    && c.nodes.iter().all(|n| {
                        n.position.is_finite()
                            && n.velocity.is_finite()
                            && n.radius.is_finite()
                            && n.radius > 0.0
                            && n.radius < 0.05
                    })
            })
    }
    /// Hit geometry matches the rendered continuous centreline, including gaps
    /// between joints. Cleanup remains a separate, explicit interaction.
    pub fn hit_test(&self, cursor: Vec2, aspect: f32, margin: f32) -> bool {
        let scale=Vec2::new(aspect.clamp(0.25,8.0),1.0);
        cursor.is_finite() && self.chains.iter().any(|c|
            c.remaining == 0.0 && c.suction.is_none() && c.distance_to(cursor,scale) <= margin)
    }

    pub fn poke(&mut self, cursor: Vec2, aspect: f32) -> bool {
        if !cursor.is_finite() {return false;}
        let scale=Vec2::new(aspect.clamp(0.25,8.0),1.0);
        let selected=self.chains.iter().enumerate()
            .filter(|(_,c)|c.remaining==0.0 && c.suction.is_none())
            .map(|(i,c)|(i,c.distance_to(cursor,scale)))
            .filter(|(_,distance)|*distance<0.007)
            .min_by(|a,b|a.1.total_cmp(&b.1));
        let Some((index,_))=selected else {return false;};
        let chain=&mut self.chains[index];
        chain.migrate_compact_shape(scale);
        chain.resting_seconds=0.0;
        let center=chain.nodes.iter().map(|n|n.position).sum::<Vec2>()/chain.nodes.len() as f32;
        let side=((center.x-cursor.x)*scale.x/0.012).clamp(-1.0,1.0);
        for n in &mut chain.nodes {
            let d=((n.position-cursor)*scale).length();
            let weight=(-d*d/(2.0*0.013_f32.powi(2))).exp();
            let impulse=Vec2::new(side*0.12,-0.12-0.07*chain.hardness)*weight;
            n.velocity=(n.velocity+impulse).clamp_length_max(0.4);
        }
        true
    }

    pub fn step(&mut self, gut: &mut DigestiveTract, f: DigestionFrame, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(1.0 / 30.0);
        let aspect = f.aspect.clamp(0.25, 8.0);
        let scale = Vec2::new(aspect, 1.0);
        let hard = gut.hardness();
        let effort_target = if f.settled && (gut.needs_to_go() || self.active()) {
            (0.2 + hard * 0.6 + gut.bowel * 0.2).min(1.0) * (0.55 + 0.45 * gut.phase.sin().max(0.0))
        } else {
            0.0
        };
        gut.strain += (effort_target - gut.strain) * (1.0 - (-dt * 5.0).exp());
        gut.effort = if f.settled && gut.needs_to_go() {
            gut.effort + gut.strain * dt
        } else {
            (gut.effort - dt).max(0.0)
        };
        let nodes = self.chains.iter().map(|c| c.nodes.len()).sum::<usize>();
        if f.settled
            && gut.needs_to_go()
            && !self.active()
            && gut.effort > 0.45 + hard * 1.6
            && nodes + 24 <= MAX_WASTE_NODES
        {
            let amount = gut
                .bowel
                .min(0.14 + gut.hydration * 0.3 + gut.phase.sin().abs() * 0.12);
            gut.bowel -= amount;
            gut.expelled += f64::from(amount);
            gut.effort = 0.0;
            let radius = (2.8 + amount.sqrt() * 2.4 + (1.0 - hard) * 0.8) / 1152.0;
            let segments = (amount * 23.0).ceil().clamp(3.0, 10.0);
            self.chains.push(WasteChain {
                nodes: vec![WasteNode {
                    position: f.outlet,
                    velocity: f.body_velocity * scale + Vec2::new(gut.phase.sin() * 0.02, 0.015),
                    radius,
                }],
                hardness: hard,
                rest_length: radius * 1.35,
                rest_turn: 0.0,
                rest_turns: rest_profile(segments as usize, gut.phase + gut.expelled as f32 * 19.7),
                remaining: segments - 1.0,
                next_segment: 0.0,
                mass: amount,
                floor: f.floor,
                resting_seconds: 0.0,
                suction: None,
            });
        }
        for (is_burp, amplitude, previous, origin) in [
            (true, gut.burp, self.last_burp, f.mouth),
            (
                false,
                gut.fart,
                self.last_fart,
                f.gas_outlet,
            ),
        ] {
            if amplitude > previous + 0.05 {
                let count = (2.0 + amplitude * 6.0).ceil() as usize;
                for i in 0..count {
                    if self.bubbles.len() >= 48 {
                        break;
                    }
                    let phase = gut.phase + i as f32 * 2.4;
                    self.bubbles.push(DigestiveBubble {
                        position: origin + Vec2::new(i as f32*0.010/aspect, -(i as f32)*0.008),
                        velocity: Vec2::new(0.035 + phase.sin() * 0.022, -0.028 - amplitude * 0.045 - i as f32 * 0.009),
                        radius: (2.0 + amplitude * 6.0 * (0.3 + phase.cos().abs())) / 1152.0,
                        life: 1.6 + amplitude + i as f32 * 0.07,
                        burp: is_burp,
                    });
                }
            }
        }
        self.last_burp = gut.burp;
        self.last_fart = gut.fart;
        for b in &mut self.bubbles {
            b.life -= dt;
            b.velocity.y -= dt * 0.015;
            b.position += b.velocity / scale * dt;
        }
        // Soft separation keeps individual transparent rims from stacking.
        for i in 0..self.bubbles.len() {
            for j in i+1..self.bubbles.len() {
                let d=(self.bubbles[j].position-self.bubbles[i].position)*scale;
                let limit=(self.bubbles[i].radius+self.bubbles[j].radius)*1.2;
                let length=d.length();
                if length<limit {
                    let push=d.normalize_or(Vec2::X)*(limit-length)*0.5;
                    self.bubbles[i].position-=push/scale;
                    self.bubbles[j].position+=push/scale;
                }
            }
        }
        self.bubbles.retain(|b| b.life > 0.0);
        for chain in &mut self.chains {
            chain.migrate_compact_shape(scale);
            if !f.settled && chain.remaining > 0.0 {
                let returned =
                    chain.mass * chain.remaining / (chain.nodes.len() as f32 + chain.remaining);
                gut.bowel += returned;
                gut.expelled -= f64::from(returned);
                chain.mass -= returned;
                chain.remaining = 0.0;
            }
            if let Some(cursor) = f.cursor
                && chain.remaining == 0.0
                && chain.suction.is_none()
            {
                let hit = chain.distance_to(cursor, scale) < 0.009;
                if hit {
                    chain.suction = Some(0.0);
                }
            }
            if let Some(t) = &mut chain.suction {
                if let Some(cursor) = f.cursor {
                    *t = (*t + dt / 0.42).min(1.0);
                    for n in &mut chain.nodes {
                        let d = (cursor - n.position) * scale;
                        let swirl = Vec2::new(-d.y, d.x) * 4.0 * (1.0 - *t);
                        n.velocity += (d * 95.0 + swirl - n.velocity * 13.0) * dt;
                        n.position += n.velocity / scale * dt;
                    }
                } else {
                    chain.suction = None;
                    chain.resting_seconds = 0.0;
                }
                continue;
            }
            if chain.remaining > 0.0 && f.settled {
                chain.next_segment +=
                    dt * (3.0 + (1.0 - chain.hardness) * 5.0) * (0.4 + gut.strain);
                if chain.next_segment >= 1.0 {
                    chain.next_segment -= 1.0;
                    chain.remaining -= 1.0;
                    let radius = chain.nodes[0].radius;
                    chain.nodes.push(WasteNode {
                        position: f.outlet,
                        velocity: f.body_velocity * scale
                            + Vec2::new(gut.phase.sin() * 0.02, 0.015),
                        radius,
                    });
                }
            }
            if chain.remaining == 0.0 && chain.resting_seconds > 1.0 {
                continue;
            }
            let old: Vec<_> = chain.nodes.iter().map(|n| n.position * scale).collect();
            for n in &mut chain.nodes {
                n.velocity.y += 0.38 * dt;
                n.velocity *= (-dt * 1.6).exp();
                n.position += n.velocity / scale * dt;
            }
            let impact: Vec<_> = chain.nodes.iter().map(|n|n.velocity.y).collect();
            let mut bend_lambda=vec![0.0;chain.nodes.len()];
            for _ in 0..6 {
                for i in 1..chain.nodes.len() {
                    let d = (chain.nodes[i].position - chain.nodes[i - 1].position) * scale;
                    let length = d.length();
                    let correction = d / length.max(1e-6)
                        * (length - chain.rest_length)
                        * 0.5
                        * (0.6 + chain.hardness * 0.4);
                    chain.nodes[i - 1].position += correction / scale;
                    chain.nodes[i].position -= correction / scale;
                }
                // Separate non-neighbour joints so a long extrusion coils on
                // the floor rather than collapsing every segment into one dot.
                for i in 0..chain.nodes.len() {
                    for j in i + 2..chain.nodes.len() {
                        let d = (chain.nodes[j].position - chain.nodes[i].position) * scale;
                        let min = (chain.nodes[i].radius + chain.nodes[j].radius) * 0.82;
                        let length = d.length();
                        if length < min {
                            let axis = if length > 1e-6 { d / length } else { Vec2::X };
                            let correction = axis * (min - length) * 0.4;
                            chain.nodes[i].position -= correction / scale;
                            chain.nodes[j].position += correction / scale;
                        }
                    }
                }
                // Firm material retains its deposited curl; wet material yields.
                for (i, accumulated) in bend_lambda.iter_mut().enumerate().skip(2) {
                    let a=chain.nodes[i-2].position*scale;
                    let b=chain.nodes[i-1].position*scale;
                    let c=chain.nodes[i].position*scale;
                    let first=b-a; let second=c-b;
                    let perp=|v:Vec2|Vec2::new(-v.y,v.x);
                    let ga=perp(first)/first.length_squared().max(1e-7);
                    let gc=perp(second)/second.length_squared().max(1e-7);
                    let gb=-ga-gc;
                    let angle=first.perp_dot(second).atan2(first.dot(second));
                    let rest=chain.rest_turns.get(i-2).copied().unwrap_or(0.0);
                    let error=(angle-rest+std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
                    // XPBD compliance keeps bend elasticity independent of the
                    // number of solver iterations. Even firm material can flex.
                    let alpha=(0.35 + (1.0-chain.hardness).powi(3)*90.0)/(dt*dt);
                    let lambda=(-error-alpha * *accumulated)
                        /(ga.length_squared()+gb.length_squared()+gc.length_squared()+alpha).max(1e-6);
                    *accumulated += lambda;
                    // Equal-mass angular projection conserves the centre of
                    // mass: the curl cannot propel itself across the desktop.
                    chain.nodes[i-2].position+=ga*lambda/scale;
                    chain.nodes[i-1].position+=gb*lambda/scale;
                    chain.nodes[i].position+=gc*lambda/scale;
                }
                if chain.remaining > 0.0 && f.settled {
                    chain.nodes.last_mut().unwrap().position = f.outlet;
                }
                for n in &mut chain.nodes {
                    n.position.x = n
                        .position
                        .x
                        .clamp(n.radius / aspect, 1.0 - n.radius / aspect);
                    n.position.y = n.position.y.clamp(
                        n.radius,
                        chain.floor - n.radius * (0.65 + chain.hardness * 0.35),
                    );
                }
            }
            for ((n, old), incoming) in chain.nodes.iter_mut().zip(old).zip(impact) {
                n.velocity = ((n.position * scale - old) / dt).clamp_length_max(0.6);
                if n.position.y >= chain.floor - n.radius * 1.01 {
                    n.velocity.x *= (-dt * (8.0 + (1.0-chain.hardness)*12.0)).exp();
                    if incoming > 0.025 {
                        n.velocity.y = -incoming * (0.10 + chain.hardness*0.28);
                    }
                }
            }
            chain.resting_seconds = if chain.remaining == 0.0
                && chain.nodes.iter().all(|n| n.velocity.length() < 0.008)
            {
                chain.resting_seconds + dt
            } else {
                0.0
            };
        }
        // No age expiry. Removal occurs only after an explicitly selected hover
        // has completed the suction animation in cleanup mode.
        self.chains.retain(|c| c.suction.is_none_or(|t| t < 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn food() -> MorselProfile {
        MorselProfile {
            hue: 0.1,
            saturation: 0.6,
            value: 0.8,
            warmth: 0.6,
            pulse_rate: 0.5,
            stimulation: 0.7,
            cohesion_bias: 0.55,
            novelty: 0.4,
        }
    }
    fn compact_fixture(hardness: f32) -> WasteWorld {
        let radius=0.0035;
        WasteWorld { chains: vec![WasteChain {
            nodes:(0..8).map(|i|WasteNode {
                position:Vec2::new(0.45+i as f32*radius*1.35,0.9-radius),
                velocity:Vec2::ZERO,radius,
            }).collect(),hardness,rest_length:radius*1.35,rest_turn:0.0,
            rest_turns:rest_profile(8,2.3),remaining:0.0,next_segment:0.0,
            mass:0.15,floor:0.9,resting_seconds:0.0,suction:None,
        }], ..Default::default() }
    }

    #[test]
    fn click_wakes_only_hit_trace_and_deforms_then_settles() {
        for hz in [60,120] {
            let mut world=compact_fixture(0.72);
            let mut distant=world.chains[0].clone();
            for n in &mut distant.nodes {n.position.x+=0.3;}
            world.chains.push(distant);
            let mut gut=DigestiveTract::default();
            for _ in 0..hz*8 {world.step(&mut gut,DigestionFrame::default(),1.0/hz as f32);}
            assert!(world.chains[0].resting_seconds>1.0);
            let other=world.chains[1].clone();
            let before=world.chains[0].clone();
            let cursor=before.nodes[0].position;
            assert!(world.hit_test(cursor,1.0,0.0));
            assert!(!world.poke(Vec2::new(0.2,0.1),1.0));
            assert!(world.poke(cursor,1.0));
            assert_eq!(world.chains[0].resting_seconds,0.0);
            let first=world.chains[0].nodes[0].velocity;
            let last=world.chains[0].nodes[7].velocity;
            assert!(first.distance(last)>0.08,"impulse must be local, not a rigid translation");
            let initial_chord=before.nodes[0].position.distance(before.nodes[7].position);
            let mut deformation=0.0_f32;
            let mut lift=0.0_f32;
            for _ in 0..hz*10 {
                world.step(&mut gut,DigestionFrame::default(),1.0/hz as f32);
                let c=&world.chains[0];
                deformation=deformation.max((c.nodes[0].position.distance(c.nodes[7].position)-initial_chord).abs());
                lift=lift.max(before.nodes[0].position.y-c.nodes[0].position.y);
                assert!(c.nodes.iter().all(|n|n.position.y<=c.floor));
            }
            assert!(lift>0.002 && deformation>0.0005,"lift {lift}, bend {deformation}");
            assert!(world.chains[0].resting_seconds>1.0,"poke must damp down");
            assert_eq!(world.chains[1],other,"unhit trace stays asleep");
            assert_eq!(world.chains.len(),2,"ordinary clicks never clean up");
        }
    }

    #[test]
    fn new_deposits_are_compact_and_curvature_is_local_not_one_arch() {
        let mut profiles=Vec::new();
        for phase in [0.3,1.7,3.6,5.2] {
            let mut gut=DigestiveTract {bowel:0.4,hydration:0.3,fiber:0.5,phase,effort:10.0,..Default::default()};
            let mut world=WasteWorld::default();
            let frame=DigestionFrame {outlet:Vec2::new(0.5,0.894),floor:0.9,settled:true,..Default::default()};
            world.step(&mut gut,frame,1.0/120.0);
            let c=&world.chains[0];
            let length=(c.remaining+c.nodes.len() as f32-1.0)*c.rest_length;
            assert!(length<0.052 && c.nodes[0].radius<0.005);
            assert!(c.rest_turns.iter().any(|a|*a>0.03) && c.rest_turns.iter().any(|a|*a< -0.03));
            profiles.push(c.rest_turns.clone());
        }
        assert!(profiles.windows(2).all(|p|p[0]!=p[1]));
    }

    #[test]
    fn old_arch_save_migrates_once_without_losing_mass_or_traces() {
        let mut world=compact_fixture(0.8);
        world.chains[0].rest_turns.clear();
        world.chains[0].rest_turn=0.4;
        world.chains[0].resting_seconds=30.0;
        let before=world.chains[0].clone();
        let encoded=serde_json::to_value(&world).unwrap();
        let mut legacy=encoded;
        legacy["chains"][0].as_object_mut().unwrap().remove("rest_turns");
        let mut loaded:WasteWorld=serde_json::from_value(legacy).unwrap();
        loaded.step(&mut DigestiveTract::default(),DigestionFrame::default(),1.0/120.0);
        assert_eq!(loaded.chains.len(),1);
        assert_eq!(loaded.chains[0].mass,before.mass);
        assert!(loaded.chains[0].rest_length < before.rest_length*0.6);
        assert!(!loaded.chains[0].rest_turns.is_empty());
        let length=loaded.chains[0].rest_length;
        loaded.step(&mut DigestiveTract::default(),DigestionFrame::default(),1.0/120.0);
        assert_eq!(loaded.chains[0].rest_length,length);
        assert!(loaded.valid());
    }

    #[test]
    fn firm_curl_retains_volume_without_self_propulsion_and_wet_trace_yields() {
        let mut heights=Vec::new();
        for hardness in [0.08,0.95] {
            let mut gut=DigestiveTract::default();
            let radius=0.003;
            let chain=WasteChain {
                nodes:(0..16).map(|i| { let angle=i as f32*std::f32::consts::PI/15.0;
                    WasteNode { position:Vec2::new(0.5+0.04*angle.cos(),0.9-radius-0.04*angle.sin()),velocity:Vec2::ZERO,radius }
                }).collect(), hardness,rest_length:0.08*(std::f32::consts::PI/30.0).sin(),
                rest_turn:0.0,rest_turns:vec![-std::f32::consts::PI/15.0;14],remaining:0.0,next_segment:0.0,mass:0.2,floor:0.9,resting_seconds:0.0,suction:None,
            };
            let mut world=WasteWorld {chains:vec![chain],..Default::default()};
            for _ in 0..2400 {world.step(&mut gut,DigestionFrame::default(),1.0/120.0);}
            let nodes=&world.chains[0].nodes;
            let center=nodes.iter().map(|n|n.position.x).sum::<f32>()/nodes.len() as f32;
            assert!((center-0.5).abs()<0.03,"curl must not propel itself: {center}");
            heights.push(0.9-nodes.iter().map(|n|n.position.y).fold(1.0_f32,f32::min));
        }
        assert!(heights[1]>heights[0]+0.008,"firm vs wet: {heights:?}");
    }

    #[test]
    fn gas_bubbles_use_side_outlet_and_do_not_share_one_position() {
        let mut gut=DigestiveTract {fart:0.9,..Default::default()};
        let mut world=WasteWorld::default();
        let frame=DigestionFrame {gas_outlet:Vec2::new(0.7,0.6),..Default::default()};
        world.step(&mut gut,frame,1.0/120.0);
        assert!(world.bubbles.len()>3);
        let first=world.bubbles[0].radius;
        assert!(world.bubbles.iter().any(|b|(b.radius-first).abs()>0.001));
        for i in 0..world.bubbles.len() {
            let a=&world.bubbles[i];assert!(a.position.x>0.68 && a.velocity.y<0.0);
            for b in &world.bubbles[i+1..] {
                assert!(a.position.distance(b.position)>(a.radius+b.radius)*0.85);
            }
        }
    }

    #[test]
    fn empty_gut_never_invents_waste_and_food_mass_is_conserved() {
        let mut gut = DigestiveTract::default();
        let mut world = WasteWorld::default();
        let frame = DigestionFrame {
            outlet: Vec2::new(0.4, 0.95),
            floor: 0.96,
            settled: true,
            ..Default::default()
        };
        for _ in 0..600 {
            gut.advance(0.1);
            world.step(&mut gut, frame, 1.0 / 30.0);
        }
        assert!(world.chains.is_empty() && world.bubbles.is_empty());
        for _ in 0..4 {
            gut.swallow(&food(), 0.6, 0.9);
        }
        for _ in 0..36000 {
            gut.advance(1.0 / 30.0);
            world.step(&mut gut, frame, 1.0 / 30.0);
        }
        assert!(!world.chains.is_empty());
        assert!(world.valid());
        let mass = f64::from(gut.esophagus + gut.stomach + gut.intestine + gut.bowel)
            + gut.absorbed
            + gut.expelled;
        assert!(
            (mass - gut.ingested).abs() < 0.0002,
            "mass {mass} / {}",
            gut.ingested
        );
        let saved = serde_json::to_string(&world).unwrap();
        let restored: WasteWorld = serde_json::from_str(&saved).unwrap();
        assert_eq!(world.chains, restored.chains);
    }
    #[test]
    fn cleanup_only_removes_hovered_chain_and_ordinary_time_never_does() {
        let mut gut = DigestiveTract::default();
        let chain = |x| WasteChain {
            nodes: vec![
                WasteNode {
                    position: Vec2::new(x, 0.95),
                    velocity: Vec2::ZERO,
                    radius: 0.005,
                },
                WasteNode {
                    position: Vec2::new(x + 0.01, 0.95),
                    velocity: Vec2::ZERO,
                    radius: 0.005,
                },
            ],
            hardness: 0.5,
            rest_length: 0.008,
            rest_turn: 0.0,
            rest_turns: vec![0.0],
            remaining: 0.0,
            next_segment: 0.0,
            mass: 0.1,
            floor: 0.96,
            resting_seconds: 0.0,
            suction: None,
        };
        let mut world = WasteWorld {
            chains: vec![chain(0.3), chain(0.8)],
            ..Default::default()
        };
        let mut frame = DigestionFrame::default();
        frame.floor = 0.96;
        for _ in 0..3600 {
            world.step(&mut gut, frame, 1.0 / 60.0);
        }
        assert_eq!(world.chains.len(), 2);
        frame.cursor = Some(world.chains[0].nodes[0].position);
        for _ in 0..60 {
            world.step(&mut gut, frame, 1.0 / 60.0);
        }
        assert_eq!(world.chains.len(), 1);
        assert!(world.chains[0].nodes[0].position.x > 0.7);
    }
    #[test]
    fn food_and_hunger_change_transit_air_hardness_and_strain() {
        let mut dry = DigestiveTract::default();
        let mut moist = DigestiveTract::default();
        let mut f = food();
        f.warmth = 0.0;
        f.cohesion_bias = 1.0;
        dry.swallow(&f, 1.0, 0.1);
        f.warmth = 1.0;
        f.cohesion_bias = 0.0;
        moist.swallow(&f, 1.0, 1.0);
        assert!(dry.hardness() > moist.hardness());
        assert!(moist.swallowed_air > dry.swallowed_air);
        for _ in 0..2000 {
            dry.advance(0.1);
            moist.advance(0.1);
        }
        assert!(dry.stomach > moist.stomach);
        assert!(moist.bowel / 0.28 > dry.bowel / 0.56);
        assert!(moist.absorbed > dry.absorbed);
    }
}

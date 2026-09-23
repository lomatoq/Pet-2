//! First Light v20 choreography, evaluated analytically so frame rate cannot alter the path.
use super::{Sprite, smooth};
use glam::Vec2;
use std::f32::consts::{PI, TAU};
const FX: f32 = 0.65;
const COLORS: [[f32; 3]; 5] = [
    [0.50, 0.88, 1.0],
    [1.0, 0.40, 0.66],
    [0.76, 0.49, 1.0],
    [1.0, 0.80, 0.42],
    [0.39, 1.0, 0.77],
];
fn critical(t: f32, w: f32) -> f32 {
    let t = t.max(0.0);
    1.0 - (1.0 + w * t) * (-w * t).exp()
}
fn pulse(t: f32, at: f32, width: f32) -> f32 {
    (-((t - at) / width).powi(2)).exp()
}
fn spring(t: f32) -> f32 {
    let t = t.max(0.0);
    let wd = 20.0 * (1.0_f32 - 0.84 * 0.84).sqrt();
    1.0 - (-0.84 * 20.0 * t).exp() * ((wd * t).cos() + 0.84 * 20.0 / wd * (wd * t).sin())
}
fn rotate(p: Vec2, a: f32) -> Vec2 {
    Vec2::new(p.x * a.cos() - p.y * a.sin(), p.x * a.sin() + p.y * a.cos())
}
fn travel(t: f32, k: f32) -> f32 {
    -(-k * t.max(0.0)).exp_m1() / k
}
fn kick(t: f32, k: f32, rise: f32) -> f32 {
    (travel(t, k) - travel(t, rise)) * rise / (rise - k)
}
fn drag(t: f32, k: f32, rise: f32) -> f32 {
    (travel(t, k) - travel(t, rise)) / (1.0 / k - 1.0 / rise)
}
pub(super) struct Pose {
    pub center: Vec2,
    pub scale: f32,
    pub rotation: f32,
    pub height: f32,
    pub orb_scale: Vec2,
    time: f32,
    monitor: [f32; 4],
}
impl Pose {
    pub fn new(t: f32, m: [f32; 4]) -> Self {
        let [mx, my, mw, mh] = m;
        let h = (mh * 0.30).min(mw * 0.42).min(300.0);
        let elapsed = (t - 0.15).max(0.0);
        let idle = smooth(3.5, 6.5, t);
        let settle = smooth(2.6, 6.6, t);
        let center = Vec2::new(
            mx + mw * 0.5 - 42.0 * (1.0 - critical(elapsed, 1.0))
                + (t - 3.0).mul_add(0.57, 0.0).sin() * 1.7 * idle,
            my + mh * 0.5 - (mh * 0.5 + h * 0.56 + 45.0) * (1.0 - critical(elapsed, 1.35))
                + ((t - 3.7) * 0.76).sin() * 3.0 * idle,
        );
        let rotation = 0.087 * (1.0 - critical(elapsed, 0.9))
            - 0.018 * smooth(3.1, 5.6, t) * (1.0 - smooth(5.6, 7.1, t))
            - 0.010 * (1.0 - settle)
            + ((t - 3.2) * 0.63).sin() * 0.006 * idle;
        let scale = h / 1840.0 * (0.91 + 0.09 * critical(elapsed, 1.20));
        let charge = smooth(5.55, 7.45, t) * (1.0 - smooth(7.68, 8.25, t));
        let size = 1.0 - 0.010 * pulse(t, 6.27, 0.115) + 1.28 * critical(t - 7.79, 7.8);
        let breathe = 0.0023 * (t * 1.25).sin() + 0.0018 * charge * (t * 2.8).sin();
        Self {
            center,
            rotation,
            scale,
            height: h,
            orb_scale: Vec2::new(size * (1.0 + breathe), size * (1.0 - breathe * 0.7)),
            time: t,
            monitor: m,
        }
    }
    fn group(&self, side: f32) -> (Vec2, Vec2, f32) {
        if side == 0.0 {
            return (Vec2::new(0.0, -42.0), Vec2::ZERO, 0.0);
        }
        let roll = (self.time - 7.65).max(0.0);
        let flight = drag(roll, 0.90, 24.0);
        let unlock = spring(self.time - 6.48);
        let anticipate = pulse(self.time, 6.27, 0.115);
        let exit = (self.monitor[3] * 0.49 + self.height * 0.26 + 34.0) / self.scale;
        if side < 0.0 {
            (
                Vec2::new(0.0, -646.0),
                Vec2::new(
                    (7.0 * (roll * 4.0).sin() + 2.0 * (roll * 1.6).sin()) / self.scale * flight,
                    -42.0 * unlock + 8.0 * anticipate - exit * flight,
                ),
                0.004 * unlock + (0.29 + 0.04 * (roll * 2.1).sin()) * critical(roll, 2.0),
            )
        } else {
            (
                Vec2::new(0.0, 622.0),
                Vec2::new(
                    (6.0 * (roll * 3.6 + 1.2).sin() - 2.0 * (roll * 1.3).sin()) / self.scale
                        * flight,
                    50.0 * unlock - 7.0 * anticipate + exit * flight,
                ),
                -0.003 * unlock - (0.24 + 0.04 * (roll * 2.0).cos()) * critical(roll, 2.0),
            )
        }
    }
    pub fn angle(&self, side: f32) -> f32 {
        self.rotation + self.group(side).2
    }
    pub fn world(&self, p: Vec2, side: f32) -> Vec2 {
        let (pivot, offset, angle) = self.group(side);
        let size = if side == 0.0 {
            self.orb_scale
        } else {
            Vec2::ONE
        };
        self.center
            + rotate(
                (rotate((p - pivot) * size, angle) + pivot + offset) * self.scale,
                self.rotation,
            )
    }
    fn membrane(&self, a: f32) -> Vec2 {
        let open = ((self.time - 7.79) / 0.92).clamp(0.0, 1.0);
        let release = smooth(0.012, 0.47, open);
        let wave = release
            * (0.074 * (3.0 * a - open * 6.3).sin()
                + 0.049 * (5.0 * a + open * 4.6).sin()
                + 0.025 * (7.0 * a - open * 8.0).sin());
        let stretch = 1.0 + 0.055 * release * (open * 5.2).sin();
        let (mut lo, mut hi) = (0.68, 1.36);
        for _ in 0..13 {
            let radius = (lo + hi) * 0.5;
            let sx = a.cos() * radius;
            let sy = a.sin() * radius;
            let px =
                (sx / (1.0 + wave) + release * 0.033 * (sy * 3.3 - open * 4.0).sin()) * stretch;
            let py =
                (sy / (1.0 + wave) + release * 0.033 * (sx * 3.6 + open * 4.5).cos()) / stretch;
            if px * px + py * py > 1.0 {
                hi = radius
            } else {
                lo = radius
            }
        }
        let radius = (lo + hi) * 0.5;
        self.world(
            Vec2::new(
                -2.4 + a.cos() * radius * 624.5,
                -48.2 + a.sin() * radius * 628.1,
            ),
            0.0,
        )
    }
}
struct Seed {
    birth: f32,
    life: f32,
    angle: f32,
    a: f32,
    b: f32,
    c: f32,
    z: f32,
    lag: f32,
    size: f32,
    color: [f32; 3],
}
struct Random(u32);
impl Random {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64 / 4294967296.0) as f32
    }
}
fn seed(r: &mut Random, i: usize, drop: bool) -> Seed {
    let birth = if drop {
        7.845 + r.next() * 0.25
    } else {
        0.18 + r.next() * 1.35
    };
    let life = if drop {
        1.10 + r.next() * 1.45
    } else {
        1.65 + r.next() * 1.25
    };
    let angle = r.next() * TAU;
    let a = r.next();
    let b = r.next();
    let c = r.next();
    let z = r.next() * 2.0 - 1.0;
    let lag = if drop { 0.0 } else { 0.09 + r.next() * 0.28 };
    let size = if drop {
        5.0 + r.next() * 9.0
    } else {
        4.8 + r.next() * 6.2
    };
    Seed {
        birth,
        life,
        angle,
        a,
        b,
        c,
        z,
        lag,
        size,
        color: COLORS[(i + if drop { 2 } else { 0 }) % 5],
    }
}
fn dust_at(p: &Seed, t: f32, m: [f32; 4]) -> Vec2 {
    let at = (t - p.lag).max(0.0);
    let root = Pose::new(at, m);
    let past = Pose::new((at - 0.08).max(0.0), m);
    let d = 1248.9952 * root.scale;
    let theta = p.angle + t * (0.065 + 0.048 * p.b) + 0.055 * (t * 0.62 + p.c * 7.0).sin();
    let rad = d * (0.54 + 0.34 * p.a);
    let projection = (1.0 - p.z * p.z * 0.64).sqrt();
    root.center
        + (root.center - past.center) * (0.24 * p.lag / 0.08)
        + Vec2::new(
            rad * theta.cos() * projection + (t * 0.76 + p.a * 8.0).sin() * d * 0.018,
            -42.0 * root.scale
                + rad * theta.sin() * (0.86 + 0.26 * p.b)
                + (t * 0.63 + p.b * 8.0).cos() * d * 0.019,
        )
}
#[allow(clippy::too_many_arguments)]
fn sprite(
    out: &mut Vec<Sprite>,
    p: Vec2,
    size: Vec2,
    angle: f32,
    alpha: f32,
    color: [f32; 3],
    kind: f32,
    front: bool,
    v: [u32; 2],
    phase: [f32; 2],
) {
    if alpha < 0.0005 {
        return;
    }
    out.push(Sprite {
        rect: [p.x, p.y, size.x * 0.5, size.y * 0.5],
        effect: [0.0, kind, phase[0], phase[1]],
        style: [
            angle,
            alpha,
            1.0 / v[0].max(1) as f32,
            1.0 / v[1].max(1) as f32,
        ],
        color: [color[0], color[1], color[2], f32::from(front)],
    });
}
fn ribbon(out: &mut Vec<Sprite>, points: &[(Vec2, f32)], color: [f32; 3], alpha: f32, v: [u32; 2]) {
    for (i, pair) in points.windows(2).enumerate() {
        let (a, wa) = pair[0];
        let (b, wb) = pair[1];
        let delta = b - a;
        if delta.length() < 0.01 {
            continue;
        }
        sprite(
            out,
            (a + b) * 0.5,
            Vec2::new((wa + wb) * 0.5, delta.length() + 0.65),
            delta.y.atan2(delta.x) - PI * 0.5,
            alpha,
            color,
            4.0,
            false,
            v,
            [
                i as f32 / (points.len() - 1) as f32,
                (i + 1) as f32 / (points.len() - 1) as f32,
            ],
        );
    }
}
pub(super) fn append(out: &mut Vec<Sprite>, t: f32, m: [f32; 4], v: [u32; 2]) {
    let pose = Pose::new(t, m);
    let scale = pose.height / 1840.0 / 0.27;
    if (0.35..7.15).contains(&t) {
        let speed = pose
            .center
            .distance(Pose::new((t - 0.035).max(0.0), m).center)
            / 0.035;
        let alpha = (speed / 160.0).clamp(0.0, 1.0) * 0.82 * (1.0 - smooth(5.0, 6.98, t)) * FX;
        for (base, jitter, color, strength, width, phase) in [
            (-0.112, 7.2, [0.52, 0.86, 1.0], 0.38, 0.66, 0.0),
            (-0.018, 4.6, [0.80, 0.91, 1.0], 0.82, 1.05, 0.8),
            (0.026, 2.6, [1.0, 0.97, 0.92], 0.62, 0.28, 1.35),
            (0.096, 8.0, [0.98, 0.72, 0.95], 0.34, 0.62, 1.9),
        ] {
            let points: Vec<_> = (0..92)
                .map(|i| {
                    let q = i as f32 / 91.0;
                    let root = Pose::new((t - q * 3.55).max(0.0), m);
                    let flow = (q * 5.1 + t * 0.52 + phase + base * 10.0).sin();
                    (
                        root.center
                            + Vec2::new(
                                pose.height * base + flow * jitter * (0.15 + 0.85 * q),
                                -pose.height * 0.19
                                    + 34.0 * q
                                    + (q * 4.4 + t * 0.44 + phase).cos() * 1.8,
                            ),
                        (34.0 + 58.0 * (q * PI).sin()) * (1.0 - 0.22 * q) * scale * width,
                    )
                })
                .collect();
            ribbon(out, &points, color, alpha * strength, v);
        }
    }
    let age = t - 7.65;
    if (0.0..2.2).contains(&age) {
        let fade = (1.0 - smooth(0.22, 2.08, age)) * FX;
        for (side, color) in [(-1.0, [0.62, 0.85, 1.0]), (1.0, [0.98, 0.69, 0.86])] {
            for (width, strength) in [(18.0, 0.40), (8.8, 0.24)] {
                let points: Vec<_> = (0..32)
                    .map(|i| {
                        let q = i as f32 / 31.0;
                        let root = Pose::new((t - q * 0.42).max(7.65), m);
                        let pivot = if side < 0.0 { -646.0 } else { 622.0 };
                        let wobble =
                            (q * 8.2 + t * 5.6 + side * 0.9).sin() * 3.2 * scale * (1.0 - q);
                        (
                            root.world(Vec2::new(0.0, pivot), side)
                                + Vec2::new(wobble * 0.55, wobble * 0.12),
                            (width - width * 0.56 * q) * scale,
                        )
                    })
                    .collect();
                ribbon(out, &points, color, strength * fade, v);
            }
        }
    }
    // Persistent arrival dust, then inherited motion and pressure impulse at opening.
    let mut rng = Random(20260919);
    for i in 0..102 {
        let drop = i >= 64;
        let p = seed(&mut rng, i - if drop { 64 } else { 0 }, drop);
        let age = t - if drop { p.birth } else { 7.79 };
        if t < p.birth || age >= p.life {
            continue;
        }
        let (position, velocity, alpha, size) = if !drop {
            let visibility = smooth(p.birth, p.birth + 0.7, t);
            let fade = if age < 0.0 {
                1.0
            } else {
                1.0 - smooth(0.14 * p.life, p.life, age)
            };
            let shimmer = 0.80 + 0.20 * (t.min(7.79) * 0.82 + p.c * 8.0).sin().powi(2);
            let position = if age < 0.0 {
                dust_at(&p, t, m)
            } else {
                let origin = dust_at(&p, 7.79, m);
                let velocity = (dust_at(&p, 7.80, m) - dust_at(&p, 7.78, m)) / 0.02;
                let radial = (origin - Pose::new(7.79, m).world(Vec2::new(0.0, -42.0), 0.0))
                    .normalize_or_zero();
                let impulse = (radial * (135.0 + 155.0 * p.b)
                    + Vec2::new(-radial.y, radial.x) * (p.c - 0.5) * 46.0)
                    * scale;
                let k = 0.87 + 0.48 * p.a;
                origin
                    + velocity * travel(age, k)
                    + impulse * kick(age, k, 28.0)
                    + Vec2::new((p.a - 0.5) * 8.0, -6.0) * scale * (age - travel(age, k))
            };
            (
                position,
                Vec2::ZERO,
                (0.28 + 0.28 * p.c) * shimmer * visibility * fade * FX,
                p.size
                    * scale
                    * (1.18 + 0.22 * p.z)
                    * (1.0 - 0.34 * smooth(0.25, 1.0, age.max(0.0) / p.life)),
            )
        } else {
            let origin = Pose::new(p.birth, m).membrane(p.angle);
            let velocity = (Pose::new(p.birth + 0.004, m).membrane(p.angle)
                - Pose::new(p.birth - 0.004, m).membrane(p.angle))
                / 0.008;
            let velocity = velocity * (0.28_f32.min(155.0 * scale / velocity.length().max(0.001)));
            let kick_vector = Vec2::from_angle(p.angle) * (150.0 + 190.0 * p.a) * scale
                + Vec2::new(-p.angle.sin(), p.angle.cos()) * (p.c - 0.5) * 105.0 * scale;
            let k = 0.92 + 0.62 * p.b;
            let position = origin
                + velocity * travel(age, k)
                + kick_vector * kick(age, k, 34.0)
                + Vec2::Y * 24.0 * scale * (age - travel(age, k)) / k;
            let velocity = velocity * (-k * age).exp()
                + kick_vector * ((-k * age).exp() - (-34.0 * age).exp()) * 34.0 / (34.0 - k)
                + Vec2::Y * 24.0 * scale * travel(age, k);
            (
                position,
                velocity,
                smooth(0.0, 0.035, age)
                    * (1.0 - smooth(0.08, 1.0, age / p.life))
                    * (0.62 + 0.27 * p.c)
                    * FX,
                p.size * scale * (1.0 - 0.64 * smooth(0.23, 1.0, age / p.life)),
            )
        };
        let stretch = if drop {
            1.0 + (velocity.length() / (550.0 * scale)).min(0.62)
        } else {
            1.0
        };
        sprite(
            out,
            position,
            Vec2::new(size * stretch, size / stretch.sqrt()),
            velocity.y.atan2(velocity.x),
            alpha,
            p.color,
            if drop { 6.0 } else { 2.0 },
            p.z >= if drop { -0.35 } else { 0.0 },
            v,
            [p.c, 0.0],
        );
    }
    // Five independently moving metabol lights radiate outside the glass.
    let charge = smooth(5.55, 7.45, t) * (1.0 - smooth(7.68, 8.25, t));
    let open = ((t - 7.79) / 0.92).clamp(0.0, 1.0);
    let aura_fade = smooth(0.35, 1.75, t) * (1.0 - smooth(0.02, 0.98, open));
    for (i, color) in [
        [1.0, 0.22, 0.57],
        [0.13, 0.80, 1.0],
        [0.55, 0.29, 1.0],
        [1.0, 0.74, 0.18],
        [0.18, 1.0, 0.64],
    ]
    .into_iter()
    .enumerate()
    {
        let fi = i as f32;
        let orbit = 0.53 + 0.095 * (t * 0.70 + fi * 1.37).sin();
        let spin = 0.60 * (t * 0.62 + 0.95 * orbit).sin() + 0.14 * (t * 1.30 + orbit * 5.0).sin();
        let angle = fi * 2.399963
            + t * (0.48 + 0.09 * (i % 3) as f32)
            + 0.21 * (t * 0.63 + fi).sin()
            + spin;
        let position = pose.world(
            Vec2::new(
                -2.4 + angle.cos() * orbit * 624.5,
                -48.2 + angle.sin() * orbit * 0.97 * 628.1,
            ),
            0.0,
        );
        let energy =
            0.86 + 0.21 * (t * 0.82 + fi * 0.97).sin() + 0.09 * (t * 1.57 + fi * 2.2).sin();
        let size = 1249.0 * pose.scale * pose.orb_scale.x * 1.55;
        sprite(
            out,
            position,
            Vec2::splat(size),
            0.0,
            0.135 * energy * (1.0 + 0.26 * charge) * aura_fade * 1.6 * FX,
            color,
            5.0,
            false,
            v,
            [0.0, 0.0],
        );
    }
    // Lamp ignition and launch glare follow their own rotating holders.
    let pre = smooth(6.05, 7.77, t) * (1.0 - smooth(7.77, 7.97, t));
    let flash = 0.38 * pulse(t, 6.48, 0.10)
        + 0.72 * pulse(t, 7.79, 0.11)
        + 0.30 * pre * (0.72 + 0.28 * (t * 10.8 + 0.55).sin().powi(2));
    let strength =
        (0.085 + 1.42 * smooth(5.10, 7.32, t) + flash) * (1.0 - smooth(8.13, 9.01, t)) * FX;
    for (position, side, height) in [
        (Vec2::new(2.0, -553.0), -1.0, 212.0),
        (Vec2::new(-4.0, 610.0), 1.0, 188.0),
    ] {
        let p = pose.world(position, side);
        let a = pose.angle(side);
        sprite(
            out,
            p,
            Vec2::new(220.0, 400.0) * pose.scale,
            a,
            strength * 1.55,
            [1.0, 0.82, 0.48],
            3.0,
            true,
            v,
            [height, 0.0],
        );
        for (w, h, alpha, col) in [
            (900.0, 112.0, 0.52, [1.0, 0.94, 0.74]),
            (520.0, 700.0, 0.20, [1.0, 0.84, 0.47]),
            (150.0, 820.0, 0.10, [1.0, 0.98, 0.92]),
        ] {
            sprite(
                out,
                p,
                Vec2::new(w, h) * pose.scale,
                a,
                strength * alpha,
                col,
                5.0,
                true,
                v,
                [0.0, 0.0],
            );
        }
    }
    // Seam mist, soft smoke puffs and holder launch sparks.
    let mist = smooth(7.61, 8.17, t) * (1.0 - smooth(8.13, 9.07, t));
    for (i, point) in [
        Vec2::new(-182.0, -350.0),
        Vec2::new(182.0, -350.0),
        Vec2::new(-178.0, 292.0),
        Vec2::new(178.0, 292.0),
    ]
    .into_iter()
    .enumerate()
    {
        let base = pose.world(point, 0.0);
        let drift = Vec2::new(
            (t * 7.0 + i as f32 * 1.7).sin() * 26.0,
            (t * 6.1 + i as f32 * 1.2).cos() * 18.0,
        ) * scale
            * mist;
        sprite(
            out,
            base + drift,
            Vec2::new(240.0, 148.0) * scale,
            0.0,
            mist * 0.22 * FX,
            [1.0, 0.94, 0.82],
            5.0,
            true,
            v,
            [0.0, 0.0],
        );
        sprite(
            out,
            base - drift * 0.45,
            Vec2::new(164.0, 188.0) * scale,
            0.0,
            mist * 0.14 * FX,
            [0.84, 0.94, 1.0],
            5.0,
            true,
            v,
            [0.0, 0.0],
        );
    }
    for i in 0..40 {
        let a = rng.next();
        let b = rng.next();
        let c = rng.next();
        let life = if i < 18 { 0.9 + b * 1.5 } else { 1.2 + b };
        let birth = if i < 18 {
            7.68 + a * 0.28
        } else {
            7.74 + a * 0.26
        };
        let age = t - birth;
        if age < 0.0 || age > life {
            continue;
        }
        let side = if i % 2 == 0 { -1.0 } else { 1.0 };
        let position = if i < 18 {
            Pose::new(birth, m).world(
                Vec2::new(0.0, if side < 0.0 { -646.0 } else { 622.0 }),
                side,
            ) + Vec2::new((a - 0.5) * 96.0, -side * (24.0 + 90.0 * b))
                * scale
                * drag(age, 1.1, 20.0)
        } else {
            Pose::new(7.79, m).world(Vec2::new(0.0, -42.0), 0.0)
                + Vec2::from_angle(a * TAU + 0.22 * (b * TAU + t * 1.5).sin())
                    * (42.0 + 110.0 * a)
                    * scale
                    * drag(age, 0.6, 12.0)
                - Vec2::Y * age * 8.0 * scale
        };
        let size = if i < 18 {
            3.0 + c * 6.0
        } else {
            (22.0 + c * 28.0) * 1.4
        };
        let alpha = if i < 18 {
            smooth(0.0, 0.10, age / life) * (1.0 - smooth(0.22, 1.0, age / life)) * 0.27
        } else {
            (1.0 - smooth(0.0, 1.0, age / life)) * (0.10 + 0.10 * c)
        };
        sprite(
            out,
            position,
            Vec2::splat(size * scale),
            0.0,
            alpha * FX,
            if i < 18 {
                COLORS[i % 5]
            } else {
                [0.95, 0.93, 1.0]
            },
            if i < 18 { 2.0 } else { 5.0 },
            true,
            v,
            [0.0, 0.0],
        );
    }
    // Reference pre-burst energy, holder edge accents and grazing glass reflections.
    let center = pose.world(Vec2::new(0.0, -42.0), 0.0);
    let pre = smooth(6.0, 7.77, t) * (1.0 - smooth(7.77, 8.01, t)) * FX;
    let beat = 0.60 + 0.40 * (t * 4.2 + 0.25).sin().powi(2);
    for (size, offset, angle, alpha, color) in [
        (
            Vec2::splat(340.0),
            0.0,
            0.0,
            0.060 * beat,
            [0.72, 0.88, 1.0],
        ),
        (
            Vec2::new(420.0, 280.0),
            16.0,
            0.0,
            0.040 * (0.72 + 0.28 * beat),
            [1.0, 0.73, 0.82],
        ),
        (
            Vec2::new(620.0, 260.0),
            0.0,
            t * 0.52,
            0.038 * (0.78 + 0.22 * (t * 3.8 + 1.1).sin()),
            [1.0, 0.46, 0.78],
        ),
        (
            Vec2::new(300.0, 560.0),
            0.0,
            -t * 0.38,
            0.030 * (0.72 + 0.28 * (t * 4.4 + 2.1).sin()),
            [0.39, 1.0, 0.82],
        ),
        (
            Vec2::new(260.0, 180.0),
            -10.0,
            0.0,
            0.026 * (0.74 + 0.26 * (t * 2.9 + 0.6).sin()),
            [1.0, 0.92, 0.78],
        ),
    ] {
        sprite(
            out,
            center + Vec2::Y * offset * scale,
            size * scale,
            angle,
            alpha * pre,
            color,
            5.0,
            true,
            v,
            [0.0, 0.0],
        );
    }
    let k = (0.012 + 0.058 * smooth(4.95, 7.40, t)) * (1.0 - smooth(8.01, 8.85, t)) * FX;
    for (side, y, size, opacity, color) in [
        (
            -1.0,
            -448.0,
            Vec2::new(548.0, 20.0),
            1.0,
            [0.96, 0.985, 1.0],
        ),
        (1.0, 502.0, Vec2::new(520.0, 18.0), 0.92, [0.96, 0.985, 1.0]),
        (
            -1.0,
            -514.0,
            Vec2::new(210.0, 58.0),
            0.75,
            [1.0, 0.93, 0.82],
        ),
        (1.0, 566.0, Vec2::new(210.0, 56.0), 0.72, [1.0, 0.93, 0.82]),
    ] {
        sprite(
            out,
            pose.world(Vec2::new(0.0, y), side),
            size * scale,
            pose.angle(side),
            k * opacity,
            color,
            5.0,
            true,
            v,
            [0.0, 0.0],
        );
    }
    for (i, point) in [
        Vec2::new(-182.0, -350.0),
        Vec2::new(182.0, -350.0),
        Vec2::new(-178.0, 292.0),
        Vec2::new(178.0, 292.0),
    ]
    .into_iter()
    .enumerate()
    {
        let p = pose.world(point, 0.0);
        let twinkle = 0.72 + 0.28 * (t * 5.1 + i as f32 * 1.7).sin();
        sprite(
            out,
            p,
            Vec2::new(74.0, 20.0) * scale,
            pose.rotation + if i < 2 { 0.0 } else { PI * 0.08 },
            k * 0.85 * twinkle,
            [1.0, 0.98, 0.94],
            5.0,
            true,
            v,
            [0.0, 0.0],
        );
        sprite(
            out,
            p,
            Vec2::splat(30.0) * scale,
            0.0,
            k * 0.40 * twinkle,
            [0.76, 0.90, 1.0],
            5.0,
            true,
            v,
            [0.0, 0.0],
        );
    }
    for (x, rotation, color) in [
        (-322.0, -0.24, [0.86, 0.94, 1.0]),
        (322.0, 0.24, [1.0, 0.94, 0.88]),
    ] {
        sprite(
            out,
            pose.world(Vec2::new(x, -60.0), 0.0),
            Vec2::new(118.0, 248.0) * scale,
            pose.rotation + rotation,
            k * 0.42,
            color,
            5.0,
            true,
            v,
            [0.0, 0.0],
        );
    }
    let fade = (1.0 - smooth(7.89, 8.59, t)) * 0.75 + 0.25;
    for (side, y, size, alpha) in [
        (-1.0, -470.0, Vec2::new(430.0, 110.0), 0.035),
        (1.0, 420.0, Vec2::new(410.0, 108.0), 0.030),
    ] {
        sprite(
            out,
            pose.world(Vec2::new(0.0, y), side),
            size * scale,
            0.0,
            alpha * fade * FX,
            [0.92, 0.98, 1.0],
            5.0,
            true,
            v,
            [0.0, 0.0],
        );
    }
}

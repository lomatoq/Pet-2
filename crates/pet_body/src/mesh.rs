use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use lifecore::{BodyGenome, stable_hash_bytes};
use thiserror::Error;

const SPHERE_LATITUDES: usize = 10;
const SPHERE_LONGITUDES: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub part: f32,
}

impl MeshVertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32];

    #[must_use]
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProceduralMesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub minimum: Vec3,
    pub maximum: Vec3,
}

#[derive(Debug, Error)]
pub enum MeshError {
    #[error("mesh contains an out-of-range index")]
    InvalidIndex,
    #[error("mesh contains non-finite vertex data")]
    NonFiniteVertex,
    #[error("mesh has non-positive bounds")]
    NonPositiveBounds,
    #[error("mesh contains a strongly inverted triangle")]
    InvertedTriangle,
}

impl ProceduralMesh {
    pub fn generate(genome: &BodyGenome) -> Result<Self, MeshError> {
        let primary = hsv_to_rgba(genome.primary_color_hsv, 1.0);
        let secondary = hsv_to_rgba(genome.secondary_color_hsv, 1.0);
        let glow = hsv_to_rgba(genome.glow_color_hsv, 1.0);
        let mut builder = MeshBuilder::default();
        builder.add_ellipsoid(
            Vec3::new(0.0, -0.08, 0.0),
            Vec3::new(
                genome.body_width * 0.42,
                genome.body_length * 0.46,
                genome.body_width * (0.30 + genome.body_roundness * 0.10),
            ),
            primary,
            0.0,
        );
        let head_center = Vec3::new(0.0, genome.body_length * 0.33, 0.04);
        builder.add_ellipsoid(
            head_center,
            Vec3::new(
                genome.head_ratio * 0.48,
                genome.head_ratio * 0.44,
                genome.head_ratio * 0.42,
            ),
            secondary,
            1.0,
        );
        for side in [-1.0_f32, 1.0] {
            let eye_center = head_center
                + Vec3::new(
                    side * genome.eye_spacing * 0.5,
                    0.03,
                    genome.head_ratio * 0.39,
                );
            builder.add_ellipsoid(
                eye_center,
                Vec3::splat(genome.eye_size * 0.52),
                [0.04, 0.055, 0.07, 1.0],
                4.0,
            );
            builder.add_ellipsoid(
                eye_center + Vec3::new(0.0, 0.0, genome.eye_size * 0.46),
                Vec3::splat(genome.eye_size * genome.pupil_ratio * 0.24),
                glow,
                5.0,
            );
        }
        builder.add_wing(genome, -1.0, primary, 2.0);
        builder.add_wing(genome, 1.0, primary, 3.0);
        for side in [-1.0_f32, 1.0] {
            let start = Vec3::new(
                side * genome.body_width * 0.25,
                -genome.body_length * 0.30,
                0.02,
            );
            let end = start + Vec3::new(side * 0.04, -genome.limb_length, 0.04);
            builder.add_tube(start, end, genome.limb_thickness, secondary, 6.0, 8);
        }
        builder.add_tail(genome, secondary, 7.0);
        if genome.ear_fin_size > 0.01 {
            builder.add_fin(
                head_center + Vec3::new(-genome.head_ratio * 0.22, genome.head_ratio * 0.28, 0.0),
                -1.0,
                genome.ear_fin_size,
                secondary,
                8.0,
            );
            builder.add_fin(
                head_center + Vec3::new(genome.head_ratio * 0.22, genome.head_ratio * 0.28, 0.0),
                1.0,
                genome.ear_fin_size,
                secondary,
                9.0,
            );
        }
        if genome.crest_size > 0.01 {
            builder.add_fin(
                head_center + Vec3::new(0.0, genome.head_ratio * 0.40, -0.02),
                0.0,
                genome.crest_size,
                glow,
                10.0,
            );
        }
        builder.finish()
    }

    pub fn validate(&self) -> Result<(), MeshError> {
        if self
            .indices
            .iter()
            .any(|index| *index as usize >= self.vertices.len())
        {
            return Err(MeshError::InvalidIndex);
        }
        if self.vertices.iter().any(|vertex| {
            vertex
                .position
                .into_iter()
                .chain(vertex.normal)
                .chain(vertex.color)
                .chain(std::iter::once(vertex.part))
                .any(|value| !value.is_finite())
                || Vec3::from_array(vertex.normal).length_squared() < 0.5
        }) {
            return Err(MeshError::NonFiniteVertex);
        }
        let extent = self.maximum - self.minimum;
        if !extent.is_finite() || extent.min_element() <= 0.0 {
            return Err(MeshError::NonPositiveBounds);
        }
        for triangle in self.indices.chunks_exact(3) {
            let a = Vec3::from_array(self.vertices[triangle[0] as usize].position);
            let b = Vec3::from_array(self.vertices[triangle[1] as usize].position);
            let c = Vec3::from_array(self.vertices[triangle[2] as usize].position);
            let geometric = (b - a).cross(c - a);
            if geometric.length_squared() < 1.0e-12 {
                continue;
            }
            let average_normal = (Vec3::from_array(self.vertices[triangle[0] as usize].normal)
                + Vec3::from_array(self.vertices[triangle[1] as usize].normal)
                + Vec3::from_array(self.vertices[triangle[2] as usize].normal))
                / 3.0;
            if geometric
                .normalize()
                .dot(average_normal.normalize_or_zero())
                < -0.25
            {
                return Err(MeshError::InvertedTriangle);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn stable_hash(&self) -> u64 {
        let mut bytes = Vec::with_capacity(self.vertices.len() * 44 + self.indices.len() * 4);
        for vertex in &self.vertices {
            for value in vertex
                .position
                .into_iter()
                .chain(vertex.normal)
                .chain(vertex.color)
                .chain(std::iter::once(vertex.part))
            {
                bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        for index in &self.indices {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        stable_hash_bytes(&bytes)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedHitShape {
    ellipses: Vec<Ellipse>,
    capsules: Vec<Capsule>,
    polygons: Vec<Vec<Vec2>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Ellipse {
    center: Vec2,
    radii: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Capsule {
    start: Vec2,
    end: Vec2,
    radius: f32,
}

impl ProjectedHitShape {
    #[must_use]
    pub fn from_genome(genome: &BodyGenome) -> Self {
        let body_x = (genome.body_width * 0.20).clamp(0.10, 0.26);
        let body_y = (genome.body_length * 0.19).clamp(0.13, 0.28);
        let head = (genome.head_ratio * 0.20).clamp(0.08, 0.17);
        let wing = (genome.wing_span * 0.16).clamp(0.08, 0.24);
        let mut capsules = Vec::new();
        for segment in 0..genome.tail_segments {
            let t0 = f32::from(segment) / f32::from(genome.tail_segments);
            let t1 = f32::from(segment + 1) / f32::from(genome.tail_segments);
            capsules.push(Capsule {
                start: Vec2::new(0.5 + t0 * 0.12, 0.68 + t0 * 0.20),
                end: Vec2::new(0.5 + t1 * 0.12, 0.68 + t1 * 0.20),
                radius: (genome.tail_thickness * 0.10 * (1.0 - t0 * 0.5)).max(0.008),
            });
        }
        Self {
            ellipses: vec![
                Ellipse {
                    center: Vec2::new(0.5, 0.56),
                    radii: Vec2::new(body_x, body_y),
                },
                Ellipse {
                    center: Vec2::new(0.5, 0.34),
                    radii: Vec2::splat(head),
                },
            ],
            capsules,
            polygons: vec![
                vec![
                    Vec2::new(0.48, 0.48),
                    Vec2::new(0.5 - wing, 0.42),
                    Vec2::new(0.5 - wing * 0.78, 0.62),
                ],
                vec![
                    Vec2::new(0.52, 0.48),
                    Vec2::new(0.5 + wing, 0.42),
                    Vec2::new(0.5 + wing * 0.78, 0.62),
                ],
            ],
        }
    }

    #[must_use]
    pub fn contains(&self, point: Vec2) -> bool {
        self.ellipses.iter().any(|ellipse| {
            let delta = (point - ellipse.center) / ellipse.radii;
            delta.length_squared() <= 1.0
        }) || self
            .capsules
            .iter()
            .any(|capsule| distance_to_segment(point, capsule.start, capsule.end) <= capsule.radius)
            || self
                .polygons
                .iter()
                .any(|polygon| point_in_polygon(point, polygon))
    }
}

#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<MeshVertex>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn add_ellipsoid(&mut self, center: Vec3, radii: Vec3, color: [f32; 4], part: f32) {
        let base = self.vertices.len() as u32;
        for latitude in 0..=SPHERE_LATITUDES {
            let phi = std::f32::consts::PI * latitude as f32 / SPHERE_LATITUDES as f32;
            for longitude in 0..=SPHERE_LONGITUDES {
                let theta = std::f32::consts::TAU * longitude as f32 / SPHERE_LONGITUDES as f32;
                let unit = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
                let normal = (unit / radii).normalize_or_zero();
                self.vertices.push(MeshVertex {
                    position: (center + unit * radii).to_array(),
                    normal: normal.to_array(),
                    color,
                    part,
                });
            }
        }
        let row = (SPHERE_LONGITUDES + 1) as u32;
        for latitude in 0..SPHERE_LATITUDES as u32 {
            for longitude in 0..SPHERE_LONGITUDES as u32 {
                let a = base + latitude * row + longitude;
                let b = a + row;
                let c = b + 1;
                let d = a + 1;
                self.indices.extend_from_slice(&[a, d, b, d, c, b]);
            }
        }
    }

    fn add_wing(&mut self, genome: &BodyGenome, side: f32, mut color: [f32; 4], part: f32) {
        color[3] = (1.0 - genome.wing_translucency).clamp(0.22, 0.9);
        const U: usize = 8;
        const V: usize = 4;
        let base = self.vertices.len() as u32;
        for u_index in 0..=U {
            let u = u_index as f32 / U as f32;
            for v_index in 0..=V {
                let v = v_index as f32 / V as f32;
                let root_x = genome.body_width * 0.34;
                let x = side * (root_x + u * genome.wing_span * 0.48);
                let y = 0.05 + (v - 0.5) * genome.wing_aspect * 0.30 - u * 0.08;
                let z = -0.02 + (std::f32::consts::PI * u).sin() * genome.wing_roundness * 0.10;
                let normal = Vec3::new(-side * 0.08, 0.18, 1.0).normalize();
                self.vertices.push(MeshVertex {
                    position: [x, y, z],
                    normal: normal.to_array(),
                    color,
                    part,
                });
            }
        }
        let row = (V + 1) as u32;
        for u in 0..U as u32 {
            for v in 0..V as u32 {
                let a = base + u * row + v;
                let b = a + row;
                let c = b + 1;
                let d = a + 1;
                if side < 0.0 {
                    self.indices.extend_from_slice(&[a, d, b, d, c, b]);
                } else {
                    self.indices.extend_from_slice(&[a, b, d, d, b, c]);
                }
            }
        }
    }

    fn add_tube(
        &mut self,
        start: Vec3,
        end: Vec3,
        radius: f32,
        color: [f32; 4],
        part: f32,
        sides: usize,
    ) {
        let base = self.vertices.len() as u32;
        let direction = (end - start).normalize_or_zero();
        let tangent = if direction.z.abs() < 0.9 {
            direction.cross(Vec3::Z).normalize_or_zero()
        } else {
            direction.cross(Vec3::X).normalize_or_zero()
        };
        let bitangent = direction.cross(tangent).normalize_or_zero();
        for ring in 0..=1 {
            let center = if ring == 0 { start } else { end };
            let taper = if ring == 0 { 1.0 } else { 0.72 };
            for side_index in 0..=sides {
                let angle = std::f32::consts::TAU * side_index as f32 / sides as f32;
                let normal = (tangent * angle.cos() + bitangent * angle.sin()).normalize_or_zero();
                self.vertices.push(MeshVertex {
                    position: (center + normal * radius * taper).to_array(),
                    normal: normal.to_array(),
                    color,
                    part,
                });
            }
        }
        let row = (sides + 1) as u32;
        for side_index in 0..sides as u32 {
            let a = base + side_index;
            let b = a + row;
            self.indices
                .extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }

    fn add_tail(&mut self, genome: &BodyGenome, color: [f32; 4], part: f32) {
        let segments = usize::from(genome.tail_segments);
        let mut previous = Vec3::new(0.0, -genome.body_length * 0.43, -0.12);
        for segment in 0..segments {
            let t = (segment + 1) as f32 / segments as f32;
            let next = Vec3::new(
                t.sin() * genome.tail_length * 0.16,
                -genome.body_length * 0.43 - t * genome.tail_length * 0.48,
                -0.12 + (t * std::f32::consts::PI).sin() * 0.08,
            );
            self.add_tube(
                previous,
                next,
                genome.tail_thickness * (1.0 - t * 0.55),
                color,
                part + segment as f32 * 0.01,
                8,
            );
            previous = next;
        }
    }

    fn add_fin(&mut self, base_center: Vec3, side: f32, size: f32, color: [f32; 4], part: f32) {
        let start = self.vertices.len() as u32;
        let points = [
            base_center + Vec3::new(-size * 0.45, 0.0, 0.0),
            base_center + Vec3::new(size * 0.45, 0.0, 0.0),
            base_center + Vec3::new(side * size * 0.22, size, -size * 0.12),
        ];
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize_or_zero();
        for point in points {
            self.vertices.push(MeshVertex {
                position: point.to_array(),
                normal: normal.to_array(),
                color,
                part,
            });
        }
        self.indices
            .extend_from_slice(&[start, start + 1, start + 2]);
    }

    fn finish(self) -> Result<ProceduralMesh, MeshError> {
        let (minimum, maximum) = self.vertices.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), vertex| {
                let position = Vec3::from_array(vertex.position);
                (minimum.min(position), maximum.max(position))
            },
        );
        let mesh = ProceduralMesh {
            vertices: self.vertices,
            indices: self.indices,
            minimum,
            maximum,
        };
        mesh.validate()?;
        Ok(mesh)
    }
}

fn hsv_to_rgba(hsv: Vec3, alpha: f32) -> [f32; 4] {
    let chroma = hsv.z * hsv.y;
    let hue = hsv.x.rem_euclid(1.0) * 6.0;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let value = hsv.z - chroma;
    [r + value, g + value, b + value, alpha]
}

fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let t = if segment.length_squared() <= f32::EPSILON {
        0.0
    } else {
        (point - start).dot(segment) / segment.length_squared()
    }
    .clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len().saturating_sub(1);
    for current in 0..polygon.len() {
        let a = polygon[current];
        let b = polygon[previous];
        if ((a.y > point.y) != (b.y > point.y))
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

//! ATLANTIS: ciudad bajo una cúpula de cristal, cardúmenes de peces, burbujas y un remolino.
//!
//!   - CIUDAD (assets/props/ciudad_atlantis.obj) y su CÚPULA: media esfera generada aquí,
//!     ADITIVA, con brillo de cristal en los bordes (fresnel) y una red de líneas que titila.
//!   - PECES (assets/props/pez.obj): cardúmenes que nadan alrededor del planeta. Cada pez va en
//!     su lugar dentro del grupo y se mece con un seno propio (no van todos iguales).
//!   - BURBUJAS: esferitas aditivas que suben desde la ciudad, se mecen y vuelven a empezar.
//!   - REMOLINO: un parche pegado a la superficie del mar con brazos en espiral que giran.

use super::{body, load_model, on_surface, oriented, quad_both_sides, smoothstep, PaletteShader};
use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
use crate::noise::hash01;
use crate::props::load_prop_lods;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

/// Ciudad: dónde está (dirección en el marco del planeta), su alto y el radio de la cúpula.
const CITY_DIR: Vec3 = Vec3::new(-0.3, 0.75, 0.55);
const CITY_HEIGHT: f32 = 1.6;
const DOME_RADIUS: f32 = 1.75;
/// Colores de la ciudad (mismo orden que en modelar_mundos.py): perla, oro, luz turquesa.
const CITY_PALETTE: [Vec3; 3] = [Vec3::new(0.85, 0.90, 0.95), Vec3::new(0.85, 0.70, 0.35), Vec3::new(0.3, 1.0, 0.9)];
const DOME_COLOR: Vec3 = Vec3::new(0.35, 0.9, 1.0);

/// Peces: cuántos grupos, peces por grupo, tamaño, distancia (radios de Atlantis) y velocidad.
const SCHOOLS: usize = 3;
const FISH_PER_SCHOOL: usize = 10;
const FISH_SIZE: f32 = 0.2;
const FISH_RADII: f32 = 1.15;
const FISH_SPEED: f32 = 0.18;
const FISH_SPREAD: f32 = 0.9; // qué tan separado va cada pez del centro del grupo
const FISH_SEED: u32 = 4545;
/// Plateado, aletas naranjas y ojos negros.
const FISH_PALETTE: [Vec3; 3] = [Vec3::new(0.65, 0.75, 0.85), Vec3::new(1.0, 0.55, 0.15), Vec3::new(0.05, 0.05, 0.06)];

/// Burbujas: cuántas, cada cuánto vuelven a salir, qué tan alto suben y su tamaño.
const BUBBLES: i32 = 24;
const BUBBLE_PERIOD: f32 = 4.0;
const BUBBLE_RISE: f32 = 2.2;
const BUBBLE_SIZE: f32 = 0.09;

/// Remolino: dónde (marco del planeta), su tamaño (radianes sobre la esfera), brazos y velocidad.
const WHIRL_DIR: Vec3 = Vec3::new(0.8, -0.2, -0.55);
const WHIRL_SIZE: f32 = 0.28;
const WHIRL_ARMS: f32 = 4.0;
const WHIRL_SPEED: f32 = 1.5;

pub struct Atlantis {
    home: usize,
    city: Vec<Mesh>,
    dome: Mesh,
    fish: Vec<Mesh>,
    bubble: Mesh,
    whirl: Mesh,
}

impl Atlantis {
    pub fn new(scene: &Scene) -> Option<Atlantis> {
        Some(Atlantis {
            home: body(scene, "Atlantis")?,
            city: load_prop_lods("ciudad_atlantis", CITY_HEIGHT).unwrap_or_else(|| vec![Mesh::icosphere(1, &|_| 1.0)]),
            dome: dome_mesh(10, 32),
            fish: load_model("pez"),
            bubble: Mesh::icosphere(1, &|_| 1.0),
            whirl: whirl_mesh(12, 40),
        })
    }

    fn city_frame(&self, scene: &Scene, t: f32) -> Mat4 {
        on_surface(scene, self.home, CITY_DIR, 0.0, t)
    }

    /// Centro del cardumen `s` en el tiempo t (en el mundo) y la orientación de su plano.
    fn school_point(&self, scene: &Scene, s: usize, t: f32) -> Vec3 {
        let center = scene.body_position(self.home, t);
        let r = FISH_RADII * scene.bodies[self.home].radius;
        let a = FISH_SPEED * t * (1.0 + 0.25 * s as f32) + s as f32 * TAU / SCHOOLS as f32;
        let plane = Mat4::rotation_x(0.9 * s as f32 - 0.6) * Mat4::rotation_z(0.4 * s as f32);
        center + plane.transform_dir(Vec3::new(a.cos() * r, 0.0, a.sin() * r))
    }

    pub fn draw(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let city = PaletteShader { palette: CITY_PALETTE.to_vec(), glow: vec![(0, 0.1), (2, 1.2)], rim: DOME_COLOR, blink: None };
        r.draw_lods(&self.city, &self.city_frame(scene, t), Arc::new(city));

        let center = scene.body_position(self.home, t);
        let shader: Arc<PaletteShader> = Arc::new(PaletteShader { palette: FISH_PALETTE.to_vec(), glow: vec![], rim: DOME_COLOR, blink: None });
        for s in 0..SCHOOLS {
            let head = self.school_point(scene, s, t);
            let ahead = self.school_point(scene, s, t + 0.1) - head;
            let up = (head - center).normalized();
            let side = ahead.cross(up).normalized();
            for k in 0..FISH_PER_SCHOOL {
                // Lugar fijo de cada pez dentro del grupo (hash) + un vaivén propio.
                let h = |n| hash01(s as i32, k as i32, n, FISH_SEED) - 0.5;
                let wobble = (t * 2.0 + k as f32 * 1.3).sin() * 0.15;
                let offset = side * ((h(1) + wobble) * FISH_SPREAD * 2.0) + up * (h(2) * FISH_SPREAD) - ahead.normalized() * (h(3) * FISH_SPREAD * 2.0);
                let model = oriented(head + offset, up, ahead) * Mat4::scale(Vec3::splat(FISH_SIZE));
                r.draw_lods(&self.fish, &model, shader.clone());
            }
        }
    }

    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let frame = self.city_frame(scene, t);
        r.draw_mesh_additive(&self.dome, &(frame * Mat4::scale(Vec3::splat(DOME_RADIUS))), Arc::new(DomeShader));

        // Burbujas: suben desde dentro de la cúpula, cada una con su propio ritmo y lugar.
        for i in 0..BUBBLES {
            let cycle = t / BUBBLE_PERIOD + hash01(i, 0, 0, 77);
            let s = cycle - cycle.floor(); // 0 = sale, 1 = desaparece
            let (a, d) = (hash01(i, 1, 0, 77) * TAU, hash01(i, 2, 0, 77) * 1.2);
            let sway = (t * 3.0 + i as f32).sin() * 0.12;
            let local = Vec3::new(a.cos() * d + sway, s * BUBBLE_RISE + 0.3, a.sin() * d);
            let size = BUBBLE_SIZE * (0.6 + 0.8 * s) * (PI * s).sin().sqrt();
            let model = Mat4::translation(frame.transform_point(local)) * Mat4::scale(Vec3::splat(size.max(0.001)));
            r.draw_mesh_additive(&self.bubble, &model, Arc::new(BubbleShader));
        }

        // Remolino: pegado al mar, gira con el planeta.
        let whirl_model = scene.body_frame(self.home, t) * Mat4::scale(Vec3::splat(scene.bodies[self.home].radius));
        r.draw_mesh_additive(&self.whirl, &whirl_model, Arc::new(WhirlShader));
    }

    /// Dónde está la ciudad (para las capturas).
    pub fn landmark(&self, scene: &Scene, t: f32) -> (Vec3, Vec3) {
        let m = self.city_frame(scene, t);
        (m.transform_point(Vec3::ZERO), m.transform_dir(Vec3::UP).normalized())
    }
}

/// Media esfera de radio 1 (y ≥ 0). uv = (longitud 0..1, latitud 0..1).
fn dome_mesh(rings: usize, segments: usize) -> Mesh {
    let mut m = Mesh::default();
    for i in 0..=rings {
        let lat = i as f32 / rings as f32 * PI / 2.0;
        for j in 0..=segments {
            let lon = j as f32 / segments as f32 * TAU;
            let p = Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
            m.vertices.push(Vertex { position: p, normal: p, uv: (j as f32 / segments as f32, i as f32 / rings as f32), tangent: Vec3::ZERO });
        }
    }
    let row = segments + 1;
    for i in 0..rings {
        for j in 0..segments {
            let (a, b) = ((i * row + j) as u32, (i * row + j + 1) as u32);
            let (c, d) = (((i + 1) * row + j) as u32, ((i + 1) * row + j + 1) as u32);
            m.indices.extend([a, c, d, a, d, b]); // antihorario visto desde afuera
        }
    }
    m.compute_bounds();
    m
}

/// Parche del remolino sobre la esfera de radio 1: anillos alrededor de WHIRL_DIR, apenas
/// encima del mar. uv = (ángulo alrededor 0..1, distancia al centro 0..1).
fn whirl_mesh(rings: usize, segments: usize) -> Mesh {
    let mut m = Mesh::default();
    let c = WHIRL_DIR.normalized();
    let t1 = c.cross(Vec3::UP).normalized();
    let t2 = c.cross(t1);
    let point = |i: usize, j: usize| {
        let r = i as f32 / rings as f32;
        let a = j as f32 / segments as f32 * TAU;
        let d = (c + (t1 * a.cos() + t2 * a.sin()) * (r * WHIRL_SIZE.tan())).normalized() * 1.004;
        (d, (j as f32 / segments as f32, r))
    };
    for i in 0..rings {
        for j in 0..segments {
            quad_both_sides(&mut m, [point(i, j), point(i + 1, j), point(i + 1, j + 1), point(i, j + 1)]);
        }
    }
    m.recompute_normals();
    m.compute_bounds();
    m
}

/// Cúpula: cristal casi invisible al centro y brillante en la orilla (fresnel) + una red de
/// líneas (meridianos y paralelos) que titila con el tiempo.
struct DomeShader;

impl FragmentShader for DomeShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let v = (u.camera_pos - f.world_pos).normalized();
        let fresnel = (1.0 - f.normal.dot(v).abs()).powf(2.0);
        let (lon, lat) = f.uv;
        let line = |x: f32, n: f32| 1.0 - smoothstep(0.0, 0.06, ((x * n).fract() - 0.5).abs() * 2.0 - 0.88).max(0.0) / 0.12;
        let grid = line(lon, 16.0).max(line(lat, 5.0));
        let shimmer = 0.75 + 0.25 * (u.time * 1.5 + lat * 12.0).sin();
        DOME_COLOR * (0.05 + 0.55 * fresnel + 0.25 * grid * shimmer)
    }
}

/// Burbuja: anillo brillante en el borde (como el reflejo de una burbuja de verdad).
struct BubbleShader;

impl FragmentShader for BubbleShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let v = (u.camera_pos - f.world_pos).normalized();
        let ring = (1.0 - f.normal.dot(v).abs()).powf(2.5);
        Vec3::new(0.7, 0.95, 1.0) * (0.08 + 0.8 * ring)
    }
}

/// Remolino: brazos en espiral (el ángulo + el logaritmo de la distancia) que giran con el
/// tiempo, más fuertes hacia el centro y que se desvanecen en la orilla.
struct WhirlShader;

impl FragmentShader for WhirlShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let (a, r) = f.uv;
        let spiral = (a * TAU * WHIRL_ARMS + (r + 0.05).ln() * 6.0 + u.time * WHIRL_SPEED).sin();
        let arms = smoothstep(0.4, 1.0, spiral);
        let fade = (1.0 - r) * smoothstep(0.0, 0.15, r);
        let l = (u.light_pos - f.world_pos).normalized();
        let day = 0.25 + 0.75 * f.normal.dot(l).max(0.0);
        Vec3::new(0.75, 0.95, 1.0) * (arms * fade * day * 0.7)
    }
}

//! SOL: llamaradas y corona (las manchas solares están en SunShader, en shaders.rs).
//!
//!   - LLAMARADAS: arcos de fuego (un tubo en forma de medio círculo, generado aquí) que nacen
//!     en la superficie, crecen, se apagan y vuelven a nacer en otro lugar. Cada llamarada
//!     repite su vida cada FLARE_PERIOD segundos; en cada repetición ("época") un hash elige
//!     dónde sale y hacia dónde se tuerce, así nunca se ve el mismo patrón.
//!   - CORONA: esfera grande alrededor del sol, aditiva, que brilla más cerca del disco y se
//!     desvanece hacia afuera, con rayos de ruido. "Respira": su tamaño y brillo pulsan.

use super::{body, oriented, smoothstep};
use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
use crate::noise::hash01;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

/// Cuántas llamaradas hay a la vez, cada cuánto renace cada una y qué tan grandes son
/// (en radios del sol).
const FLARE_COUNT: i32 = 7;
const FLARE_PERIOD: f32 = 7.0;
const FLARE_SIZE: f32 = 0.32;
/// Grosor del tubo del arco (en fracción del tamaño del arco).
const FLARE_THICKNESS: f32 = 0.12;
const FLARE_BRIGHTNESS: f32 = 0.9;
const FLARE_SEED: u32 = 808;

/// Tamaño de la corona (radios del sol), qué tan rápido se desvanece y cómo respira.
const CORONA_SIZE: f32 = 2.4;
const CORONA_FALLOFF: f32 = 2.2;
const CORONA_BRIGHTNESS: f32 = 0.45;
const CORONA_BREATH_SPEED: f32 = 0.8;
const CORONA_BREATH: f32 = 0.06;

const FIRE_DEEP: Vec3 = Vec3::new(1.0, 0.35, 0.05);
const FIRE_HOT: Vec3 = Vec3::new(1.0, 0.85, 0.45);

pub struct Sol {
    home: usize,
    arc: Mesh,
    shell: Mesh,
}

impl Sol {
    pub fn new(scene: &Scene) -> Option<Sol> {
        Some(Sol { home: body(scene, "Sol")?, arc: arc_mesh(24, 6), shell: Mesh::icosphere(2, &|_| 1.0) })
    }

    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let center = scene.body_position(self.home, t);
        let radius = scene.bodies[self.home].radius;

        // Llamaradas.
        for i in 0..FLARE_COUNT {
            let cycle = t / FLARE_PERIOD + hash01(i, 0, 0, FLARE_SEED) * 9.0;
            let epoch = cycle.floor() as i32;
            let life = cycle - cycle.floor(); // 0 = nace, 1 = se apaga
            let h = |k| hash01(i, epoch, k, FLARE_SEED);
            // Dirección al azar sobre la esfera y hacia dónde se tuerce el arco.
            let y = h(1) * 2.0 - 1.0;
            let a = h(2) * TAU;
            let s = (1.0 - y * y).max(0.0).sqrt();
            let dir = Vec3::new(s * a.cos(), y, s * a.sin());
            let twist = Vec3::new(h(3) - 0.5, h(4) - 0.5, h(5) - 0.5);
            // Crece y se encoge con la vida (escala uniforme: las normales no se deforman).
            let size = radius * FLARE_SIZE * (0.5 + h(6)) * (0.3 + 0.7 * (PI * life).sin());
            let model = oriented(center + dir * (radius * 0.97), dir, twist) * Mat4::scale(Vec3::splat(size));
            r.draw_mesh_additive(&self.arc, &model, Arc::new(FlareShader { life }));
        }

        // Corona.
        let breath = 1.0 + CORONA_BREATH * (t * CORONA_BREATH_SPEED).sin();
        let model = Mat4::translation(center) * Mat4::scale(Vec3::splat(radius * CORONA_SIZE * breath));
        r.draw_mesh_additive(&self.shell, &model, Arc::new(CoronaShader { center, radius, breath }));
    }
}

/// Tubo en forma de medio círculo de radio 1 en el plano XY: sale de (−1, 0, 0), sube hasta
/// (0, 1, 0) y baja a (1, 0, 0). uv.0 = avance por el arco (0 a 1). Con sus dos caras.
fn arc_mesh(segments: usize, sides: usize) -> Mesh {
    let mut m = Mesh::default();
    for i in 0..=segments {
        let th = i as f32 / segments as f32 * PI;
        let c = Vec3::new(-th.cos(), th.sin(), 0.0);
        let out = c; // dirección "hacia afuera" del arco (radial)
        let side = Vec3::new(0.0, 0.0, 1.0);
        // El tubo se adelgaza en las puntas (donde toca la superficie).
        let thick = FLARE_THICKNESS * (0.5 + 0.5 * th.sin());
        for j in 0..sides {
            let ph = j as f32 / sides as f32 * TAU;
            let position = c + (out * ph.cos() + side * ph.sin()) * thick;
            m.vertices.push(Vertex { position, normal: Vec3::ZERO, uv: (i as f32 / segments as f32, 0.0), tangent: Vec3::ZERO });
        }
    }
    for i in 0..segments {
        for j in 0..sides {
            let k = (j + 1) % sides;
            let (a, b) = ((i * sides + j) as u32, (i * sides + k) as u32);
            let (c, d) = (((i + 1) * sides + j) as u32, ((i + 1) * sides + k) as u32);
            m.indices.extend([a, b, d, a, d, c, a, d, b, a, c, d]); // las dos caras
        }
    }
    m.recompute_normals();
    m.compute_bounds();
    m
}

/// Llamarada: fuego naranja en las puntas y casi blanco arriba, que parpadea y se apaga al
/// final de su vida.
struct FlareShader {
    life: f32,
}

impl FragmentShader for FlareShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let top = (f.uv.0 * PI).sin(); // 0 en las puntas, 1 arriba
        let flicker = 0.8 + 0.2 * (f.uv.0 * 25.0 + u.time * 6.0).sin();
        let fade = (PI * self.life).sin().powf(0.7);
        let color = FIRE_DEEP + (FIRE_HOT - FIRE_DEEP) * top;
        color * (FLARE_BRIGHTNESS * fade * flicker)
    }
}

/// Corona: brillo según qué tan lejos del centro del sol se ve el píxel (en radios del sol),
/// medido perpendicular a la línea de vista. Pegado al disco brilla más; afuera se apaga.
struct CoronaShader {
    center: Vec3,
    radius: f32,
    breath: f32,
}

impl FragmentShader for CoronaShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let to_cam = (u.camera_pos - self.center).normalized();
        let rel = f.world_pos - self.center;
        let d = (rel - to_cam * rel.dot(to_cam)).length() / self.radius; // distancia al centro en pantalla
        // Continuo: vale 1 en la orilla del disco, se apaga hacia afuera y también hacia adentro.
        let glow = (-(d - 1.0).max(0.0) * CORONA_FALLOFF).exp() * smoothstep(0.75, 1.0, d);
        // Rayos: ruido según la dirección desde el centro, que cambia despacio.
        // (Con senos y no con ruido: la corona cubre mucha pantalla y así es barata.)
        let q = rel.normalized();
        let rays = 0.8 + 0.35 * ((q.x * 9.0 + u.time * 0.2).sin() * (q.y * 11.0 - u.time * 0.15).sin() * (q.z * 7.0).sin());
        FIRE_DEEP * (glow * rays * CORONA_BRIGHTNESS * self.breath)
    }
}

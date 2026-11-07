//! THEMYSCIRA: templo griego junto a la estatua de la Mujer Maravilla y una capa de nubes
//! (las olas de la costa están en ThemysciraShader, en shaders.rs).
//!
//!   - TEMPLO (assets/props/templo.obj): parado cerca de la cima más alta, donde está la estatua.
//!   - NUBES: una esfera un poco más grande que el planeta, ADITIVA, que gira MÁS LENTO que él
//!     (se ve que las nubes se mueven sobre las islas). Dónde hay nube lo dice un fBm; solo
//!     brillan del lado de día (de noche no se ven, como en la Tierra).

use super::{body, on_surface, smoothstep, PaletteShader};
use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::noise::fbm;
use crate::props::load_prop_lods;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use std::sync::Arc;

/// Alto del templo y qué tan lejos de la cima queda (en radianes sobre el planeta).
const TEMPLE_HEIGHT: f32 = 0.9;
const TEMPLE_FROM_TOP: f32 = 0.25;
/// Colores del templo (mismo orden que en modelar_mundos.py): mármol, oro, techo azul.
const TEMPLE_PALETTE: [Vec3; 3] = [Vec3::new(0.92, 0.90, 0.84), Vec3::new(1.0, 0.78, 0.30), Vec3::new(0.25, 0.40, 0.75)];

/// Nubes: altura de la capa (radios del planeta), velocidad de giro PROPIA (el planeta gira
/// aparte), cuánto cielo cubren y su brillo.
const CLOUD_HEIGHT: f32 = 1.09;
const CLOUD_SPEED: f32 = 0.03;
const CLOUD_COVER: f32 = 0.08; // más alto = menos nubes
const CLOUD_BRIGHTNESS: f32 = 0.55;
const CLOUD_SEED: u32 = 314;

pub struct Themyscira {
    home: usize,
    temple: Vec<Mesh>,
    temple_dir: Vec3,
    shell: Mesh,
}

impl Themyscira {
    pub fn new(scene: &Scene) -> Option<Themyscira> {
        let home = body(scene, "Themyscira")?;
        // La cima: el vértice de la malla más lejos del centro (ahí está la estatua).
        let top = scene.bodies[home].lods[0].vertices.iter().map(|v| v.position).fold(Vec3::ZERO, |best, p| {
            if p.length() > best.length() { p } else { best }
        });
        let up = top.normalized();
        let side = up.cross(Vec3::new(1.0, 0.0, 0.0)).normalized();
        let temple_dir = (up * TEMPLE_FROM_TOP.cos() + side * TEMPLE_FROM_TOP.sin()).normalized();
        let temple = load_prop_lods("templo", TEMPLE_HEIGHT).unwrap_or_else(|| vec![Mesh::icosphere(1, &|_| 1.0)]);
        Some(Themyscira { home, temple, temple_dir, shell: Mesh::icosphere(3, &|_| 1.0) })
    }

    pub fn draw(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let shader = PaletteShader { palette: TEMPLE_PALETTE.to_vec(), glow: vec![(1, 0.15)], rim: Vec3::new(1.0, 0.9, 0.7), blink: None };
        r.draw_lods(&self.temple, &on_surface(scene, self.home, self.temple_dir, 0.0, t), Arc::new(shader));
    }

    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let b = &scene.bodies[self.home];
        // Misma inclinación que el planeta, pero su propio giro.
        let model = Mat4::translation(scene.body_position(self.home, t))
            * Mat4::rotation_z(b.tilt)
            * Mat4::rotation_y(t * CLOUD_SPEED)
            * Mat4::scale(Vec3::splat(b.radius * CLOUD_HEIGHT));
        r.draw_mesh_additive(&self.shell, &model, Arc::new(CloudShader));
    }

    /// Dónde está el templo (para las capturas).
    pub fn landmark(&self, scene: &Scene, t: f32) -> (Vec3, Vec3) {
        let m = on_surface(scene, self.home, self.temple_dir, 0.0, t);
        (m.transform_point(Vec3::ZERO), m.transform_dir(Vec3::UP).normalized())
    }
}

struct CloudShader;

impl FragmentShader for CloudShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let p = f.local_pos.normalized();
        // Dos escalas de ruido: masas grandes de nubes + bordes deshilachados.
        let n = fbm(p * 2.5, 3, CLOUD_SEED) + 0.35 * fbm(p * 9.0, 2, CLOUD_SEED + 1);
        let cover = smoothstep(CLOUD_COVER, CLOUD_COVER + 0.3, n);
        let l = (u.light_pos - f.world_pos).normalized();
        let day = smoothstep(-0.05, 0.3, f.normal.dot(l));
        Vec3::new(1.0, 1.0, 0.98) * (cover * day * CLOUD_BRIGHTNESS)
    }
}

//! CENTRAL CITY: S.T.A.R. Labs y su acelerador de partículas.
//!
//!   - S.T.A.R. LABS (assets/props/star_labs.obj): edificio redondo con domo de vidrio, torre
//!     y la luz roja del logo que parpadea.
//!   - ACELERADOR: un anillo pegado a la superficie de la luna alrededor del laboratorio
//!     (generado aquí, ADITIVO). Por él corren pulsos de energía amarilla, como Flash.

use super::{body, on_surface, quad_both_sides, PaletteShader};
use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::props::load_prop_lods;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use std::f32::consts::TAU;
use std::sync::Arc;

/// Laboratorio: dónde está (marco de la luna) y su alto (la luna tiene radio 1.2).
const LAB_DIR: Vec3 = Vec3::new(0.3, 0.8, 0.5);
const LAB_HEIGHT: f32 = 0.38;
/// Blanco, ventanas, domo de vidrio y luz roja del logo (mismo orden que modelar_mundos.py).
const LAB_PALETTE: [Vec3; 4] = [Vec3::new(0.85, 0.86, 0.88), Vec3::new(0.95, 0.85, 0.55), Vec3::new(0.35, 0.60, 0.95), Vec3::new(1.0, 0.15, 0.1)];

/// Acelerador: radio del anillo (radianes sobre la luna), ancho, altura sobre el suelo,
/// cuántos pulsos corren a la vez y qué tan rápido (vueltas por segundo).
const ACC_RADIUS: f32 = 0.45;
const ACC_WIDTH: f32 = 0.035;
const ACC_HEIGHT: f32 = 1.04;
const ACC_PULSES: f32 = 3.0;
const ACC_SPEED: f32 = 0.6;

pub struct CentralCity {
    home: usize,
    lab: Vec<Mesh>,
    ring: Mesh,
}

impl CentralCity {
    pub fn new(scene: &Scene) -> Option<CentralCity> {
        Some(CentralCity {
            home: body(scene, "Central City")?,
            lab: load_prop_lods("star_labs", LAB_HEIGHT).unwrap_or_else(|| vec![Mesh::icosphere(1, &|_| 1.0)]),
            ring: ring_on_sphere(64),
        })
    }

    pub fn draw(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let shader = PaletteShader { palette: LAB_PALETTE.to_vec(), glow: vec![(1, 0.6), (2, 0.35), (3, 1.5)], rim: Vec3::new(1.0, 0.85, 0.4), blink: Some(3) };
        r.draw_lods(&self.lab, &on_surface(scene, self.home, LAB_DIR, 0.0, t), Arc::new(shader));
    }

    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let model = scene.body_frame(self.home, t) * Mat4::scale(Vec3::splat(scene.bodies[self.home].radius));
        r.draw_mesh_additive(&self.ring, &model, Arc::new(AcceleratorShader));
    }

    /// Dónde está el laboratorio (para las capturas).
    pub fn landmark(&self, scene: &Scene, t: f32) -> (Vec3, Vec3) {
        let m = on_surface(scene, self.home, LAB_DIR, 0.0, t);
        (m.transform_point(Vec3::ZERO), m.transform_dir(Vec3::UP).normalized())
    }
}

/// Anillo delgado sobre la esfera de radio 1, alrededor de LAB_DIR. uv.0 = vuelta (0 a 1).
fn ring_on_sphere(segments: usize) -> Mesh {
    let mut m = Mesh::default();
    let c = LAB_DIR.normalized();
    let t1 = c.cross(Vec3::UP).normalized();
    let t2 = c.cross(t1);
    let point = |j: usize, angle: f32| {
        let a = j as f32 / segments as f32 * TAU;
        let d = (c + (t1 * a.cos() + t2 * a.sin()) * angle.tan()).normalized() * ACC_HEIGHT;
        (d, (j as f32 / segments as f32, 0.0))
    };
    let (inner, outer) = (ACC_RADIUS - ACC_WIDTH * 0.5, ACC_RADIUS + ACC_WIDTH * 0.5);
    for j in 0..segments {
        quad_both_sides(&mut m, [point(j, inner), point(j, outer), point(j + 1, outer), point(j + 1, inner)]);
    }
    m.recompute_normals();
    m.compute_bounds();
    m
}

/// Acelerador: tubo celeste tenue + pulsos amarillos que corren con una cola que se apaga.
struct AcceleratorShader;

impl FragmentShader for AcceleratorShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let s = f.uv.0 * ACC_PULSES - u.time * ACC_SPEED * ACC_PULSES;
        let d = s - s.floor(); // 0 = la punta del pulso, crece hacia la cola
        let pulse = (-(1.0 - d) * 6.0).exp(); // brillante en la punta, cola hacia atrás
        Vec3::new(0.25, 0.55, 0.8) * 0.25 + Vec3::new(1.0, 0.85, 0.3) * (pulse * 1.3)
    }
}

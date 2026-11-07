//! GOTHAM: Torre Wayne, dirigibles, reflectores, niebla nocturna y el ECLIPSE de la luna
//! Central City (la sombra se calcula en GothamShader con `sun_visibility` de renderer.rs).
//!
//!   - TORRE WAYNE (assets/props/torre_wayne.obj): rascacielos con la "W" encendida, cerca
//!     de la azotea de Batman.
//!   - DIRIGIBLES (assets/props/dirigible.obj): dan vueltas despacio sobre la ciudad, con
//!     luces de aviso que parpadean y pantallas de anuncios.
//!   - REFLECTORES: haces de luz (conos aditivos) que salen de la ciudad y barren el cielo.
//!   - NIEBLA: esfera pegada a la ciudad, aditiva, que solo se ve del lado de noche: manchas
//!     de ruido que se mueven, iluminadas por las luces naranjas de la ciudad.

use super::{body, cone_mesh, load_model, on_surface, oriented, smoothstep, BeamShader, PaletteShader};
use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::noise::{fbm, hash01};
use crate::props::load_prop_lods;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use std::f32::consts::TAU;
use std::sync::Arc;

/// Torre Wayne: alto y qué tan lejos de la azotea de Batman (radianes sobre el planeta).
const WAYNE_HEIGHT: f32 = 2.4;
const WAYNE_FROM_BATMAN: f32 = 0.18;
/// Colores (mismo orden que en modelar_mundos.py): vidrio, ventanas, la W.
const WAYNE_PALETTE: [Vec3; 3] = [Vec3::new(0.10, 0.11, 0.14), Vec3::new(1.0, 0.80, 0.40), Vec3::new(1.0, 0.95, 0.75)];

/// Dirigibles: cuántos, tamaño, a qué distancia vuelan (radios de Gotham) y velocidad.
const BLIMP_COUNT: usize = 3;
const BLIMP_SIZE: f32 = 0.8;
const BLIMP_RADII: f32 = 1.3;
const BLIMP_SPEED: f32 = 0.08;
/// Casco, partes oscuras, luces de aviso (parpadean) y pantallas.
const BLIMP_PALETTE: [Vec3; 4] = [Vec3::new(0.55, 0.56, 0.60), Vec3::new(0.12, 0.12, 0.14), Vec3::new(1.0, 0.2, 0.1), Vec3::new(0.3, 0.9, 1.0)];

/// Reflectores: cuántos, semilla de sus lugares, largo y ancho del haz, inclinación y velocidad
/// con que barren el cielo.
const SEARCHLIGHT_COUNT: i32 = 5;
const SEARCHLIGHT_SEED: u32 = 3333;
const SEARCHLIGHT_LENGTH: f32 = 5.0;
const SEARCHLIGHT_WIDTH: f32 = 0.3;
const SEARCHLIGHT_TILT: f32 = 0.45;
const SEARCHLIGHT_SPEED: f32 = 0.5;

/// Niebla: altura de la capa, brillo y color (luces de la ciudad reflejadas en la niebla).
const FOG_HEIGHT: f32 = 1.035;
const FOG_BRIGHTNESS: f32 = 0.22;
const FOG_COLOR: Vec3 = Vec3::new(0.85, 0.55, 0.30);

pub struct Gotham {
    home: usize,
    wayne: Vec<Mesh>,
    wayne_dir: Vec3,
    blimp: Vec<Mesh>,
    searchlights: Vec<Vec3>,
    cone: Mesh,
    shell: Mesh,
}

/// El eclipse: la luna Central City (centro y radio) como esfera que puede tapar el sol.
pub fn eclipse(scene: &Scene, t: f32) -> (Vec3, f32) {
    match body(scene, "Central City") {
        Some(i) => (scene.body_position(i, t), scene.bodies[i].radius),
        None => (Vec3::ZERO, 0.0),
    }
}

impl Gotham {
    pub fn new(scene: &Scene) -> Option<Gotham> {
        let home = body(scene, "Gotham")?;
        // Batman está en el punto más alto de la malla; la torre va a un lado.
        let top = scene.bodies[home].lods[0].vertices.iter().map(|v| v.position).fold(Vec3::ZERO, |best, p| {
            if p.length() > best.length() { p } else { best }
        });
        let up = top.normalized();
        let side = up.cross(Vec3::new(0.0, 0.0, 1.0)).normalized();
        let wayne_dir = (up * WAYNE_FROM_BATMAN.cos() + side * WAYNE_FROM_BATMAN.sin()).normalized();
        let searchlights = (0..SEARCHLIGHT_COUNT)
            .map(|i| {
                let y = hash01(i, 1, 0, SEARCHLIGHT_SEED) * 1.6 - 0.8;
                let a = hash01(i, 2, 0, SEARCHLIGHT_SEED) * TAU;
                let s = (1.0 - y * y).sqrt();
                Vec3::new(s * a.cos(), y, s * a.sin())
            })
            .collect();
        Some(Gotham {
            home,
            wayne: load_prop_lods("torre_wayne", WAYNE_HEIGHT).unwrap_or_else(|| vec![Mesh::icosphere(1, &|_| 1.0)]),
            wayne_dir,
            blimp: load_model("dirigible"),
            searchlights,
            cone: cone_mesh(16, 0.08),
            shell: Mesh::icosphere(3, &|_| 1.0),
        })
    }

    /// Posición del dirigible `i` en el tiempo t, en el mundo.
    fn blimp_point(&self, scene: &Scene, i: usize, t: f32) -> Vec3 {
        let center = scene.body_position(self.home, t);
        let r = BLIMP_RADII * scene.bodies[self.home].radius;
        let a = BLIMP_SPEED * t * (1.0 + 0.3 * i as f32) + i as f32 * TAU / BLIMP_COUNT as f32;
        let plane = Mat4::rotation_x(0.5 * i as f32 - 0.4) * Mat4::rotation_z(0.3 * i as f32);
        center + plane.transform_dir(Vec3::new(a.cos() * r, 0.0, a.sin() * r))
    }

    pub fn draw(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let wayne = PaletteShader { palette: WAYNE_PALETTE.to_vec(), glow: vec![(1, 0.9), (2, 1.8)], rim: Vec3::new(0.6, 0.5, 0.4), blink: None };
        r.draw_lods(&self.wayne, &on_surface(scene, self.home, self.wayne_dir, 0.0, t), Arc::new(wayne));

        let center = scene.body_position(self.home, t);
        let shader: Arc<PaletteShader> =
            Arc::new(PaletteShader { palette: BLIMP_PALETTE.to_vec(), glow: vec![(2, 1.5), (3, 0.9)], rim: Vec3::new(0.5, 0.5, 0.6), blink: Some(2) });
        for i in 0..BLIMP_COUNT {
            let pos = self.blimp_point(scene, i, t);
            let ahead = self.blimp_point(scene, i, t + 0.1);
            let model = oriented(pos, pos - center, ahead - pos) * Mat4::scale(Vec3::splat(BLIMP_SIZE));
            r.draw_lods(&self.blimp, &model, shader.clone());
        }
    }

    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        // Reflectores: cada uno gira alrededor de su vertical con su propio desfase.
        for (i, dir) in self.searchlights.iter().enumerate() {
            let base = on_surface(scene, self.home, *dir, 0.0, t);
            let origin = base.transform_point(Vec3::ZERO);
            let up = base.transform_dir(Vec3::UP).normalized();
            let x = base.transform_dir(Vec3::new(1.0, 0.0, 0.0)).normalized();
            let z = up.cross(x);
            let a = t * SEARCHLIGHT_SPEED * (1.0 + 0.2 * i as f32) + i as f32 * 1.7;
            let beam = (up * SEARCHLIGHT_TILT.cos() + (x * a.cos() + z * a.sin()) * SEARCHLIGHT_TILT.sin()).normalized();
            let model = oriented(origin, beam, x) * Mat4::scale(Vec3::new(SEARCHLIGHT_WIDTH, SEARCHLIGHT_LENGTH, SEARCHLIGHT_WIDTH));
            let shader = BeamShader {
                origin,
                dir: beam,
                length: SEARCHLIGHT_LENGTH,
                base_color: Vec3::new(0.9, 0.95, 1.0),
                tip_color: Vec3::new(0.5, 0.6, 0.8),
                brightness: 0.35,
            };
            r.draw_mesh_additive(&self.cone, &model, Arc::new(shader));
        }

        // Niebla: gira con la ciudad.
        let model = scene.body_frame(self.home, t) * Mat4::scale(Vec3::splat(scene.bodies[self.home].radius * FOG_HEIGHT));
        r.draw_mesh_additive(&self.shell, &model, Arc::new(FogShader));
    }

    /// Dónde está la Torre Wayne (para las capturas).
    pub fn landmark(&self, scene: &Scene, t: f32) -> (Vec3, Vec3) {
        let m = on_surface(scene, self.home, self.wayne_dir, 0.0, t);
        (m.transform_point(Vec3::ZERO), m.transform_dir(Vec3::UP).normalized())
    }
}

struct FogShader;

impl FragmentShader for FogShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let l = (u.light_pos - f.world_pos).normalized();
        let night = 1.0 - smoothstep(-0.1, 0.15, f.normal.dot(l));
        if night <= 0.0 {
            return Vec3::ZERO;
        }
        let p = f.local_pos.normalized();
        let drift = Vec3::new(u.time * 0.03, 0.0, u.time * 0.02);
        let patches = smoothstep(-0.05, 0.4, fbm(p * 7.0 + drift, 2, 99));
        FOG_COLOR * (patches * night * FOG_BRIGHTNESS)
    }
}

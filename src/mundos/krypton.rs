//! KRYPTON: la Fortaleza de la Soledad y géiseres de lava (el pulso de las grietas rojas está
//! en KryptonShader, en shaders.rs).
//!
//!   - FORTALEZA (assets/props/fortaleza.obj): racimo de agujas de cristal en el polo norte.
//!   - GÉISERES: chorros de lava (conos ADITIVOS) en lugares fijos elegidos con una semilla.
//!     Cada uno erupciona cada GEYSER_PERIOD segundos: el chorro sube, se sostiene y baja.

use super::{body, cone_mesh, on_surface, BeamShader, PaletteShader};
use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::noise::hash01;
use crate::props::load_prop_lods;
use crate::renderer::Renderer;
use crate::scene::Scene;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

/// Alto de la Fortaleza (Krypton tiene radio 5) y dónde está (polo norte del planeta).
const FORTRESS_HEIGHT: f32 = 2.2;
const FORTRESS_DIR: Vec3 = Vec3::new(0.0, 1.0, 0.0);
/// Colores (mismo orden que en modelar_mundos.py): hielo, base, núcleo que brilla.
const FORTRESS_PALETTE: [Vec3; 3] = [Vec3::new(0.75, 0.90, 1.0), Vec3::new(0.30, 0.45, 0.60), Vec3::new(0.40, 0.95, 1.0)];

/// Géiseres: cuántos, semilla de sus lugares, cada cuánto erupcionan, cuánto dura la erupción
/// (fracción del periodo), alto y ancho máximo del chorro.
const GEYSER_COUNT: i32 = 7;
const GEYSER_SEED: u32 = 2121;
const GEYSER_PERIOD: f32 = 5.0;
const GEYSER_ACTIVE: f32 = 0.4;
const GEYSER_HEIGHT: f32 = 2.6;
const GEYSER_WIDTH: f32 = 0.35;
const LAVA_HOT: Vec3 = Vec3::new(1.0, 0.85, 0.4);
const LAVA_DEEP: Vec3 = Vec3::new(1.0, 0.2, 0.05);

pub struct Krypton {
    home: usize,
    fortress: Vec<Mesh>,
    geysers: Vec<Vec3>,
    cone: Mesh,
}

impl Krypton {
    pub fn new(scene: &Scene) -> Option<Krypton> {
        let home = body(scene, "Krypton")?;
        let fortress = load_prop_lods("fortaleza", FORTRESS_HEIGHT).unwrap_or_else(|| vec![Mesh::icosphere(1, &|_| 1.0)]);
        // Lugares de los géiseres: direcciones al azar con la semilla (lejos del polo norte).
        let geysers = (0..GEYSER_COUNT)
            .map(|i| {
                let y = hash01(i, 1, 0, GEYSER_SEED) * 1.4 - 0.8;
                let a = hash01(i, 2, 0, GEYSER_SEED) * TAU;
                let s = (1.0 - y * y).sqrt();
                Vec3::new(s * a.cos(), y, s * a.sin())
            })
            .collect();
        Some(Krypton { home, fortress, geysers, cone: cone_mesh(16, 0.35) })
    }

    pub fn draw(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let shader = PaletteShader { palette: FORTRESS_PALETTE.to_vec(), glow: vec![(0, 0.2), (2, 0.9)], rim: Vec3::new(0.5, 0.9, 1.0) };
        r.draw_lods(&self.fortress, &on_surface(scene, self.home, FORTRESS_DIR, 0.0, t), Arc::new(shader));
    }

    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        for (i, dir) in self.geysers.iter().enumerate() {
            // ¿En qué parte de su ciclo va? (cada géiser con su propio desfase)
            let cycle = t / GEYSER_PERIOD + hash01(i as i32, 3, 0, GEYSER_SEED);
            let s = (cycle - cycle.floor()) / GEYSER_ACTIVE;
            if s >= 1.0 {
                continue; // dormido
            }
            let height = GEYSER_HEIGHT * (PI * s).sin().powf(0.6); // sube rápido, baja despacio
            let base = on_surface(scene, self.home, *dir, 0.0, t);
            let origin = base.transform_point(Vec3::ZERO);
            let up = base.transform_dir(Vec3::UP).normalized();
            let model = base * Mat4::scale(Vec3::new(GEYSER_WIDTH, height.max(0.01), GEYSER_WIDTH));
            let shader = BeamShader { origin, dir: up, length: height.max(0.01), base_color: LAVA_HOT, tip_color: LAVA_DEEP, brightness: 1.1 };
            r.draw_mesh_additive(&self.cone, &model, Arc::new(shader));
        }
    }

    /// Dónde está la Fortaleza (para las capturas).
    pub fn landmark(&self, scene: &Scene, t: f32) -> (Vec3, Vec3) {
        let m = on_surface(scene, self.home, FORTRESS_DIR, 0.0, t);
        (m.transform_point(Vec3::ZERO), m.transform_dir(Vec3::UP).normalized())
    }
}

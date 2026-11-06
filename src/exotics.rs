//! OBJETOS EXÓTICOS del sistema (Diego, D7): el cinturón de asteroides.
//!
//! El cinturón es la prueba de los contadores: son muchísimos objetos pequeños, así que con
//! el culling (C) y el LOD (L) apagados los triángulos que entran al pipeline suben mucho.

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::noise::{fbm, hash01};
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::shaders::lambert;
use std::sync::Arc;

// --- Cinturón de asteroides: entre Krypton (órbita 70) y Gotham (órbita 100) ---
/// Semilla del cinturón: la misma semilla siempre da las mismas rocas en los mismos lugares.
pub const ASTEROID_SEED: u32 = 2026;
/// Cuántas rocas hay.
pub const ASTEROID_COUNT: usize = 1500;
/// Radio medio del cinturón y cuánto se aleja cada roca de él (hacia adentro o afuera).
const BELT_RADIUS: f32 = 85.0;
const BELT_WIDTH: f32 = 8.0;
/// Cuánto se sale cada roca del plano de las órbitas (arriba o abajo).
const BELT_THICKNESS: f32 = 2.5;
/// Tamaño de las rocas (la mayoría salen pequeñas).
const ASTEROID_MIN_SIZE: f32 = 0.25;
const ASTEROID_MAX_SIZE: f32 = 1.1;
/// Radianes por segundo alrededor del sol en el centro del cinturón.
/// Ley de Kepler: las de adentro van más rápido (ω ∝ r^-1.5).
const BELT_SPEED: f32 = 0.04;
/// Cuántas formas distintas de roca se generan (todas comparten las mallas de su forma).
const ASTEROID_SHAPES: u32 = 4;
/// Subdivisiones de la icosfera de cada LOD: 320, 80 y 20 triángulos.
const ASTEROID_LOD_SUBDIV: [u32; 3] = [2, 1, 0];
const ASTEROID_DARK: Vec3 = Vec3::new(0.18, 0.16, 0.14);
const ASTEROID_LIGHT: Vec3 = Vec3::new(0.47, 0.42, 0.36);
/// Luz ambiente de las rocas: sin ella, las que están a contraluz se ven negras.
const ASTEROID_AMBIENT: f32 = 0.12;

/// Una roca del cinturón: todo lo necesario para saber dónde está en cualquier tiempo t.
struct Asteroid {
    radius: f32,
    angle0: f32,
    height: f32,
    size: f32,
    /// Radianes por segundo alrededor del sol.
    speed: f32,
    /// Giro sobre sí misma (radianes por segundo) e inclinación de ese eje.
    spin: f32,
    tilt: f32,
    shape: usize,
}

/// Forma de "papa" de una roca: la icosfera se deforma con dos capas de ruido.
fn rock_shape(dir: Vec3, seed: u32) -> f32 {
    1.0 + 0.35 * fbm(dir * 1.6, 3, seed) + 0.12 * fbm(dir * 5.0, 2, seed + 7)
}

pub struct AsteroidBelt {
    rocks: Vec<Asteroid>,
    /// shapes[forma][lod]
    shapes: Vec<Vec<Mesh>>,
    shader: Arc<dyn FragmentShader>,
}

impl AsteroidBelt {
    pub fn new(seed: u32) -> AsteroidBelt {
        // Mallas: cada forma tiene su propio ruido y sus 3 niveles de detalle.
        let shapes = (0..ASTEROID_SHAPES)
            .map(|s| {
                ASTEROID_LOD_SUBDIV
                    .iter()
                    .map(|&sub| Mesh::icosphere(sub, &|d| rock_shape(d, seed + s * 101)))
                    .collect()
            })
            .collect();

        // Rocas: cada una saca sus números del hash con su índice y la semilla (nada al azar real).
        let rocks = (0..ASTEROID_COUNT)
            .map(|i| {
                let h = |k| hash01(i as i32, k, 0, seed);
                // Sumar dos números del hash amontona las rocas hacia el centro del cinturón.
                let radius = BELT_RADIUS + (h(1) + h(2) - 1.0) * BELT_WIDTH;
                Asteroid {
                    radius,
                    angle0: h(3) * std::f32::consts::TAU,
                    height: (h(4) + h(5) - 1.0) * BELT_THICKNESS,
                    size: ASTEROID_MIN_SIZE + (ASTEROID_MAX_SIZE - ASTEROID_MIN_SIZE) * h(6).powi(3),
                    speed: BELT_SPEED * (BELT_RADIUS / radius).powf(1.5),
                    spin: (h(7) * 2.0 - 1.0) * 1.5,
                    tilt: h(8) * std::f32::consts::PI,
                    shape: (h(9) * ASTEROID_SHAPES as f32) as usize % ASTEROID_SHAPES as usize,
                }
            })
            .collect();

        AsteroidBelt { rocks, shapes, shader: Arc::new(AsteroidShader { seed }) }
    }

    /// Matriz de modelo de una roca: Traslación(órbita) * Giro * Inclinación * Escala.
    fn model(rock: &Asteroid, t: f32) -> Mat4 {
        let a = rock.angle0 + rock.speed * t;
        let pos = Vec3::new(a.cos() * rock.radius, rock.height, a.sin() * rock.radius);
        Mat4::translation(pos)
            * Mat4::rotation_y(rock.spin * t)
            * Mat4::rotation_x(rock.tilt)
            * Mat4::scale(Vec3::splat(rock.size))
    }

    /// Dibuja todas las rocas con draw_lods: culling por roca y LOD según su tamaño en pantalla.
    pub fn draw(&self, r: &mut Renderer, t: f32) {
        for rock in &self.rocks {
            r.draw_lods(&self.shapes[rock.shape], &Self::model(rock, t), self.shader.clone());
        }
    }
}

/// Roca gris-café con manchas de ruido e iluminada con Lambert.
struct AsteroidShader {
    seed: u32,
}

impl FragmentShader for AsteroidShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let p = f.local_pos.normalized(); // gira con la roca
        let n = (fbm(p * 4.0, 3, self.seed) * 0.5 + 0.5).clamp(0.0, 1.0);
        let base = ASTEROID_DARK.lerp(ASTEROID_LIGHT, n);
        base * lambert(f, u, ASTEROID_AMBIENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misma_semilla_mismas_rocas() {
        let (a, b) = (AsteroidBelt::new(ASTEROID_SEED), AsteroidBelt::new(ASTEROID_SEED));
        let c = AsteroidBelt::new(ASTEROID_SEED + 1);
        let pos = |belt: &AsteroidBelt, i: usize| AsteroidBelt::model(&belt.rocks[i], 0.0).transform_point(Vec3::ZERO);
        assert_eq!(a.rocks.len(), ASTEROID_COUNT);
        assert!((pos(&a, 10) - pos(&b, 10)).length() < 1e-5);
        assert!((pos(&a, 10) - pos(&c, 10)).length() > 1e-3);
    }

    #[test]
    fn las_rocas_quedan_entre_krypton_y_gotham() {
        let belt = AsteroidBelt::new(ASTEROID_SEED);
        for rock in &belt.rocks {
            assert!(rock.radius > 70.0 + 2.0 && rock.radius < 100.0 - 2.0);
        }
    }
}

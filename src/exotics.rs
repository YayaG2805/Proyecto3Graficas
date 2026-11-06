//! OBJETOS EXÓTICOS del sistema (Diego, D7): el cinturón de asteroides y el cometa.
//!
//! El cinturón es la prueba de los contadores: son muchísimos objetos pequeños, así que con
//! el culling (C) y el LOD (L) apagados los triángulos que entran al pipeline suben mucho.
//! El cometa sigue una órbita elíptica de Kepler y su cola siempre apunta lejos del sol.

use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
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

// ---------------------------------------------------------------------------
// COMETA: órbita ELÍPTICA de Kepler con el sol en un foco.
//   M = M0 + n·t           (anomalía media: crece parejo con el tiempo, n = 2π / periodo)
//   E − e·sen(E) = M        (ecuación de Kepler: se resuelve con el método de Newton)
//   x = a·(cos E − e),  z = b·sen E,  con b = a·√(1 − e²)
// Así el cometa va lento lejos del sol y muy rápido cuando pasa cerca (segunda ley de Kepler).
// ---------------------------------------------------------------------------
/// Semieje mayor de la elipse (qué tan grande es la órbita).
const COMET_A: f32 = 120.0;
/// Excentricidad: 0 = círculo, cerca de 1 = elipse muy alargada (perihelio 30, afelio 210).
const COMET_E: f32 = 0.75;
/// Segundos que tarda en dar una vuelta completa.
const COMET_PERIOD: f32 = 240.0;
/// Anomalía media al inicio: un poco antes del perihelio, para verlo acercarse al sol.
const COMET_M0: f32 = -0.6;
/// Inclinación del plano de la órbita y hacia dónde apunta su eje mayor (radianes).
const COMET_INCLINATION: f32 = 0.35;
const COMET_ORIENTATION: f32 = 2.0;
/// Radio del núcleo de hielo y del halo (coma) que lo rodea.
const COMET_NUCLEUS: f32 = 0.8;
const COMET_COMA: f32 = 2.6;
/// Largo y ancho de la cola a la distancia de referencia; cerca del sol crece.
const COMET_TAIL_LENGTH: f32 = 30.0;
const COMET_TAIL_WIDTH: f32 = 2.2;
const COMET_TAIL_REF_DIST: f32 = 60.0;
/// Brillo de la cola (es aditiva: se suma a lo que haya detrás).
const COMET_TAIL_BRIGHTNESS: f32 = 1.3;
const COMET_ICE: Vec3 = Vec3::new(0.70, 0.80, 0.90);
const COMET_GLOW_CORE: Vec3 = Vec3::new(0.85, 0.95, 1.0);
const COMET_GLOW_TAIL: Vec3 = Vec3::new(0.25, 0.55, 1.0);

/// Malla de la cola: un cono que se ENSANCHA hacia +Z (lejos del núcleo), abierto al final.
/// uv = (vuelta alrededor, de 0 a 1; avance a lo largo, de 0 a 1).
fn tail_mesh(segments: u32) -> Mesh {
    // (avance z, radio relativo)
    let rings = [(0.0, 0.25), (0.08, 0.55), (0.3, 0.8), (0.65, 0.95), (1.0, 1.05)];
    let n = segments as usize;
    let mut mesh = Mesh::default();
    for &(z, r) in &rings {
        for j in 0..n {
            let a = j as f32 / n as f32 * std::f32::consts::TAU;
            mesh.vertices.push(Vertex {
                position: Vec3::new(a.cos() * r, a.sin() * r, z),
                normal: Vec3::ZERO,
                uv: (j as f32 / n as f32, z),
                tangent: Vec3::ZERO,
            });
        }
    }
    for i in 0..rings.len() - 1 {
        for j in 0..n {
            let k = (j + 1) % n;
            let (p00, p01) = ((i * n + j) as u32, (i * n + k) as u32);
            let (p10, p11) = (((i + 1) * n + j) as u32, ((i + 1) * n + k) as u32);
            mesh.indices.extend([p00, p01, p11, p00, p11, p10]);
        }
    }
    mesh.recompute_normals();
    mesh.compute_bounds();
    mesh
}

pub struct Comet {
    nucleus: Vec<Mesh>,
    coma: Mesh,
    tail: Mesh,
}

impl Comet {
    pub fn new() -> Comet {
        Comet {
            nucleus: [3, 2, 1].iter().map(|&s| Mesh::icosphere(s, &|d| rock_shape(d, 77))).collect(),
            coma: Mesh::icosphere(2, &|_| 1.0),
            tail: tail_mesh(24),
        }
    }

    /// Posición del cometa en el tiempo t (el sol está en el origen, en un foco de la elipse).
    pub fn position(&self, t: f32) -> Vec3 {
        let n = std::f32::consts::TAU / COMET_PERIOD;
        let m = COMET_M0 + n * t;
        // Newton: E ← E − (E − e·sen E − M) / (1 − e·cos E). Converge en pocas vueltas.
        let mut e_anom = m;
        for _ in 0..6 {
            e_anom -= (e_anom - COMET_E * e_anom.sin() - m) / (1.0 - COMET_E * e_anom.cos());
        }
        let b = COMET_A * (1.0 - COMET_E * COMET_E).sqrt();
        let p = Vec3::new(COMET_A * (e_anom.cos() - COMET_E), 0.0, b * e_anom.sin());
        // Inclinar el plano de la órbita y girarlo para que no quede alineado con los planetas.
        (Mat4::rotation_y(COMET_ORIENTATION) * Mat4::rotation_x(COMET_INCLINATION)).transform_point(p)
    }

    /// Qué tan activo está: 1 a la distancia de referencia, más cerca del sol más brillante.
    fn activity(&self, pos: Vec3) -> f32 {
        (COMET_TAIL_REF_DIST / pos.length().max(1.0)).clamp(0.7, 2.0)
    }

    /// El núcleo (opaco): se dibuja con los demás objetos sólidos.
    pub fn draw_nucleus(&self, r: &mut Renderer, t: f32) {
        let model = Mat4::translation(self.position(t)) * Mat4::rotation_y(t * 0.5) * Mat4::scale(Vec3::splat(COMET_NUCLEUS));
        r.draw_lods(&self.nucleus, &model, Arc::new(CometIceShader));
    }

    /// La coma y la cola (aditivas): se dibujan DESPUÉS de todo lo opaco.
    pub fn draw_glow(&self, r: &mut Renderer, t: f32) {
        let pos = self.position(t);
        let activity = self.activity(pos);

        // Base de la cola: su +Z local apunta LEJOS del sol (from_basis usa -Z como "adelante").
        let away = pos.normalized();
        let toward_sun = -away;
        let side = if toward_sun.cross(Vec3::UP).length() > 1e-3 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
        let right = toward_sun.cross(side).normalized();
        let up = right.cross(toward_sun);
        let width = COMET_TAIL_WIDTH * activity.sqrt();
        let length = COMET_TAIL_LENGTH * activity;
        let tail = Mat4::from_basis(right, up, toward_sun, pos) * Mat4::scale(Vec3::new(width, width, length));
        r.draw_mesh_additive(&self.tail, &tail, Arc::new(CometTailShader { activity, right, up }));

        let coma = Mat4::translation(pos) * Mat4::scale(Vec3::splat(COMET_COMA));
        r.draw_mesh_additive(&self.coma, &coma, Arc::new(CometComaShader { activity }));
    }
}

/// Núcleo de hielo sucio: Lambert con un poco de brillo especular.
struct CometIceShader;

impl FragmentShader for CometIceShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let n = (fbm(f.local_pos * 3.0, 3, 77) * 0.5 + 0.5).clamp(0.0, 1.0);
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let spec = f.normal.dot((l + v).normalized()).max(0.0).powf(30.0);
        COMET_ICE * (0.6 + 0.4 * n) * lambert(f, u, 0.15) + Vec3::splat(spec * 0.4)
    }
}

/// Halo alrededor del núcleo: brilla donde la esfera mira a la cámara (el centro)
/// y se desvanece hacia la orilla, así no se ve el borde de la esfera.
struct CometComaShader {
    activity: f32,
}

impl FragmentShader for CometComaShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let v = (u.camera_pos - f.world_pos).normalized();
        let facing = f.normal.dot(v).abs();
        COMET_GLOW_CORE * (facing.powf(4.0) * 0.35 * self.activity)
    }
}

/// Cola: más brillante junto al núcleo, se apaga hacia el final y tiene estrías de gas
/// que se alejan del sol con el tiempo (animada).
/// OJO: la cola se escala distinto en largo que en ancho, y así la normal interpolada sale
/// torcida (habría que usar la inversa transpuesta). Por eso la normal se reconstruye aquí:
/// en un cono, la normal apunta hacia afuera desde el eje = (x, y) local en la base (right, up).
struct CometTailShader {
    activity: f32,
    right: Vec3,
    up: Vec3,
}

impl FragmentShader for CometTailShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let along = f.uv.1.clamp(0.0, 1.0);
        let v = (u.camera_pos - f.world_pos).normalized();
        let n = (self.right * f.local_pos.x + self.up * f.local_pos.y).normalized();
        let facing = n.dot(v).abs().powf(1.2);
        let fade = (1.0 - along).powf(1.6);
        let streaks = 0.6 + 0.6 * fbm(Vec3::new(f.uv.0 * 10.0, along * 3.0 - u.time * 0.4, 0.0), 2, 91).max(0.0);
        let color = COMET_GLOW_CORE.lerp(COMET_GLOW_TAIL, (along * 2.0).min(1.0));
        color * (facing * fade * streaks * COMET_TAIL_BRIGHTNESS * self.activity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_cometa_sigue_una_elipse_con_el_sol_en_el_foco() {
        let comet = Comet::new();
        // A lo largo de una vuelta, su distancia al sol va del perihelio a(1-e) al afelio a(1+e).
        let (mut near, mut far) = (f32::MAX, 0.0f32);
        for k in 0..2400 {
            let d = comet.position(k as f32 * COMET_PERIOD / 2400.0).length();
            near = near.min(d);
            far = far.max(d);
        }
        assert!((near - COMET_A * (1.0 - COMET_E)).abs() < 0.5);
        assert!((far - COMET_A * (1.0 + COMET_E)).abs() < 0.5);
        // Y después de un periodo vuelve al mismo lugar.
        assert!((comet.position(0.0) - comet.position(COMET_PERIOD)).length() < 0.01);
    }

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

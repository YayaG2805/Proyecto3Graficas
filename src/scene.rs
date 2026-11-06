//! Escena: sol y planetas (cuerpos celestes). Aquí se configuran radios, órbitas,
//! velocidades y shaders. Todo movimiento depende del TIEMPO, no de los cuadros.

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::noise::fbm;
use crate::normal_map::NormalMap;
use crate::renderer::{FragmentShader, Renderer};
use crate::shaders::{
    gotham_height, themyscira_elevation, GothamShader, KryptonShader, OrbitShader, SunShader, ThemysciraShader,
};
use std::sync::Arc;

/// Cuántos segmentos de línea forman cada órbita (más = círculo más redondo).
const ORBIT_SEGMENTS: usize = 128;

// --- Themyscira (los umbrales de color y el nivel del mar están en shaders.rs) ---
/// Semilla del terreno: cambiarla mueve las islas.
const THEMYSCIRA_SEED: u32 = 7;
/// Qué tanto suben las islas sobre el mar, en fracción del radio (0.06 = 6%).
const THEMYSCIRA_RELIEF: f32 = 0.06;

// --- Krypton (tamaño de cristales, venas y colores están en shaders.rs) ---
/// Semilla de las crestas y de los cristales.
const KRYPTON_SEED: u32 = 21;
/// Altura de las crestas de cristal, en fracción del radio (0.08 = 8%).
const KRYPTON_RELIEF: f32 = 0.08;
/// Frecuencia de las cordilleras (más alto = más crestas y más juntas).
const KRYPTON_RIDGE_FREQ: f32 = 2.5;
/// Qué tan afiladas son las crestas (exponente: más alto = más finas y puntiagudas).
const KRYPTON_RIDGE_SHARPNESS: f32 = 3.0;

/// Ruido RIDGED para Krypton: 1 - |fbm| vale 1 justo donde el ruido cruza por 0, así que
/// se forman "líneas" de crestas afiladas en vez de colinas redondas.
fn krypton_ridge(dir: Vec3) -> f32 {
    let ridge = (1.0 - fbm(dir * KRYPTON_RIDGE_FREQ, 4, KRYPTON_SEED).abs() * 2.0).max(0.0);
    ridge.powf(KRYPTON_RIDGE_SHARPNESS)
}

// --- Gotham (manzanas, ventanas y colores están en shaders.rs) ---
/// Semilla de la ciudad: cambiarla cambia alturas, tonos y ventanas.
const GOTHAM_SEED: u32 = 33;
/// Altura de la torre más alta, en fracción del radio (0.035 = 3.5%).
const GOTHAM_BUILDING_HEIGHT: f32 = 0.035;

pub struct Body {
    pub name: &'static str,
    pub radius: f32,
    /// Distancia al sol (0 = el sol).
    pub orbit_radius: f32,
    /// Radianes por segundo alrededor del sol.
    pub orbit_speed: f32,
    /// Ángulo inicial en la órbita.
    pub orbit_phase: f32,
    /// Radianes por segundo sobre su propio eje.
    pub spin_speed: f32,
    /// Inclinación del eje (radianes).
    pub tilt: f32,
    /// Niveles de detalle: [0] = más detallado. Se elige según la distancia.
    pub lods: Vec<Mesh>,
    pub shader: Arc<dyn FragmentShader>,
}

impl Body {
    /// Centro del cuerpo en el tiempo `t` (órbita circular en el plano XZ).
    pub fn position(&self, t: f32) -> Vec3 {
        self.orbit_point(self.orbit_phase + self.orbit_speed * t)
    }

    /// Punto de la órbita en el ángulo `a` (radianes).
    fn orbit_point(&self, a: f32) -> Vec3 {
        Vec3::new(a.cos() * self.orbit_radius, 0.0, a.sin() * self.orbit_radius)
    }

    /// La órbita completa como puntos: la misma fórmula de `position`, dando toda la vuelta.
    pub fn orbit_points(&self) -> Vec<Vec3> {
        (0..ORBIT_SEGMENTS)
            .map(|i| self.orbit_point(i as f32 / ORBIT_SEGMENTS as f32 * std::f32::consts::TAU))
            .collect()
    }

    /// Model = Traslación * Inclinación * Rotación propia * Escala
    pub fn model_matrix(&self, t: f32) -> Mat4 {
        Mat4::translation(self.position(t))
            * Mat4::rotation_z(self.tilt)
            * Mat4::rotation_y(self.spin_speed * t)
            * Mat4::scale(Vec3::splat(self.radius))
    }

    /// LOD: según qué tan grande se ve (radio / distancia), elegimos una malla.
    pub fn pick_lod(&self, camera_pos: Vec3, t: f32) -> usize {
        let dist = (self.position(t) - camera_pos).length().max(0.001);
        let apparent = self.radius / dist; // ~ tamaño en pantalla
        let level = if apparent > 0.5 { 0 } else if apparent > 0.15 { 1 } else if apparent > 0.05 { 2 } else { 3 };
        level.min(self.lods.len() - 1)
    }
}

/// Genera las mallas de LOD de una esfera (opcionalmente deformada).
/// subdivisiones 5,4,3,2 → 20480, 5120, 1280, 320 triángulos.
fn sphere_lods(displace: &dyn Fn(Vec3) -> f32) -> Vec<Mesh> {
    [5, 4, 3, 2].iter().map(|&s| Mesh::icosphere(s, displace)).collect()
}

pub struct Scene {
    pub bodies: Vec<Body>,
    /// Si es false se usa siempre el LOD 0 (para comparar con el contador de triángulos).
    pub use_lod: bool,
    /// Dibujar las órbitas (tecla O).
    pub show_orbits: bool,
    pub orbit_shader: Arc<dyn FragmentShader>,
}

impl Scene {
    pub fn new() -> Scene {
        let sun = Body {
            name: "Sol",
            radius: 12.0,
            orbit_radius: 0.0,
            orbit_speed: 0.0,
            orbit_phase: 0.0,
            spin_speed: 0.05,
            tilt: 0.0,
            lods: sphere_lods(&|_| 1.0),
            shader: Arc::new(SunShader { seed: 1, speed: 0.15 }),
        };

        // THEMYSCIRA (Mujer Maravilla): islas de piedra clara y pasto sobre un mar Mediterráneo.
        // TODO(D1): cuando Body tenga `info`, llenarlo con:
        //   title: "Themyscira", hero: "Mujer Maravilla",
        //   fact: "Isla oculta de las amazonas; ningún hombre podía pisarla."
        // Deformación con AGUA PLANA: radio = 1 + max(0, elevación) × relieve.
        // Donde la elevación es negativa (mar) el radio queda en 1 exacto → superficie lisa.
        let themyscira = Body {
            name: "Themyscira",
            radius: 4.0,
            orbit_radius: 45.0,
            orbit_speed: 0.08,
            orbit_phase: 0.0,
            spin_speed: 0.25,
            tilt: 0.3,
            lods: sphere_lods(&|d| 1.0 + themyscira_elevation(d, THEMYSCIRA_SEED).max(0.0) * THEMYSCIRA_RELIEF),
            shader: Arc::new(ThemysciraShader {
                seed: THEMYSCIRA_SEED,
                // Mapa de normales 512x256: frecuencia 10 (tamaño de las rocas), relieve 3% del radio.
                normal_map: Some(NormalMap::generate(512, 256, THEMYSCIRA_SEED + 100, 10.0, 0.03)),
                normal_strength: 1.0,
            }),
        };

        // KRYPTON (Superman): cristales azules con venas rojas brillantes.
        // TODO(D1): cuando Body tenga estos campos, llenarlos con:
        //   gravity: 90.0,
        //   info: title "Krypton", hero "Superman",
        //         fact "Su sol rojo no le daba poderes a Kal-El; el sol amarillo de la Tierra sí."
        // Deformación RIDGED: radio = 1 + relieve × (1 - |fbm|)^afilado → cordilleras de cristal.
        let krypton = Body {
            name: "Krypton",
            radius: 5.0,
            orbit_radius: 70.0,
            orbit_speed: 0.05,
            orbit_phase: 2.4, // distinta a Themyscira para que no queden alineados
            spin_speed: 0.18,
            tilt: 0.15,
            lods: sphere_lods(&|d| 1.0 + krypton_ridge(d) * KRYPTON_RELIEF),
            shader: Arc::new(KryptonShader { seed: KRYPTON_SEED }),
        };

        // GOTHAM (Batman): planeta-ciudad oscuro, casi siempre de noche.
        // TODO(D1): cuando Body tenga estos campos, llenarlos con:
        //   gravity: 100.0,
        //   info: title "Gotham", hero "Batman",
        //         fact "Bruce Wayne patrulla de noche; de día es el dueño de Industrias Wayne."
        // Deformación EN ESCALÓN: cada manzana sube a su propia altura, como edificios.
        let gotham = Body {
            name: "Gotham",
            radius: 5.5,
            orbit_radius: 100.0,
            orbit_speed: 0.035,
            orbit_phase: 4.4, // distinta a los demás para que no queden alineados
            spin_speed: 0.12,
            tilt: 0.2,
            lods: sphere_lods(&|d| 1.0 + gotham_height(d, GOTHAM_SEED) * GOTHAM_BUILDING_HEIGHT),
            shader: Arc::new(GothamShader { seed: GOTHAM_SEED }),
        };

        // TODO(planetas): agregar más planetas con identidad propia (ver README).
        Scene {
            bodies: vec![sun, themyscira, krypton, gotham],
            use_lod: true,
            show_orbits: true,
            orbit_shader: Arc::new(OrbitShader { color: Vec3::new(0.95, 0.55, 0.3), dash_length: 3.0, speed: 3.0 }),
        }
    }

    pub fn draw(&self, r: &mut Renderer, t: f32) {
        let cam = r.uniforms.camera_pos;
        for b in &self.bodies {
            let lod = if self.use_lod { b.pick_lod(cam, t) } else { 0 };
            r.draw_mesh(&b.lods[lod], &b.model_matrix(t), b.shader.clone());
        }
        if self.show_orbits {
            // El sol no orbita (radio 0), así que no tiene línea.
            for b in self.bodies.iter().filter(|b| b.orbit_radius > 0.0) {
                r.draw_line_loop(&b.orbit_points(), self.orbit_shader.clone());
            }
        }
    }
}

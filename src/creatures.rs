//! PERSONAJES de la Liga que viven en los mundos (Diego, D5).
//!
//! Cada personaje tiene una trayectoria `path(t)` medida en el MARCO de su mundo
//! (`Scene::body_frame`: posición + inclinación + giro del planeta, sin escala).
//! Su matriz de modelo final es:  Model = body_frame(mundo) · path(t) · Escala
//! Así el personaje acompaña a su planeta mientras éste orbita al sol, sin cálculos extra.

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use std::sync::Arc;

// --- Superman (vuela en loops alrededor de Krypton) ---
/// Radio de sus vueltas en múltiplos del radio de Krypton.
pub const SUPERMAN_ORBIT_RADII: f32 = 1.6;
/// Radio de Krypton (debe coincidir con scene.rs) → radio de las vueltas en unidades del mundo.
const KRYPTON_RADIUS: f32 = 5.0;
/// Radianes por segundo: qué tan rápido da la vuelta.
pub const SUPERMAN_SPEED: f32 = 0.9;
/// Inclinación del plano de sus vueltas (radianes), para que no vuele sobre el ecuador.
const SUPERMAN_TILT: f32 = 0.5;
/// Sube y baja suave: amplitud (unidades) y frecuencia (veces por vuelta).
const SUPERMAN_BOB: f32 = 0.8;
const SUPERMAN_BOB_FREQ: f32 = 3.0;
/// Tamaño del modelo (radio de su esfera envolvente).
const SUPERMAN_SIZE: f32 = 0.7;
/// Colores del traje y de la capa (la capa es la parte trasera del modelo, z local > CAPE_Z).
const SUIT_BLUE: Vec3 = Vec3::new(0.10, 0.25, 0.80);
const CAPE_RED: Vec3 = Vec3::new(0.85, 0.08, 0.06);
const CAPE_Z: f32 = 0.25;

// --- Estela ---
/// Cuántas posiciones anteriores forman la estela y cada cuánto (segundos) se toma una.
const TRAIL_POINTS: usize = 20;
const TRAIL_STEP: f32 = 0.06;
const TRAIL_COLOR: Vec3 = Vec3::new(0.95, 0.25, 0.15);

/// Un personaje: su modelo, su shader y cómo se mueve en el marco de su mundo.
pub struct Creature {
    pub name: &'static str,
    /// Índice del mundo donde vive (en scene.bodies).
    pub home: usize,
    pub lods: Vec<Mesh>,
    pub shader: Arc<dyn FragmentShader>,
    /// Posición y orientación en el marco del planeta en el tiempo t (sin escala).
    pub path: fn(t: f32) -> Mat4,
    /// Escala del modelo.
    pub size: f32,
    /// Color de la estela (None = sin estela).
    pub trail: Option<Vec3>,
}

impl Creature {
    /// Matriz de modelo en el mundo: marco del planeta · trayectoria · escala.
    pub fn model(&self, scene: &Scene, t: f32) -> Mat4 {
        scene.body_frame(self.home, t) * (self.path)(t) * Mat4::scale(Vec3::splat(self.size))
    }

    /// Dónde está en el mundo en el tiempo t.
    pub fn position(&self, scene: &Scene, t: f32) -> Vec3 {
        self.model(scene, t).transform_point(Vec3::ZERO)
    }
}

/// Carga un modelo de assets/heroes/<nombre>.obj y, si existen, sus LODs <nombre>_lod1.obj y
/// <nombre>_lod2.obj (versiones con menos triángulos hechas con Decimate en Blender).
/// Si el modelo principal no existe todavía, usa `placeholder` para poder probar el movimiento.
fn load_hero(name: &str, placeholder: fn() -> Mesh) -> Vec<Mesh> {
    let main = format!("assets/heroes/{name}.obj");
    let Ok(first) = Mesh::load_obj(&main, Some(1.0)) else {
        eprintln!("Falta {main}: se usa un modelo provisional hasta que se modele en Blender");
        return vec![placeholder()];
    };
    let mut lods = vec![first];
    for k in 1..=2 {
        match Mesh::load_obj(&format!("assets/heroes/{name}_lod{k}.obj"), Some(1.0)) {
            Ok(m) => lods.push(m),
            Err(_) => break,
        }
    }
    lods // todas miden radio 1: el tamaño real se pone con la escala de la matriz de modelo
}

/// Trayectoria de Superman en el marco de Krypton: una vuelta inclinada con sube y baja,
/// mirando siempre hacia donde vuela.
fn superman_path(t: f32) -> Mat4 {
    let r = SUPERMAN_ORBIT_RADII * KRYPTON_RADIUS;
    let point = |t: f32| {
        let a = SUPERMAN_SPEED * t;
        let flat = Vec3::new(a.cos() * r, SUPERMAN_BOB * (a * SUPERMAN_BOB_FREQ).sin(), a.sin() * r);
        Mat4::rotation_x(SUPERMAN_TILT).transform_point(flat)
    };
    let pos = point(t);
    // Hacia dónde vuela = diferencia con la posición un instante después.
    let forward = (point(t + 0.01) - pos).normalized();
    // "Arriba" = hacia afuera del planeta, corregido para que sea perpendicular a forward.
    let out = pos.normalized();
    let up = (out - forward * out.dot(forward)).normalized();
    let right = forward.cross(up).normalized();
    Mat4::from_basis(right, up, forward, pos)
}

/// Todos los personajes que hay hasta ahora (por ahora Superman; D6 agrega al resto).
pub fn create_creatures(scene: &Scene) -> Vec<Creature> {
    let mut list = Vec::new();
    if let Some(krypton) = scene.bodies.iter().position(|b| b.name == "Krypton") {
        list.push(Creature {
            name: "Superman",
            home: krypton,
            lods: load_hero("superman", Mesh::placeholder_ship),
            shader: Arc::new(HeroShader { main: SUIT_BLUE, back: CAPE_RED, back_z: CAPE_Z }),
            path: superman_path,
            size: SUPERMAN_SIZE,
            trail: Some(TRAIL_COLOR),
        });
    }
    list
}

/// Dibuja a todos los personajes (con draw_lods: el culling y el LOD los cuentan) y sus estelas.
pub fn draw_creatures(r: &mut Renderer, scene: &Scene, t: f32, creatures: &[Creature]) {
    for c in creatures {
        r.draw_lods(&c.lods, &c.model(scene, t), c.shader.clone());
        if let Some(color) = c.trail {
            // Estela: las últimas TRAIL_POINTS posiciones. Como la trayectoria es una función del
            // tiempo, basta con evaluarla en t, t - paso, t - 2·paso... (no hay que guardar nada).
            let points: Vec<Vec3> = (0..TRAIL_POINTS).map(|k| c.position(scene, t - k as f32 * TRAIL_STEP)).collect();
            let length: f32 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
            r.draw_line_strip(&points, Arc::new(TrailShader { color, length }));
        }
    }
}

/// Personaje: color principal adelante y otro atrás (la capa), con Blinn-Phong.
struct HeroShader {
    main: Vec3,
    back: Vec3,
    /// Desde qué z local (hacia atrás, +Z) empieza el segundo color.
    back_z: f32,
}

impl FragmentShader for HeroShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let base = if f.local_pos.z > self.back_z { self.back } else { self.main };
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let spec = f.normal.dot((l + v).normalized()).max(0.0).powf(24.0);
        base * (0.2 + 0.8 * diff) + Vec3::splat(spec * 0.35)
    }
}

/// Estela que se desvanece: brillante junto al personaje (uv.0 = 0) y se apaga hacia la cola.
struct TrailShader {
    color: Vec3,
    /// Largo total de la estela, para saber qué tan lejos de la cabeza está cada píxel.
    length: f32,
}

impl FragmentShader for TrailShader {
    fn shade(&self, f: &Fragment, _u: &Uniforms) -> Vec3 {
        let fade = (1.0 - f.uv.0 / self.length.max(1e-3)).clamp(0.0, 1.0);
        self.color * fade * fade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superman_vuela_alrededor_de_krypton_a_1_6_radios() {
        let scene = Scene::new();
        let list = create_creatures(&scene);
        let sup = list.iter().find(|c| c.name == "Superman").unwrap();
        for k in 0..50 {
            let t = k as f32 * 0.37;
            let d = (sup.position(&scene, t) - scene.body_position(sup.home, t)).length();
            // Radio de las vueltas ± lo que sube y baja.
            assert!((d - SUPERMAN_ORBIT_RADII * scene.bodies[sup.home].radius).abs() < SUPERMAN_BOB + 0.05);
        }
    }

    #[test]
    fn superman_mira_hacia_donde_vuela() {
        // El "adelante" del modelo es -Z local: debe coincidir con la dirección del movimiento.
        let m = superman_path(2.0);
        let forward = -m.transform_dir(Vec3::new(0.0, 0.0, 1.0));
        let motion = (superman_path(2.01).transform_point(Vec3::ZERO) - m.transform_point(Vec3::ZERO)).normalized();
        assert!(forward.dot(motion) > 0.99);
    }
}

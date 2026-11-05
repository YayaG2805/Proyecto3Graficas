//! FRAGMENT SHADERS escritos por nosotros. Cada uno es una struct con sus parámetros
//! ("uniforms" propios del objeto) que implementa `FragmentShader`.
//!
//! Para agregar un shader nuevo:
//!   1. Crear una struct con sus parámetros.
//!   2. `impl FragmentShader for MiShader { fn shade(...) -> Vec3 { ... } }`
//!   3. Usarlo al crear el planeta en `scene.rs`.

use crate::math::Vec3;
use crate::noise::{fbm, hash01};
use crate::renderer::{Fragment, FragmentShader, Uniforms};

/// Iluminación difusa (Lambert) desde el sol + luz ambiente.
pub fn lambert(frag: &Fragment, u: &Uniforms, ambient: f32) -> f32 {
    let l = (u.light_pos - frag.world_pos).normalized();
    ambient + (1.0 - ambient) * frag.normal.dot(l).max(0.0)
}

fn mix(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a.lerp(b, t.clamp(0.0, 1.0))
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---------------------------------------------------------------------------
// SOL: shader ANIMADO. Ruido fBm que se desplaza con el tiempo (manchas/plasma)
// y un borde más brillante. No recibe luz: emite la suya.
// ---------------------------------------------------------------------------
pub struct SunShader {
    pub seed: u32,
    /// Qué tan rápido "hierve" la superficie.
    pub speed: f32,
}

impl FragmentShader for SunShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let t = u.time * self.speed;
        let p = f.local_pos * 3.0;
        // Domain warping: el ruido desplaza las coordenadas de otro ruido → aspecto de plasma.
        let warp = Vec3::new(fbm(p + Vec3::new(t, 0.0, 0.0), 3, self.seed), fbm(p + Vec3::new(0.0, t, 0.0), 3, self.seed + 7), 0.0);
        let n = fbm(p + warp * 1.5 + Vec3::splat(t * 0.5), 4, self.seed + 13) * 0.5 + 0.5;

        let deep = Vec3::new(0.85, 0.25, 0.02);
        let mid = Vec3::new(1.0, 0.6, 0.1);
        let hot = Vec3::new(1.0, 0.95, 0.6);
        let mut c = if n < 0.5 { mix(deep, mid, n * 2.0) } else { mix(mid, hot, (n - 0.5) * 2.0) };

        // Borde brillante (fresnel): donde la normal es casi perpendicular a la vista.
        let view = (u.camera_pos - f.world_pos).normalized();
        let rim = 1.0 - f.normal.dot(view).max(0.0);
        c += Vec3::new(1.0, 0.5, 0.1) * rim.powf(3.0) * 0.8;
        c
    }
}

// ---------------------------------------------------------------------------
// PLANETA ROCOSO: colores según altura del terreno (ruido) + Lambert.
// Usa local_pos para que el patrón gire con el planeta.
// ---------------------------------------------------------------------------
pub struct RockyShader {
    pub seed: u32,
    pub low: Vec3,
    pub high: Vec3,
    pub peak: Vec3,
}

impl FragmentShader for RockyShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let dir = f.local_pos.normalized();
        let h = fbm(dir * 3.0, 5, self.seed) * 0.5 + 0.5;
        let detail = fbm(dir * 25.0, 2, self.seed + 99) * 0.08; // textura fina para verse de cerca
        let mut c = mix(self.low, self.high, smoothstep(0.35, 0.6, h + detail));
        c = mix(c, self.peak, smoothstep(0.62, 0.75, h));
        c * lambert(f, u, 0.06)
    }
}

// ---------------------------------------------------------------------------
// NAVE: Blinn-Phong (difuso + brillo especular) + luz de borde para que se vea
// aunque esté del lado oscuro.
// ---------------------------------------------------------------------------
pub struct ShipShader {
    pub base: Vec3,
}

impl FragmentShader for ShipShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let h = (l + v).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let spec = f.normal.dot(h).max(0.0).powf(48.0);
        let rim = (1.0 - f.normal.dot(v).max(0.0)).powf(2.5) * 0.35;
        self.base * (0.18 + 0.82 * diff) + Vec3::splat(spec * 0.7) + Vec3::new(0.3, 0.6, 1.0) * rim
    }
}

// ---------------------------------------------------------------------------
// CIELO (skybox procedural): se evalúa por dirección de vista en cada píxel vacío.
// Las estrellas están "pegadas" a la dirección, así que se ven en el horizonte y
// giran correctamente cuando gira la cámara (pero no se acercan al moverse).
// ---------------------------------------------------------------------------
pub fn sky(dir: Vec3, u: &Uniforms) -> Vec3 {
    let mut c = Vec3::new(0.005, 0.006, 0.02);

    // Nebulosa tenue.
    let n = fbm(dir * 2.5, 3, 4242) * 0.5 + 0.5;
    c += Vec3::new(0.10, 0.03, 0.16) * smoothstep(0.55, 0.85, n);
    c += Vec3::new(0.02, 0.06, 0.12) * smoothstep(0.5, 0.8, 1.0 - n) * 0.6;

    // Estrellas: dividir el espacio en celdas; algunas celdas tienen una estrella.
    for (scale, density, seed) in [(320.0, 0.020, 11u32), (140.0, 0.010, 23u32)] {
        let p = dir * scale;
        let cell = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if hash01(cell.0, cell.1, cell.2, seed) < density {
            let jitter = Vec3::new(
                hash01(cell.0, cell.1, cell.2, seed + 1),
                hash01(cell.0, cell.1, cell.2, seed + 2),
                hash01(cell.0, cell.1, cell.2, seed + 3),
            ) * 0.6 + Vec3::splat(0.2);
            let star = Vec3::new(cell.0 as f32, cell.1 as f32, cell.2 as f32) + jitter;
            let d = (p - star).length();
            let twinkle = 0.75 + 0.25 * (u.time * 3.0 + hash01(cell.0, cell.1, cell.2, seed + 4) * 50.0).sin();
            let b = (1.0 - d / 0.6).max(0.0).powf(1.5) * twinkle;
            let tint = hash01(cell.0, cell.1, cell.2, seed + 5);
            c += mix(Vec3::new(0.7, 0.8, 1.0), Vec3::new(1.0, 0.85, 0.6), tint) * b;
        }
    }

    // Resplandor alrededor del sol.
    let to_sun = (u.light_pos - u.camera_pos).normalized();
    let g = dir.dot(to_sun).max(0.0);
    c += Vec3::new(1.0, 0.55, 0.15) * (g.powf(60.0) * 0.6 + g.powf(8.0) * 0.08);
    c
}

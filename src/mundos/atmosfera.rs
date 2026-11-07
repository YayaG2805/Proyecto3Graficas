//! ATMÓSFERAS: un brillo de color en el borde de cada planeta, como la Tierra vista desde el
//! espacio. Es una esfera un poco más grande que el planeta, dibujada ADITIVA (luz).
//!
//! Técnica: donde vemos la esfera de lado (su normal casi perpendicular a la vista) hay más
//! aire entre nosotros y el espacio, así que brilla más: brillo = (1 − n·v)^ATMOS_POWER.
//! Solo del lado de día (n·l > 0) brilla fuerte; del lado de noche queda un resplandor tenue.

use super::{body, smoothstep};
use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use std::sync::Arc;

/// Qué tan pegado al borde queda el brillo (más alto = franja más delgada).
const ATMOS_POWER: f32 = 3.0;
/// Cuánto brilla del lado de noche (0 = nada).
const ATMOS_NIGHT: f32 = 0.12;

/// (mundo, color, tamaño de la esfera en radios del planeta, fuerza).
/// El tamaño debe pasar las montañas más altas (Krypton sube hasta ~8 %).
const ATMOSPHERES: [(&str, Vec3, f32, f32); 5] = [
    ("Themyscira", Vec3::new(0.35, 0.60, 1.00), 1.12, 0.9),
    ("Krypton", Vec3::new(1.00, 0.35, 0.25), 1.14, 0.8),
    ("Gotham", Vec3::new(0.65, 0.45, 0.30), 1.08, 0.6),
    ("Atlantis", Vec3::new(0.20, 0.75, 1.00), 1.08, 0.9),
    ("Oa", Vec3::new(0.20, 1.00, 0.40), 1.10, 0.5),
];

pub struct Atmosferas {
    shell: Mesh,
    /// (índice del mundo, color, tamaño, fuerza)
    list: Vec<(usize, Vec3, f32, f32)>,
}

impl Atmosferas {
    pub fn new(scene: &Scene) -> Atmosferas {
        let list = ATMOSPHERES.iter().filter_map(|&(name, c, s, k)| body(scene, name).map(|i| (i, c, s, k))).collect();
        Atmosferas { shell: Mesh::icosphere(3, &|_| 1.0), list }
    }

    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        for &(i, color, size, strength) in &self.list {
            let model = Mat4::translation(scene.body_position(i, t)) * Mat4::scale(Vec3::splat(scene.bodies[i].radius * size));
            r.draw_mesh_additive(&self.shell, &model, Arc::new(AtmosphereShader { color, strength }));
        }
    }
}

struct AtmosphereShader {
    color: Vec3,
    strength: f32,
}

impl FragmentShader for AtmosphereShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let v = (u.camera_pos - f.world_pos).normalized();
        let l = (u.light_pos - f.world_pos).normalized();
        let facing = f.normal.dot(v).max(0.0);
        // Más brillo hacia el borde, pero en la orilla misma se apaga (si no, se ve una raya dura).
        let edge = (1.0 - facing).powf(ATMOS_POWER) * smoothstep(0.0, 0.3, facing);
        let day = smoothstep(-0.25, 0.35, f.normal.dot(l));
        self.color * (edge * self.strength * (ATMOS_NIGHT + (1.0 - ATMOS_NIGHT) * day))
    }
}

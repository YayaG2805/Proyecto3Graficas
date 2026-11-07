//! DETALLES Y MONUMENTOS DE CADA MUNDO (Diego). Un archivo por mundo:
//!   atmosfera.rs  brillo de aire en el borde de cada planeta
//!   sol.rs        llamaradas y corona
//!   themyscira.rs templo, nubes y olas en la costa
//!   krypton.rs    Fortaleza de la Soledad y géiseres de lava
//!   gotham.rs     Torre Wayne, dirigibles, reflectores y niebla
//!   atlantis.rs   ciudad con cúpula, peces, burbujas y remolino
//!   central_city.rs S.T.A.R. Labs y el acelerador de partículas
//! Cada uno tiene `draw` (lo opaco) y `draw_glow` (lo que es luz, aditivo, al final).
//! Aquí están las herramientas que comparten.

pub mod atmosfera;

use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
use crate::props::{instance_model, PropInstance};
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;

/// Todo lo extra de los mundos.
pub struct Mundos {
    atmosferas: atmosfera::Atmosferas,
}

impl Mundos {
    pub fn new(scene: &Scene) -> Mundos {
        Mundos { atmosferas: atmosfera::Atmosferas::new(scene) }
    }

    /// Lo opaco (modelos): va junto con el resto de la escena.
    pub fn draw(&self, _r: &mut Renderer, _scene: &Scene, _t: f32) {}

    /// Lo aditivo (luz): va después de todo lo opaco.
    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        self.atmosferas.draw_glow(r, scene, t);
    }
}

// ----------------------------------------------------------------------------------------
// Herramientas compartidas
// ----------------------------------------------------------------------------------------

/// Índice de un cuerpo por su nombre.
pub fn body(scene: &Scene, name: &str) -> Option<usize> {
    scene.bodies.iter().position(|b| b.name == name)
}

/// Matriz que pone algo mirando hacia `forward` con su "arriba" hacia `up`, en `pos`.
pub fn oriented(pos: Vec3, up: Vec3, forward: Vec3) -> Mat4 {
    let up = up.normalized();
    let forward = (forward - up * forward.dot(up)).normalized();
    let right = forward.cross(up).normalized();
    Mat4::from_basis(right, up, forward, pos)
}

/// Marco de algo PARADO en la superficie del mundo `i`, en la dirección `dir` (marco del
/// planeta) y girado `yaw` sobre sí mismo. Sigue al planeta mientras orbita y gira.
pub fn on_surface(scene: &Scene, i: usize, dir: Vec3, yaw: f32, t: f32) -> Mat4 {
    let b = &scene.bodies[i];
    let dir = dir.normalized();
    let p = PropInstance { kind: 0, dir, scale: 1.0, yaw };
    instance_model(&scene.body_frame(i, t), b.radius * (b.surface)(dir), &p)
}

/// Carga un modelo que se mueve (pez, dirigible...) con sus LODs, todos de radio 1.
/// Si falta el archivo usa una esfera, para que el juego no se caiga.
pub fn load_model(name: &str) -> Vec<Mesh> {
    let lods: Vec<Mesh> = ["", "_lod1", "_lod2"]
        .iter()
        .filter_map(|s| Mesh::load_obj(&format!("assets/props/{name}{s}.obj"), Some(1.0)).ok())
        .collect();
    if lods.is_empty() {
        eprintln!("Falta assets/props/{name}.obj: se usa una esfera");
        return vec![Mesh::icosphere(1, &|_| 1.0)];
    }
    lods
}

/// Agrega un cuadrilátero (a, b, c, d en orden) con sus DOS caras, para verlo de ambos lados
/// aunque el renderer descarte las caras traseras.
pub fn quad_both_sides(m: &mut Mesh, corners: [(Vec3, (f32, f32)); 4]) {
    let base = m.vertices.len() as u32;
    for (position, uv) in corners {
        m.vertices.push(Vertex { position, normal: Vec3::ZERO, uv, tangent: Vec3::ZERO });
    }
    m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    m.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
}

/// Transición suave de 0 a 1 entre a y b.
pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Modelo de Blender coloreado por PALETA (cada parte trae su banda en u, ver
/// modelar_mundos.py). Lambert + luz de borde; las bandas de `glow` además brillan solas
/// (EMISIVO: se ven también del lado de noche).
pub struct PaletteShader {
    pub palette: Vec<Vec3>,
    /// Bandas que emiten luz propia y cuánto.
    pub glow: Vec<(usize, f32)>,
    /// Color de la luz de borde (recorta la silueta contra el espacio).
    pub rim: Vec3,
}

impl FragmentShader for PaletteShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let n = self.palette.len();
        let band = ((f.uv.0 * n as f32).floor() as usize).min(n - 1);
        let base = self.palette[band];
        let l = (u.light_pos - f.world_pos).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let diff = f.normal.dot(l).max(0.0);
        let rim = (1.0 - f.normal.dot(v).max(0.0)).powf(3.0);
        let emit = self.glow.iter().find(|(b, _)| *b == band).map_or(0.0, |(_, k)| *k);
        base * (0.12 + 0.88 * diff + emit) + self.rim * (rim * 0.4)
    }
}

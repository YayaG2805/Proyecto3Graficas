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
pub mod sol;
pub mod themyscira;
pub mod krypton;
pub mod gotham;
pub mod atlantis;
pub mod central_city;

use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
use crate::props::{instance_model, PropInstance};
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;

/// Todo lo extra de los mundos. Cada uno es Option: si ese mundo no está en la escena, no se arma.
pub struct Mundos {
    atmosferas: atmosfera::Atmosferas,
    sol: Option<sol::Sol>,
    themyscira: Option<themyscira::Themyscira>,
    krypton: Option<krypton::Krypton>,
    gotham: Option<gotham::Gotham>,
    atlantis: Option<atlantis::Atlantis>,
    central_city: Option<central_city::CentralCity>,
}

impl Mundos {
    pub fn new(scene: &Scene) -> Mundos {
        Mundos {
            atmosferas: atmosfera::Atmosferas::new(scene),
            sol: sol::Sol::new(scene),
            themyscira: themyscira::Themyscira::new(scene),
            krypton: krypton::Krypton::new(scene),
            gotham: gotham::Gotham::new(scene),
            atlantis: atlantis::Atlantis::new(scene),
            central_city: central_city::CentralCity::new(scene),
        }
    }

    /// Lo opaco (modelos): va junto con el resto de la escena.
    pub fn draw(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        if let Some(w) = &self.themyscira {
            w.draw(r, scene, t);
        }
        if let Some(w) = &self.krypton {
            w.draw(r, scene, t);
        }
        if let Some(w) = &self.gotham {
            w.draw(r, scene, t);
        }
        if let Some(w) = &self.atlantis {
            w.draw(r, scene, t);
        }
        if let Some(w) = &self.central_city {
            w.draw(r, scene, t);
        }
    }

    /// Monumentos para las capturas del snapshot: (nombre, base, "arriba", tamaño).
    pub fn landmarks(&self, scene: &Scene, t: f32) -> Vec<(&'static str, Vec3, Vec3, f32)> {
        let mut list = Vec::new();
        if let Some(w) = &self.themyscira {
            let (p, up) = w.landmark(scene, t);
            list.push(("templo", p, up, 1.0));
        }
        if let Some(w) = &self.krypton {
            let (p, up) = w.landmark(scene, t);
            list.push(("fortaleza", p, up, 2.2));
        }
        if let Some(w) = &self.gotham {
            let (p, up) = w.landmark(scene, t);
            list.push(("torre_wayne", p, up, 2.4));
        }
        if let Some(w) = &self.atlantis {
            let (p, up) = w.landmark(scene, t);
            list.push(("ciudad_atlantis", p, up, 1.8));
            let (p, up) = w.whirl_landmark(scene, t);
            list.push(("remolino", p, up, 1.6));
            let (p, up) = w.fish_landmark(scene, t);
            list.push(("peces", p - up * 0.5, up, 1.2));
        }
        if let Some(w) = &self.central_city {
            let (p, up) = w.landmark(scene, t);
            list.push(("star_labs", p, up, 0.6));
        }
        list
    }

    /// Lo aditivo (luz): va después de todo lo opaco.
    pub fn draw_glow(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        self.atmosferas.draw_glow(r, scene, t);
        if let Some(s) = &self.sol {
            s.draw_glow(r, scene, t);
        }
        if let Some(w) = &self.themyscira {
            w.draw_glow(r, scene, t);
        }
        if let Some(w) = &self.krypton {
            w.draw_glow(r, scene, t);
        }
        if let Some(w) = &self.gotham {
            w.draw_glow(r, scene, t);
        }
        if let Some(w) = &self.atlantis {
            w.draw_glow(r, scene, t);
        }
        if let Some(w) = &self.central_city {
            w.draw_glow(r, scene, t);
        }
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

/// Cono abierto a lo largo de +Y: radio `base` en y = 0 y radio 1 en y = 1. Sirve para chorros
/// y haces de luz (se escala con el largo y el ancho que haga falta).
pub fn cone_mesh(segments: usize, base: f32) -> Mesh {
    let mut m = Mesh::default();
    for (y, r) in [(0.0, base), (1.0, 1.0)] {
        for j in 0..segments {
            let a = j as f32 / segments as f32 * std::f32::consts::TAU;
            m.vertices.push(Vertex { position: Vec3::new(a.cos() * r, y, a.sin() * r), normal: Vec3::ZERO, uv: (0.0, y), tangent: Vec3::ZERO });
        }
    }
    for j in 0..segments {
        let k = (j + 1) % segments;
        let (a, b, c, d) = (j as u32, k as u32, (segments + j) as u32, (segments + k) as u32);
        m.indices.extend([a, c, d, a, d, b]); // antihorario visto desde afuera
    }
    m.recompute_normals();
    m.compute_bounds();
    m
}

/// Haz o chorro de luz con forma de cono (aditivo): brilla más al centro (donde lo vemos de
/// frente) y se apaga hacia la punta. La normal se reconstruye desde el eje, porque la escala
/// no uniforme del cono deforma la de la malla.
pub struct BeamShader {
    pub origin: Vec3,
    pub dir: Vec3,
    pub length: f32,
    /// Color en la base y en la punta.
    pub base_color: Vec3,
    pub tip_color: Vec3,
    pub brightness: f32,
}

impl FragmentShader for BeamShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let rel = f.world_pos - self.origin;
        let along = (rel.dot(self.dir) / self.length).clamp(0.0, 1.0);
        let normal = (rel - self.dir * rel.dot(self.dir)).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let facing = normal.dot(v).abs();
        let color = self.base_color + (self.tip_color - self.base_color) * along;
        color * (self.brightness * facing * facing * (1.0 - 0.7 * along))
    }
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
    /// Banda que PARPADEA (luces de aviso), si hay.
    pub blink: Option<usize>,
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
        let mut emit = self.glow.iter().find(|(b, _)| *b == band).map_or(0.0, |(_, k)| *k);
        if self.blink == Some(band) {
            emit *= if (u.time * 3.0).sin() > 0.3 { 1.0 } else { 0.15 };
        }
        base * (0.12 + 0.88 * diff + emit) + self.rim * (rim * 0.4)
    }
}

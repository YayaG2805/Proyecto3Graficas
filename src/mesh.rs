//! Mallas: vértices + índices. Carga de .obj y generación procedural de esferas.

use crate::math::Vec3;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: (f32, f32),
    /// Dirección en la que crece `u` sobre la superficie (para el mapeo de normales).
    /// ZERO si la malla no la usa.
    pub tangent: Vec3,
}

/// Coordenadas (u, v) de un punto de la esfera de radio 1: u = longitud, v = latitud.
/// u va de 0 a 1 dando la vuelta; v va de 0 (polo norte) a 1 (polo sur).
pub fn sphere_uv(d: Vec3) -> (f32, f32) {
    let u = 0.5 + d.z.atan2(d.x) / (2.0 * std::f32::consts::PI);
    let v = 0.5 - d.y.clamp(-1.0, 1.0).asin() / std::f32::consts::PI;
    (u, v)
}

/// Inversa de `sphere_uv`: de (u, v) a un punto de la esfera de radio 1.
pub fn sphere_dir(u: f32, v: f32) -> Vec3 {
    let lon = (u - 0.5) * 2.0 * std::f32::consts::PI;
    let lat = (0.5 - v) * std::f32::consts::PI;
    Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    /// Cada 3 índices forman un triángulo (orden antihorario = cara frontal).
    pub indices: Vec<u32>,
    /// Radio de la esfera que envuelve la malla (en espacio local). Se usa para el culling.
    pub bounding_radius: f32,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn compute_bounds(&mut self) {
        self.bounding_radius = self
            .vertices
            .iter()
            .map(|v| v.position.length())
            .fold(0.0, f32::max);
    }

    /// Recalcula normales suaves promediando las normales de las caras que tocan cada vértice.
    /// Necesario después de deformar la geometría.
    pub fn recompute_normals(&mut self) {
        for v in &mut self.vertices {
            v.normal = Vec3::ZERO;
        }
        for tri in self.indices.chunks_exact(3) {
            let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let (pa, pb, pc) = (self.vertices[a].position, self.vertices[b].position, self.vertices[c].position);
            let n = (pb - pa).cross(pc - pa); // sin normalizar: pesa por área
            self.vertices[a].normal += n;
            self.vertices[b].normal += n;
            self.vertices[c].normal += n;
        }
        for v in &mut self.vertices {
            v.normal = v.normal.normalized();
        }
    }

    /// Carga un .obj (triangulado). Si el archivo no trae normales, se calculan.
    /// `target_size`: escala la malla para que su radio envolvente sea este valor (None = sin escalar).
    pub fn load_obj(path: &str, target_size: Option<f32>) -> Result<Mesh, String> {
        let opts = tobj::LoadOptions { triangulate: true, single_index: true, ..Default::default() };
        let (models, _materials) = tobj::load_obj(path, &opts).map_err(|e| format!("{path}: {e}"))?;

        let mut mesh = Mesh::default();
        for model in models {
            let m = model.mesh;
            let base = mesh.vertices.len() as u32;
            let vcount = m.positions.len() / 3;
            let has_normals = m.normals.len() == m.positions.len();
            for i in 0..vcount {
                let position = Vec3::new(m.positions[3 * i], m.positions[3 * i + 1], m.positions[3 * i + 2]);
                let normal = if has_normals {
                    Vec3::new(m.normals[3 * i], m.normals[3 * i + 1], m.normals[3 * i + 2])
                } else {
                    Vec3::ZERO
                };
                let uv = if m.texcoords.len() >= 2 * (i + 1) { (m.texcoords[2 * i], m.texcoords[2 * i + 1]) } else { (0.0, 0.0) };
                mesh.vertices.push(Vertex { position, normal, uv, tangent: Vec3::ZERO });
            }
            mesh.indices.extend(m.indices.iter().map(|i| i + base));
            if !has_normals {
                mesh.recompute_normals();
            }
        }

        // Centrar la malla en su origen y escalarla.
        let n = mesh.vertices.len().max(1) as f32;
        let center = mesh.vertices.iter().fold(Vec3::ZERO, |acc, v| acc + v.position) / n;
        for v in &mut mesh.vertices {
            v.position -= center;
        }
        mesh.compute_bounds();
        if let Some(size) = target_size {
            let s = size / mesh.bounding_radius.max(1e-6);
            for v in &mut mesh.vertices {
                v.position = v.position * s;
            }
            mesh.compute_bounds();
        }
        Ok(mesh)
    }

    /// ICOSFERA: parte de un icosaedro (20 triángulos) y subdivide cada triángulo en 4,
    /// empujando los puntos nuevos a la superficie de la esfera.
    /// Triángulos = 20 * 4^subdivisiones  (0→20, 1→80, 2→320, 3→1280, 4→5120, 5→20480)
    ///
    /// `displace(dir) -> radio`: permite deformar la esfera (montañas, cráteres...).
    /// Para una esfera perfecta usar `|_| 1.0`.
    pub fn icosphere(subdivisions: u32, displace: &dyn Fn(Vec3) -> f32) -> Mesh {
        let t = (1.0 + 5f32.sqrt()) / 2.0;
        let mut dirs: Vec<Vec3> = [
            (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
            (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
            (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
        ]
        .iter()
        .map(|&(x, y, z)| Vec3::new(x, y, z).normalized())
        .collect();

        let mut faces: Vec<[u32; 3]> = vec![
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
            [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
            [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];

        for _ in 0..subdivisions {
            // Cache para no duplicar el punto medio de una arista compartida.
            let mut midpoint: HashMap<(u32, u32), u32> = HashMap::new();
            let mut mid = |a: u32, b: u32, dirs: &mut Vec<Vec3>| -> u32 {
                let key = if a < b { (a, b) } else { (b, a) };
                *midpoint.entry(key).or_insert_with(|| {
                    dirs.push(((dirs[a as usize] + dirs[b as usize]) * 0.5).normalized());
                    (dirs.len() - 1) as u32
                })
            };
            let mut new_faces = Vec::with_capacity(faces.len() * 4);
            for &[a, b, c] in &faces {
                let ab = mid(a, b, &mut dirs);
                let bc = mid(b, c, &mut dirs);
                let ca = mid(c, a, &mut dirs);
                new_faces.push([a, ab, ca]);
                new_faces.push([b, bc, ab]);
                new_faces.push([c, ca, bc]);
                new_faces.push([ab, bc, ca]);
            }
            faces = new_faces;
        }

        let vertices = dirs
            .iter()
            .map(|&d| {
                // Tangente = derivada del punto (cos·cos, sin, cos·sin) respecto a la longitud:
                // apunta hacia el "este", que es hacia donde crece u. En los polos vale cero.
                let tangent = Vec3::new(-d.z, 0.0, d.x).normalized();
                Vertex { position: d * displace(d), normal: d, uv: sphere_uv(d), tangent }
            })
            .collect();

        let mut mesh = Mesh { vertices, indices: faces.into_iter().flatten().collect(), bounding_radius: 1.0 };
        mesh.recompute_normals();
        mesh.compute_bounds();
        mesh
    }

    /// Caja (mínimo, máximo) de los vértices que cumplen `pred`. None si ninguno la cumple.
    /// Sirve, por ejemplo, para encontrar dónde están los motores de la nave (material MOTOR).
    pub fn bounds_where(&self, pred: impl Fn(&Vertex) -> bool) -> Option<(Vec3, Vec3)> {
        let mut it = self.vertices.iter().filter(|v| pred(v)).map(|v| v.position);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| {
            (
                Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
            )
        }))
    }

    /// LLAMA del motor: un cono de sección elíptica que sale hacia +Z (atrás de la nave).
    /// La boca mide `half_w` x `half_h` (semiejes) en z = 0 y la punta está en z = 1;
    /// el largo real se da con la matriz de modelo (escala en Z según el turbo).
    /// uv = (vuelta alrededor de 0 a 1, avance a lo largo de 0 a 1).
    pub fn exhaust_plume(half_w: f32, half_h: f32, segments: u32) -> Mesh {
        // (avance z, radio relativo): se ensancha un poco al salir y luego se cierra en punta.
        let rings = [(0.0, 0.9), (0.15, 1.0), (0.45, 0.8), (0.75, 0.45), (1.0, 0.0)];
        let n = segments as usize;
        let mut mesh = Mesh::default();
        for &(z, r) in &rings {
            for j in 0..n {
                let a = j as f32 / n as f32 * std::f32::consts::TAU;
                mesh.vertices.push(Vertex {
                    position: Vec3::new(a.cos() * r * half_w, a.sin() * r * half_h, z),
                    normal: Vec3::ZERO,
                    uv: (j as f32 / n as f32, z),
                    tangent: Vec3::ZERO,
                });
            }
        }
        // Cada par de anillos se une con quads (2 triángulos), antihorarios vistos desde afuera.
        for i in 0..rings.len() - 1 {
            for j in 0..n {
                let k = (j + 1) % n;
                let (p00, p01) = ((i * n + j) as u32, (i * n + k) as u32);
                let (p10, p11) = (((i + 1) * n + j) as u32, ((i + 1) * n + k) as u32);
                mesh.indices.extend([p00, p01, p11, p00, p11, p10]);
            }
        }
        mesh.recompute_normals();
        // En la punta todos los puntos coinciden: su normal debe apuntar hacia atrás.
        for v in mesh.vertices.iter_mut().skip((rings.len() - 1) * n) {
            v.normal = Vec3::new(0.0, 0.0, 1.0);
        }
        mesh.compute_bounds();
        mesh
    }

    /// Nave provisional (una "flecha" low-poly) mientras modelamos la de verdad en Blender.
    /// La nariz apunta a -Z (adelante en espacio local).
    /// TODO(nave): reemplazar por assets/ship.obj modelado por nosotros.
    pub fn placeholder_ship() -> Mesh {
        let p = [
            Vec3::new(0.0, 0.0, -1.6),  // 0 nariz
            Vec3::new(-1.2, -0.1, 0.8), // 1 ala izq
            Vec3::new(1.2, -0.1, 0.8),  // 2 ala der
            Vec3::new(0.0, 0.45, 0.6),  // 3 cabina (arriba)
            Vec3::new(0.0, -0.3, 0.7),  // 4 panza
            Vec3::new(0.0, 0.0, 1.0),   // 5 cola
        ];
        let tris = [[0, 1, 3], [0, 3, 2], [0, 4, 1], [0, 2, 4], [1, 5, 3], [3, 5, 2], [1, 4, 5], [2, 5, 4]];
        flat_mesh(&p, &tris)
    }
}

/// Crea una malla con normales por cara (estilo low-poly) y orienta cada cara hacia afuera.
pub fn flat_mesh(points: &[Vec3], tris: &[[usize; 3]]) -> Mesh {
    let center = points.iter().fold(Vec3::ZERO, |a, &p| a + p) / points.len() as f32;
    let mut mesh = Mesh::default();
    for t in tris {
        let (mut a, b, mut c) = (points[t[0]], points[t[1]], points[t[2]]);
        let mut n = (b - a).cross(c - a).normalized();
        let centroid = (a + b + c) / 3.0;
        if n.dot(centroid - center) < 0.0 {
            std::mem::swap(&mut a, &mut c);
            n = -n;
        }
        let base = mesh.vertices.len() as u32;
        for p in [a, b, c] {
            mesh.vertices.push(Vertex { position: p, normal: n, uv: (0.0, 0.0), tangent: Vec3::ZERO });
        }
        mesh.indices.extend([base, base + 1, base + 2]);
    }
    mesh.compute_bounds();
    mesh
}

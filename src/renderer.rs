//! PIPELINE DE RENDER POR SOFTWARE (todo escrito por nosotros).
//!
//! Por cada objeto (`draw_mesh`):
//!   1. Culling por esfera envolvente contra el frustum (si está fuera, no se procesa).
//!   2. Etapa de vértices: posición de clip = Projection * View * Model * v.
//!   3. Ensamblado de triángulos + recorte contra el plano cercano (near clipping).
//!   4. División de perspectiva → NDC → coordenadas de pantalla (viewport).
//!   5. Back-face culling.
//!   6. Rasterización con coordenadas baricéntricas + interpolación corregida por perspectiva.
//!   7. Prueba de profundidad (z-buffer). El ganador se guarda en un G-buffer.
//!
//! Al final del cuadro (`finish_frame`):
//!   8. Etapa de fragmentos: se ejecuta el fragment shader de cada píxel visible (en paralelo).
//!      Los píxeles vacíos usan el shader del cielo (skybox procedural).
//!
//! Esto es "sombreado diferido": el shader corre UNA vez por píxel visible, no por cada
//! triángulo que pasa encima. Así un planeta cercano que llena la pantalla no mata el FPS.

use crate::math::{Mat4, Vec3, Vec4};
use crate::mesh::Mesh;
use rayon::prelude::*;
use std::sync::Arc;

/// Datos globales del cuadro que reciben todos los shaders.
#[derive(Clone, Copy, Debug)]
pub struct Uniforms {
    pub time: f32,
    pub view: Mat4,
    pub projection: Mat4,
    pub camera_pos: Vec3,
    /// Base de la cámara (para calcular la dirección de cada píxel del cielo).
    pub camera_right: Vec3,
    pub camera_up: Vec3,
    pub camera_forward: Vec3,
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    /// El sol es la luz (luz puntual).
    pub light_pos: Vec3,
}

/// Lo que recibe un fragment shader por cada píxel.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fragment {
    /// Posición en el mundo.
    pub world_pos: Vec3,
    /// Normal en el mundo (normalizada).
    pub normal: Vec3,
    /// Posición en espacio del objeto: sirve para que los patrones "giren" junto con el planeta.
    pub local_pos: Vec3,
    pub uv: (f32, f32),
}

/// Un fragment shader: recibe el fragmento y devuelve un color RGB en [0, 1].
/// `Send + Sync` porque se ejecuta desde varios hilos a la vez.
pub trait FragmentShader: Send + Sync {
    fn shade(&self, frag: &Fragment, u: &Uniforms) -> Vec3;
}

/// Contadores para demostrar que el culling y el LOD cambian el trabajo del renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub objects_drawn: u32,
    pub objects_culled: u32,
    /// Triángulos que entraron al pipeline (de objetos no descartados).
    pub triangles_in: u32,
    /// Triángulos que llegaron a rasterizarse (después de recorte y back-face culling).
    pub triangles_rasterized: u32,
}

/// Lo que el G-buffer guarda por píxel tras la prueba de profundidad.
#[derive(Clone, Copy, Default)]
struct GPixel {
    /// Profundidad = 1/w (más grande = más cerca). 0 = vacío.
    inv_w: f32,
    /// Índice del draw call dueño del píxel (para saber qué shader usar).
    draw: u16,
    frag: Fragment,
}

/// Vértice después de la etapa de vértices.
#[derive(Clone, Copy, Default)]
struct ClipVertex {
    clip: Vec4,
    world: Vec3,
    normal: Vec3,
    local: Vec3,
    uv: (f32, f32),
}

impl ClipVertex {
    fn lerp(&self, o: &ClipVertex, t: f32) -> ClipVertex {
        ClipVertex {
            clip: self.clip.lerp(o.clip, t),
            world: self.world.lerp(o.world, t),
            normal: self.normal.lerp(o.normal, t),
            local: self.local.lerp(o.local, t),
            uv: (self.uv.0 + (o.uv.0 - self.uv.0) * t, self.uv.1 + (o.uv.1 - self.uv.1) * t),
        }
    }
}

pub struct Renderer {
    pub width: usize,
    pub height: usize,
    /// Imagen final RGBA8 que se sube a la textura de raylib.
    pub color: Vec<u8>,
    gbuffer: Vec<GPixel>,
    shaders: Vec<Arc<dyn FragmentShader>>,
    pub uniforms: Uniforms,
    pub stats: Stats,
    /// Se pueden apagar para comparar (teclas en main).
    pub frustum_culling: bool,
    pub backface_culling: bool,
}

impl Renderer {
    pub fn new(width: usize, height: usize, uniforms: Uniforms) -> Self {
        Renderer {
            width,
            height,
            color: vec![0; width * height * 4],
            gbuffer: vec![GPixel::default(); width * height],
            shaders: Vec::new(),
            uniforms,
            stats: Stats::default(),
            frustum_culling: true,
            backface_culling: true,
        }
    }

    pub fn begin_frame(&mut self, uniforms: Uniforms) {
        self.uniforms = uniforms;
        self.stats = Stats::default();
        self.shaders.clear();
        self.gbuffer.fill(GPixel::default());
    }

    /// ¿La esfera (centro en mundo, radio) toca el volumen de visión? (culling)
    pub fn sphere_visible(&self, center: Vec3, radius: f32) -> bool {
        let u = &self.uniforms;
        let c = u.view.transform_point(center); // a espacio de cámara (mira hacia -Z)
        let depth = -c.z;
        if depth + radius < u.near || depth - radius > u.far {
            return false;
        }
        let ty = (u.fov_y * 0.5).tan();
        let tx = ty * u.aspect;
        // Distancia con signo a los 4 planos laterales del frustum.
        let side = |coord: f32, t: f32| (coord.abs() - depth * t) / (1.0 + t * t).sqrt();
        side(c.x, tx) <= radius && side(c.y, ty) <= radius
    }

    /// Dibuja una malla con su matriz de modelo y su shader.
    /// Devuelve false si se descartó por estar fuera de la vista.
    pub fn draw_mesh(&mut self, mesh: &Mesh, model: &Mat4, shader: Arc<dyn FragmentShader>) -> bool {
        // --- 1. Culling por esfera envolvente ---
        if self.frustum_culling {
            let center = model.transform_point(Vec3::ZERO);
            let scale = max_scale(model);
            if !self.sphere_visible(center, mesh.bounding_radius * scale) {
                self.stats.objects_culled += 1;
                return false;
            }
        }
        self.stats.objects_drawn += 1;
        self.stats.triangles_in += mesh.triangle_count() as u32;

        if self.shaders.len() >= u16::MAX as usize - 1 {
            return false;
        }
        let draw_id = self.shaders.len() as u16 + 1; // 0 = vacío
        self.shaders.push(shader);

        // --- 2. Etapa de vértices ---
        let mvp = self.uniforms.projection * self.uniforms.view * *model;
        let verts: Vec<ClipVertex> = mesh
            .vertices
            .iter()
            .map(|v| {
                let p = v.position;
                ClipVertex {
                    clip: mvp.transform_vec4(Vec4::new(p.x, p.y, p.z, 1.0)),
                    world: model.transform_point(p),
                    normal: model.transform_dir(v.normal).normalized(),
                    local: p,
                    uv: v.uv,
                }
            })
            .collect();

        // --- 3. Ensamblado de triángulos ---
        for tri in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (verts[tri[0] as usize], verts[tri[1] as usize], verts[tri[2] as usize]);
            if outside_same_plane(&a.clip, &b.clip, &c.clip) {
                continue; // todo el triángulo fuera de un mismo borde de la pantalla
            }
            // Recorte contra el plano cercano: z + w >= 0 (si no, se dibuja "detrás" de la cámara).
            let polygon = clip_near(&[a, b, c]);
            for i in 1..polygon.len().saturating_sub(1) {
                self.raster_triangle(&polygon[0], &polygon[i], &polygon[i + 1], draw_id);
            }
        }
        true
    }

    /// --- 4 a 7: pantalla, back-face, rasterizado y z-buffer ---
    fn raster_triangle(&mut self, a: &ClipVertex, b: &ClipVertex, c: &ClipVertex, draw_id: u16) {
        let (w, h) = (self.width as f32, self.height as f32);

        // División de perspectiva y viewport.
        let to_screen = |v: &ClipVertex| {
            let iw = 1.0 / v.clip.w;
            let (nx, ny) = (v.clip.x * iw, v.clip.y * iw);
            ((nx + 1.0) * 0.5 * w, (1.0 - ny) * 0.5 * h, iw)
        };
        let (x0, y0, iw0) = to_screen(a);
        let (x1, y1, iw1) = to_screen(b);
        let (x2, y2, iw2) = to_screen(c);

        // Área con signo. En pantalla Y crece hacia abajo, así que los triángulos
        // frontales (antihorarios en el mundo) quedan con área negativa.
        let area = (x1 - x0) * (y2 - y0) - (y1 - y0) * (x2 - x0);
        if area.abs() < 1e-9 || (self.backface_culling && area > 0.0) {
            return;
        }
        self.stats.triangles_rasterized += 1;
        let inv_area = 1.0 / area;

        // Caja envolvente recortada a la pantalla.
        let min_x = x0.min(x1).min(x2).floor().max(0.0) as usize;
        let max_x = (x0.max(x1).max(x2).ceil() as isize).min(self.width as isize - 1);
        let min_y = y0.min(y1).min(y2).floor().max(0.0) as usize;
        let max_y = (y0.max(y1).max(y2).ceil() as isize).min(self.height as isize - 1);
        if max_x < 0 || max_y < 0 {
            return;
        }

        // Atributos divididos por w (para la corrección de perspectiva).
        let (wa, wb, wc) = (a.world * iw0, b.world * iw1, c.world * iw2);
        let (na, nb, nc) = (a.normal * iw0, b.normal * iw1, c.normal * iw2);
        let (la, lb, lc) = (a.local * iw0, b.local * iw1, c.local * iw2);
        let (ua, ub, uc) = ((a.uv.0 * iw0, a.uv.1 * iw0), (b.uv.0 * iw1, b.uv.1 * iw1), (c.uv.0 * iw2, c.uv.1 * iw2));

        for py in min_y..=max_y as usize {
            let fy = py as f32 + 0.5;
            let row = py * self.width;
            for px in min_x..=max_x as usize {
                let fx = px as f32 + 0.5;
                // Coordenadas baricéntricas (funciones de arista).
                let b0 = ((x1 - fx) * (y2 - fy) - (y1 - fy) * (x2 - fx)) * inv_area;
                let b1 = ((x2 - fx) * (y0 - fy) - (y2 - fy) * (x0 - fx)) * inv_area;
                let b2 = 1.0 - b0 - b1;
                if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                    continue;
                }
                // 1/w sí es lineal en pantalla → sirve como profundidad.
                let inv_w = b0 * iw0 + b1 * iw1 + b2 * iw2;
                let gp = &mut self.gbuffer[row + px];
                if inv_w <= gp.inv_w {
                    continue; // prueba de profundidad: ya hay algo más cerca
                }
                let wcorr = 1.0 / inv_w;
                gp.inv_w = inv_w;
                gp.draw = draw_id;
                gp.frag = Fragment {
                    world_pos: (wa * b0 + wb * b1 + wc * b2) * wcorr,
                    normal: (na * b0 + nb * b1 + nc * b2) * wcorr, // se normaliza al sombrear
                    local_pos: (la * b0 + lb * b1 + lc * b2) * wcorr,
                    uv: ((ua.0 * b0 + ub.0 * b1 + uc.0 * b2) * wcorr, (ua.1 * b0 + ub.1 * b1 + uc.1 * b2) * wcorr),
                };
            }
        }
    }

    /// --- 8. Etapa de fragmentos (en paralelo por filas) ---
    pub fn finish_frame(&mut self, sky: &(dyn Fn(Vec3, &Uniforms) -> Vec3 + Sync)) {
        let u = self.uniforms;
        let (w, h) = (self.width, self.height);
        let shaders = &self.shaders;
        let gbuffer = &self.gbuffer;
        let ty = (u.fov_y * 0.5).tan();
        let tx = ty * u.aspect;

        self.color.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
            let ndc_y = 1.0 - (y as f32 + 0.5) / h as f32 * 2.0;
            for x in 0..w {
                let gp = &gbuffer[y * w + x];
                let rgb = if gp.draw == 0 {
                    let ndc_x = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                    let dir = (u.camera_forward + u.camera_right * (ndc_x * tx) + u.camera_up * (ndc_y * ty)).normalized();
                    sky(dir, &u)
                } else {
                    let mut f = gp.frag;
                    f.normal = f.normal.normalized();
                    shaders[gp.draw as usize - 1].shade(&f, &u)
                };
                let c = rgb.clamp01();
                let i = x * 4;
                row[i] = (c.x * 255.0) as u8;
                row[i + 1] = (c.y * 255.0) as u8;
                row[i + 2] = (c.z * 255.0) as u8;
                row[i + 3] = 255;
            }
        });
    }

    /// Guarda el cuadro como imagen PPM (sin dependencias). Útil para capturas y pruebas.
    pub fn save_ppm(&self, path: &str) -> std::io::Result<()> {
        let mut data = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        for px in self.color.chunks_exact(4) {
            data.extend_from_slice(&px[..3]);
        }
        std::fs::write(path, data)
    }
}

/// La escala más grande que aplica una matriz de modelo (para agrandar la esfera envolvente).
fn max_scale(m: &Mat4) -> f32 {
    (0..3)
        .map(|c| Vec3::new(m.m[0][c], m.m[1][c], m.m[2][c]).length())
        .fold(0.0, f32::max)
}

/// ¿Los 3 vértices están fuera del MISMO plano del volumen de clip? Entonces el triángulo no se ve.
fn outside_same_plane(a: &Vec4, b: &Vec4, c: &Vec4) -> bool {
    let out = |f: &dyn Fn(&Vec4) -> bool| f(a) && f(b) && f(c);
    out(&|v| v.x > v.w) || out(&|v| v.x < -v.w) || out(&|v| v.y > v.w) || out(&|v| v.y < -v.w) || out(&|v| v.z < -v.w)
}

/// Sutherland–Hodgman contra un solo plano (el cercano). Un triángulo puede salir
/// como 0, 3 o 4 vértices (este último se vuelve 2 triángulos en abanico).
fn clip_near(input: &[ClipVertex; 3]) -> Vec<ClipVertex> {
    let dist = |v: &ClipVertex| v.clip.z + v.clip.w; // >= 0 → del lado visible
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let cur = &input[i];
        let next = &input[(i + 1) % 3];
        let (dc, dn) = (dist(cur), dist(next));
        if dc >= 0.0 {
            out.push(*cur);
        }
        if (dc >= 0.0) != (dn >= 0.0) {
            out.push(cur.lerp(next, dc / (dc - dn)));
        }
    }
    out
}

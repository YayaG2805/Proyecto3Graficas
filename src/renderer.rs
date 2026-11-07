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
//! Las líneas (`draw_line_loop`, para las órbitas) usan las mismas etapas 2, 4 y 7, pero
//! se recortan con `clip_segment` y se rasterizan con DDA en vez de baricéntricas.
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

/// cuando el objeto se ve mas chico q estos tamaños (tamaño / distancia) pasamos al siguiente lod
const LOD_THRESHOLDS: [f32; 3] = [0.5, 0.15, 0.05];
/// Si un objeto mide menos que esto en pantalla (radio en píxeles), no se dibuja (con LOD activo).
const MIN_PIXEL_RADIUS: f32 = 0.5;
/// Desde cuántos vértices la etapa de vértices se reparte entre todos los núcleos.
const PARALLEL_VERTICES: usize = 2048;

/// Datos globales del cuadro que reciben todos los shaders.
#[derive(Clone, Copy, Debug)]
pub struct Uniforms {
    pub time: f32,
    pub view: Mat4,
    pub projection: Mat4,
    pub camera_pos: Vec3,
    /// donde va la nave, pa los shaders q reaccionan cuando se acerca
    // el allow se quita cuando algun shader lo lea (s8)
    #[allow(dead_code)]
    pub ship_pos: Vec3,
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
    /// Activar el mapeo de normales en los shaders que lo usan (tecla N).
    pub normal_mapping: bool,
    /// Qué tan avanzado va el warp: 0 = vuelo normal, 1 = velocidad máxima (rayas en el cielo).
    pub warp: f32,
    /// ECLIPSE: esfera (centro, radio) que puede tapar el sol, p. ej. la luna Central City sobre
    /// Gotham. Radio 0 = no hay. La usa `sun_visibility`.
    pub eclipse: (Vec3, f32),
}

/// ¿Cuánto sol le llega al punto `p`? 1 = todo, 0 = sombra total del eclipse.
/// Se traza el rayo de `p` hacia el sol y se mide qué tan cerca pasa del centro de la esfera
/// que tapa: si pasa por dentro de ella hay sombra; cerca del borde, penumbra (transición suave).
pub fn sun_visibility(u: &Uniforms, p: Vec3) -> f32 {
    let (center, radius) = u.eclipse;
    if radius <= 0.0 {
        return 1.0;
    }
    let to_sun = (u.light_pos - p).normalized();
    let along = (center - p).dot(to_sun);
    if along <= 0.0 {
        return 1.0; // la esfera está detrás del punto, no entre él y el sol
    }
    let closest = (center - (p + to_sun * along)).length();
    let t = ((closest - radius * 0.7) / (radius * 0.6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    /// Tangente en el mundo (sin normalizar). Junto con la normal forma la base TBN.
    pub tangent: Vec3,
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

/// Profundidad y dueño de cada píxel (el z-buffer). El fragmento va APARTE, en `frags`.
/// OPTIMIZACIÓN: antes todo iba junto (~70 bytes por píxel). Separado, la prueba de
/// profundidad y la limpieza de cada cuadro solo tocan 8 bytes por píxel: caben muchos más
/// píxeles en la memoria caché del procesador.
#[derive(Clone, Copy, Default)]
struct GPixel {
    /// Profundidad = 1/w (más grande = más cerca). 0 = vacío.
    inv_w: f32,
    /// Índice del draw call dueño del píxel (para saber qué shader usar).
    draw: u16,
}

/// Vértice después de la etapa de vértices.
#[derive(Clone, Copy, Default)]
struct ClipVertex {
    clip: Vec4,
    world: Vec3,
    normal: Vec3,
    local: Vec3,
    uv: (f32, f32),
    tangent: Vec3,
}

impl ClipVertex {
    fn lerp(&self, o: &ClipVertex, t: f32) -> ClipVertex {
        ClipVertex {
            clip: self.clip.lerp(o.clip, t),
            world: self.world.lerp(o.world, t),
            normal: self.normal.lerp(o.normal, t),
            local: self.local.lerp(o.local, t),
            uv: (self.uv.0 + (o.uv.0 - self.uv.0) * t, self.uv.1 + (o.uv.1 - self.uv.1) * t),
            tangent: self.tangent.lerp(o.tangent, t),
        }
    }
}

pub struct Renderer {
    pub width: usize,
    pub height: usize,
    /// Imagen final RGBA8 que se sube a la textura de raylib.
    pub color: Vec<u8>,
    gbuffer: Vec<GPixel>,
    /// Fragmento ganador de cada píxel (solo vale si gbuffer[i].draw != 0).
    frags: Vec<Fragment>,
    /// Luz que se SUMA encima de la imagen (objetos aditivos, como la llama del turbo).
    glow: Vec<Vec3>,
    shaders: Vec<Arc<dyn FragmentShader>>,
    /// OPTIMIZACIÓN: los fragmentos ADITIVOS no se sombrean al rasterizar (eso corría en un
    /// solo núcleo); se guardan aquí (píxel, shader, fragmento) y se sombrean todos juntos en
    /// paralelo en `finish_frame`. Como la luz se SUMA, el orden no importa.
    additive_shaders: Vec<Arc<dyn FragmentShader>>,
    additive_frags: Vec<(u32, u16, Fragment)>,
    /// Memoria reutilizada de un cuadro a otro (así no se pide memoria nueva en cada dibujo).
    additive_colors: Vec<Vec3>,
    scratch_verts: Vec<ClipVertex>,
    pub uniforms: Uniforms,
    pub stats: Stats,
    /// Se pueden apagar para comparar (teclas en main).
    pub frustum_culling: bool,
    pub backface_culling: bool,
    /// si esta en false draw_lods usa siempre el mas detallado, pa comparar los contadores (tecla L)
    pub use_lod: bool,
}

impl Renderer {
    pub fn new(width: usize, height: usize, uniforms: Uniforms) -> Self {
        Renderer {
            width,
            height,
            color: vec![0; width * height * 4],
            gbuffer: vec![GPixel::default(); width * height],
            frags: vec![Fragment::default(); width * height],
            glow: vec![Vec3::ZERO; width * height],
            shaders: Vec::new(),
            additive_shaders: Vec::new(),
            additive_frags: Vec::new(),
            additive_colors: Vec::new(),
            scratch_verts: Vec::new(),
            uniforms,
            stats: Stats::default(),
            frustum_culling: true,
            backface_culling: true,
            use_lod: true,
        }
    }

    pub fn begin_frame(&mut self, uniforms: Uniforms) {
        self.uniforms = uniforms;
        self.stats = Stats::default();
        self.shaders.clear();
        self.additive_shaders.clear();
        self.additive_frags.clear();
        // OPTIMIZACIÓN: limpiar en paralelo y solo lo necesario: la profundidad y el dueño del
        // píxel (draw = 0 → vacío). Los fragmentos no se borran: solo se leen si alguien
        // escribió encima en este cuadro, y en ese caso ya traen datos nuevos.
        self.gbuffer.par_iter_mut().for_each(|g| *g = GPixel::default());
        self.glow.par_iter_mut().for_each(|g| *g = Vec3::ZERO);
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
        self.draw_mesh_mode(mesh, model, shader, false)
    }

    /// Elige el LOD según el tamaño aparente y dibuja (con culling). lods[0] = más detallado.
    /// Devuelve false si se descartó por estar fuera de la vista.
    pub fn draw_lods(&mut self, lods: &[Mesh], model: &Mat4, shader: Arc<dyn FragmentShader>) -> bool {
        if lods.is_empty() {
            return false;
        }
        if self.use_lod && self.pixel_radius(&lods[0], model) < MIN_PIXEL_RADIUS {
            // OPTIMIZACIÓN: mide menos de medio píxel en pantalla → no se notaría; no se dibuja.
            self.stats.objects_culled += 1;
            return false;
        }
        let level = if self.use_lod { self.pick_lod(lods, model) } else { 0 };
        self.draw_mesh(&lods[level], model, shader)
    }

    /// Radio aproximado del objeto en PÍXELES: su tamaño aparente (radio / distancia) por los
    /// píxeles que caben en una unidad de tangente del campo de visión.
    fn pixel_radius(&self, mesh: &Mesh, model: &Mat4) -> f32 {
        let u = &self.uniforms;
        let dist = (model.transform_point(Vec3::ZERO) - u.camera_pos).length().max(0.001);
        let apparent = mesh.bounding_radius * max_scale(model) / dist;
        apparent * self.height as f32 * 0.5 / (u.fov_y * 0.5).tan()
    }

    // cual lod usar: entre mas chico se ve el objeto en pantalla, menos triangulos necesita
    fn pick_lod(&self, lods: &[Mesh], model: &Mat4) -> usize {
        // paso 1: q tan lejos esta el centro del objeto de la camara
        let center = model.transform_point(Vec3::ZERO);
        let dist = (center - self.uniforms.camera_pos).length().max(0.001);

        // paso 2: su tamaño real entre la distancia da mas o menos lo q ocupa en pantalla
        let apparent = lods[0].bounding_radius * max_scale(model) / dist;

        // paso 3: el primer umbral q supera es su nivel, si no supera ninguno va el ultimo
        let level = LOD_THRESHOLDS.iter().position(|&limit| apparent > limit).unwrap_or(LOD_THRESHOLDS.len());

        // paso 4: por si el objeto trae menos lods q umbrales
        level.min(lods.len() - 1)
    }

    /// Dibuja una malla ADITIVA (luz: llamas, brillos). Diferencias con `draw_mesh`:
    ///   - Hace la prueba de profundidad contra lo opaco, pero NO escribe en el z-buffer,
    ///     así que lo que está detrás se sigue viendo (parece transparente).
    ///   - Sus fragmentos se sombrean al final, todos juntos y en paralelo, y su color se SUMA
    ///     en `glow`.
    /// Se debe llamar DESPUÉS de dibujar todos los objetos opacos del cuadro.
    pub fn draw_mesh_additive(&mut self, mesh: &Mesh, model: &Mat4, shader: Arc<dyn FragmentShader>) -> bool {
        self.draw_mesh_mode(mesh, model, shader, true)
    }

    fn draw_mesh_mode(&mut self, mesh: &Mesh, model: &Mat4, shader: Arc<dyn FragmentShader>, additive: bool) -> bool {
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

        // Cada draw call guarda su shader en su lista (opacos o aditivos); draw_id = posición + 1
        // (0 = vacío).
        let list = if additive { &mut self.additive_shaders } else { &mut self.shaders };
        if list.len() >= u16::MAX as usize - 1 {
            return false;
        }
        list.push(shader);
        let draw_id = list.len() as u16;

        // --- 2. Etapa de vértices ---
        // (Se reutiliza el mismo Vec en cada dibujo: std::mem::take lo "presta" y al final se
        // devuelve, así no se pide memoria nueva por cada uno de los ~1500 asteroides.)
        let mvp = self.uniforms.projection * self.uniforms.view * *model;
        let mut verts = std::mem::take(&mut self.scratch_verts);
        verts.clear();
        let model = *model;
        let transform = move |v: &crate::mesh::Vertex| {
            let p = v.position;
            ClipVertex {
                clip: mvp.transform_vec4(Vec4::new(p.x, p.y, p.z, 1.0)),
                world: model.transform_point(p),
                normal: model.transform_dir(v.normal).normalized(),
                local: p,
                uv: v.uv,
                // La tangente es una dirección, igual que la normal: gira con el objeto.
                tangent: model.transform_dir(v.tangent),
            }
        };
        // OPTIMIZACIÓN: las mallas grandes (los planetas cercanos tienen ~10 000 vértices)
        // transforman sus vértices en paralelo; cada vértice es independiente. Las pequeñas
        // no, porque repartir el trabajo entre hilos cuesta más que hacerlo directo.
        if mesh.vertices.len() >= PARALLEL_VERTICES {
            verts.par_extend(mesh.vertices.par_iter().map(transform));
        } else {
            verts.extend(mesh.vertices.iter().map(transform));
        }

        // --- 3. Ensamblado de triángulos ---
        for tri in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (verts[tri[0] as usize], verts[tri[1] as usize], verts[tri[2] as usize]);
            if outside_same_plane(&a.clip, &b.clip, &c.clip) {
                continue; // todo el triángulo fuera de un mismo borde de la pantalla
            }
            // Recorte contra el plano cercano: z + w >= 0 (si no, se dibuja "detrás" de la cámara).
            let polygon = clip_near(&[a, b, c]);
            for i in 1..polygon.len().saturating_sub(1) {
                self.raster_triangle(&polygon[0], &polygon[i], &polygon[i + 1], draw_id, additive);
            }
        }
        self.scratch_verts = verts;
        true
    }

    /// Dibuja una línea cerrada (p. ej. una órbita) que une todos los puntos en orden.
    /// Los puntos ya están en el mundo, así que no hay matriz de modelo: clip = P * V * p.
    /// En `uv.0` cada píxel recibe la distancia recorrida sobre la línea (sirve para puntear).
    pub fn draw_line_loop(&mut self, points: &[Vec3], shader: Arc<dyn FragmentShader>) {
        self.draw_polyline(points, true, shader);
    }

    /// Igual que `draw_line_loop` pero ABIERTA: el último punto no se une con el primero
    /// (sirve para estelas). `uv.0` = distancia desde el primer punto.
    pub fn draw_line_strip(&mut self, points: &[Vec3], shader: Arc<dyn FragmentShader>) {
        self.draw_polyline(points, false, shader);
    }

    fn draw_polyline(&mut self, points: &[Vec3], closed: bool, shader: Arc<dyn FragmentShader>) {
        if points.len() < 2 || self.shaders.len() >= u16::MAX as usize - 1 {
            return;
        }
        let draw_id = self.shaders.len() as u16 + 1; // 0 = vacío
        self.shaders.push(shader);

        // --- Etapa de vértices (igual que en draw_mesh, pero sin Model) ---
        let vp = self.uniforms.projection * self.uniforms.view;
        let to_clip = |p: Vec3, dist: f32| ClipVertex {
            clip: vp.transform_vec4(Vec4::new(p.x, p.y, p.z, 1.0)),
            world: p,
            normal: Vec3::ZERO, // una línea no tiene normal
            local: p,
            uv: (dist, 0.0),
            tangent: Vec3::ZERO,
        };

        let mut dist = 0.0; // distancia recorrida sobre la línea
        let segments = if closed { points.len() } else { points.len() - 1 };
        for i in 0..segments {
            let (pa, pb) = (points[i], points[(i + 1) % points.len()]); // cerrada: el último se une con el primero
            let seg_len = (pb - pa).length();
            let (a, b) = (to_clip(pa, dist), to_clip(pb, dist + seg_len));
            dist += seg_len;
            // Recorte del segmento contra el volumen de visión (incluye el plano cercano).
            if let Some((a, b)) = clip_segment(&a, &b) {
                self.raster_line(&a, &b, draw_id);
            }
        }
    }

    /// Rasterizado de un segmento con DDA + prueba de profundidad (z-buffer).
    fn raster_line(&mut self, a: &ClipVertex, b: &ClipVertex, draw_id: u16) {
        let (w, h) = (self.width as f32, self.height as f32);

        // División de perspectiva y viewport (mismas fórmulas que en raster_triangle).
        let to_screen = |v: &ClipVertex| {
            let iw = 1.0 / v.clip.w;
            let (nx, ny) = (v.clip.x * iw, v.clip.y * iw);
            ((nx + 1.0) * 0.5 * w, (1.0 - ny) * 0.5 * h, iw)
        };
        let (x0, y0, iw0) = to_screen(a);
        let (x1, y1, iw1) = to_screen(b);

        // DDA: damos un paso por píxel en el eje donde la línea avanza más.
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).ceil().max(1.0) as usize;

        // Atributos divididos por w (corrección de perspectiva, igual que en los triángulos).
        let (wa, wb) = (a.world * iw0, b.world * iw1);
        let (ua, ub) = (a.uv.0 * iw0, b.uv.0 * iw1);

        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            if x < 0.0 || y < 0.0 || x >= w || y >= h {
                continue;
            }
            // 1/w es lineal en pantalla → sirve como profundidad.
            let inv_w = iw0 + (iw1 - iw0) * t;
            let idx = y as usize * self.width + x as usize;
            let gp = &mut self.gbuffer[idx];
            if inv_w <= gp.inv_w {
                continue; // prueba de profundidad: un planeta o el sol está delante
            }
            let wcorr = 1.0 / inv_w;
            let world = wa.lerp(wb, t) * wcorr;
            gp.inv_w = inv_w;
            gp.draw = draw_id;
            self.frags[idx] = Fragment {
                world_pos: world,
                normal: Vec3::ZERO,
                local_pos: world,
                uv: ((ua + (ub - ua) * t) * wcorr, 0.0),
                tangent: Vec3::ZERO,
            };
        }
    }

    /// --- 4 a 7: pantalla, back-face, rasterizado y z-buffer ---
    /// `additive`: el triángulo es aditivo (ver `draw_mesh_additive`) y `draw_id` es su shader
    /// en `additive_shaders`.
    fn raster_triangle(&mut self, a: &ClipVertex, b: &ClipVertex, c: &ClipVertex, draw_id: u16, additive: bool) {
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
        let (ta, tb, tc) = (a.tangent * iw0, b.tangent * iw1, c.tangent * iw2);

        // OPTIMIZACIÓN: las funciones de arista son LINEALES en x, así que en vez de calcularlas
        // desde cero en cada píxel (varias multiplicaciones) se calculan al inicio de la fila y
        // luego solo se les SUMA un paso fijo por píxel.
        let step0 = (y1 - y2) * inv_area; // cuánto cambia b0 al avanzar un píxel en x
        let step1 = (y2 - y0) * inv_area; // cuánto cambia b1
        for py in min_y..=max_y as usize {
            let fy = py as f32 + 0.5;
            let row = py * self.width;
            let fx0 = min_x as f32 + 0.5;
            // Coordenadas baricéntricas (funciones de arista) en el primer píxel de la fila.
            let mut b0 = ((x1 - fx0) * (y2 - fy) - (y1 - fy) * (x2 - fx0)) * inv_area;
            let mut b1 = ((x2 - fx0) * (y0 - fy) - (y2 - fy) * (x0 - fx0)) * inv_area;
            let mut inside_row = false;
            for px in min_x..=max_x as usize {
                let (c0, c1) = (b0, b1);
                b0 += step0;
                b1 += step1;
                let b2 = 1.0 - c0 - c1;
                if c0 < 0.0 || c1 < 0.0 || b2 < 0.0 {
                    // El triángulo es convexo: si ya entramos a él en esta fila y salimos, el
                    // resto de la fila queda afuera → pasar a la siguiente fila.
                    if inside_row {
                        break;
                    }
                    continue;
                }
                inside_row = true;
                let (b0, b1) = (c0, c1);
                // 1/w sí es lineal en pantalla → sirve como profundidad.
                let inv_w = b0 * iw0 + b1 * iw1 + b2 * iw2;
                let gp = &mut self.gbuffer[row + px];
                if inv_w <= gp.inv_w {
                    continue; // prueba de profundidad: ya hay algo más cerca
                }
                let wcorr = 1.0 / inv_w;
                let frag = Fragment {
                    world_pos: (wa * b0 + wb * b1 + wc * b2) * wcorr,
                    normal: (na * b0 + nb * b1 + nc * b2) * wcorr, // se normaliza al sombrear
                    local_pos: (la * b0 + lb * b1 + lc * b2) * wcorr,
                    uv: ((ua.0 * b0 + ub.0 * b1 + uc.0 * b2) * wcorr, (ua.1 * b0 + ub.1 * b1 + uc.1 * b2) * wcorr),
                    tangent: (ta * b0 + tb * b1 + tc * b2) * wcorr,
                };
                if additive {
                    // Aditivo: se anota para sombrearlo después (en paralelo); el z-buffer no cambia.
                    self.additive_frags.push(((row + px) as u32, draw_id, frag));
                    continue;
                }
                gp.inv_w = inv_w;
                gp.draw = draw_id;
                self.frags[row + px] = frag;
            }
        }
    }

    /// --- 8. Etapa de fragmentos (en paralelo por filas) ---
    pub fn finish_frame(&mut self, sky: &(dyn Fn(Vec3, &Uniforms) -> Vec3 + Sync)) {
        let u = self.uniforms;

        // --- 8a. Fragmentos aditivos: se sombrean en paralelo (cada uno es independiente) y
        // luego se suman a `glow` en orden (sumar es barato; sombrear es lo caro).
        let add_shaders = &self.additive_shaders;
        self.additive_colors.resize(self.additive_frags.len(), Vec3::ZERO);
        self.additive_colors.par_iter_mut().zip(self.additive_frags.par_iter()).for_each(|(out, (_, id, frag))| {
            let mut f = *frag;
            f.normal = f.normal.normalized();
            *out = add_shaders[*id as usize - 1].shade(&f, &u);
        });
        for ((px, _, _), c) in self.additive_frags.iter().zip(&self.additive_colors) {
            self.glow[*px as usize] += *c;
        }
        let (w, h) = (self.width, self.height);
        let shaders = &self.shaders;
        let gbuffer = &self.gbuffer;
        let frags = &self.frags;
        let glow = &self.glow;
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
                    let mut f = frags[y * w + x];
                    f.normal = f.normal.normalized();
                    shaders[gp.draw as usize - 1].shade(&f, &u)
                };
                // Mezcla aditiva: la luz de los objetos aditivos se suma al color de abajo.
                let c = (rgb + glow[y * w + x]).clamp01();
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

/// Recorta un segmento contra los planos del volumen de visión en espacio de clip
/// (izquierda, derecha, abajo, arriba y cercano). Cada plano es una "distancia" que debe ser >= 0.
/// Así nunca rasterizamos detrás de la cámara ni recorremos píxeles fuera de la pantalla.
/// Devuelve None si el segmento queda totalmente fuera.
fn clip_segment(a: &ClipVertex, b: &ClipVertex) -> Option<(ClipVertex, ClipVertex)> {
    let planes: [fn(&Vec4) -> f32; 5] = [
        |v| v.w + v.x, // izquierda: x >= -w
        |v| v.w - v.x, // derecha:   x <= w
        |v| v.w + v.y, // abajo:     y >= -w
        |v| v.w - v.y, // arriba:    y <= w
        |v| v.w + v.z, // cercano:   z >= -w
    ];
    // Parte visible del segmento: de t0 a t1 (0 = punto a, 1 = punto b).
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for dist in planes {
        let (da, db) = (dist(&a.clip), dist(&b.clip));
        if da < 0.0 && db < 0.0 {
            return None; // los dos extremos fuera del mismo plano
        }
        if da < 0.0 {
            t0 = t0.max(da / (da - db)); // a está fuera: el segmento entra en este punto
        } else if db < 0.0 {
            t1 = t1.min(da / (da - db)); // b está fuera: el segmento sale en este punto
        }
    }
    if t0 > t1 {
        return None;
    }
    Some((a.lerp(b, t0), a.lerp(b, t1)))
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

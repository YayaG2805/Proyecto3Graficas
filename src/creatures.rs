//! PERSONAJES de la Liga que viven en los mundos (Diego, D5 y D6).
//!
//! Cada personaje tiene una trayectoria `path(t)` medida en el MARCO de su mundo
//! (`Scene::body_frame`: posición + inclinación + giro del planeta, sin escala).
//! Su matriz de modelo final es:  Model = body_frame(mundo) · place · path(t) · Escala
//!   - `place`: dónde está parado si no se mueve (Batman en su azotea); si se mueve es la identidad.
//!   - `path(t)`: su movimiento (Superman dando vueltas); si está quieto es la identidad.
//! Así el personaje acompaña a su planeta mientras éste orbita al sol, sin cálculos extra.
//!
//! Los modelos se cargan de assets/heroes/<nombre>.obj (hechos por nosotros en Blender).
//! Mientras no exista el archivo se usa una figura provisional hecha con cajas.

use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
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
const SUPERMAN_TRAIL: Vec3 = Vec3::new(0.95, 0.25, 0.15);

// --- Flash (corre por el ecuador de Central City) ---
/// Radio de Central City (debe coincidir con scene.rs).
const CENTRAL_CITY_RADIUS: f32 = 1.2;
/// Radianes por segundo: da una vuelta a la luna en ~2 segundos.
pub const FLASH_SPEED: f32 = 3.0;
/// Tamaño de Flash (radio de su esfera envolvente).
const FLASH_SIZE: f32 = 0.16;
const FLASH_RED: Vec3 = Vec3::new(0.80, 0.06, 0.04);
const FLASH_TRAIL: Vec3 = Vec3::new(1.0, 0.85, 0.15);

// --- Batman (quieto en la azotea más alta de Gotham) ---
/// Tamaño de Batman. Es más grande que un edificio para que se vea desde la nave.
const BATMAN_SIZE: f32 = 0.22;
const BATMAN_SUIT: Vec3 = Vec3::new(0.22, 0.22, 0.26);
const BATMAN_CAPE: Vec3 = Vec3::new(0.04, 0.04, 0.05);

// --- Aquaman montado en una ballena espacial (alrededor de Atlantis) ---
/// Radio de Atlantis (debe coincidir con scene.rs).
const ATLANTIS_RADIUS: f32 = 6.0;
/// Radio de la vuelta de la ballena en múltiplos del radio de Atlantis.
const WHALE_ORBIT_RADII: f32 = 1.35;
/// Radianes por segundo: nada lento.
pub const WHALE_SPEED: f32 = 0.25;
/// Ondulación: cuánto sube y baja y qué tan seguido.
const WHALE_BOB: f32 = 0.5;
const WHALE_BOB_FREQ: f32 = 0.8;
/// Tamaño de la ballena y de Aquaman.
const WHALE_SIZE: f32 = 1.3;
const AQUAMAN_SIZE: f32 = 0.22;
/// Qué tan arriba del centro de la ballena va parado Aquaman (su lomo).
const WHALE_BACK: f32 = 0.42;
const WHALE_COLOR: Vec3 = Vec3::new(0.32, 0.42, 0.58);
const AQUAMAN_ORANGE: Vec3 = Vec3::new(0.95, 0.55, 0.10);
const AQUAMAN_GREEN: Vec3 = Vec3::new(0.10, 0.55, 0.25);

// --- Linterna Verde (flota sobre Oa con un brillo que pulsa) ---
/// Radio de Oa (debe coincidir con scene.rs).
const OA_RADIUS: f32 = 4.5;
/// Altura a la que flota, en múltiplos del radio de Oa, y sobre qué punto del planeta.
const LANTERN_HOVER_RADII: f32 = 1.35;
const LANTERN_SPOT: Vec3 = Vec3::new(0.3, 0.9, 0.3);
/// Sube y baja suave al flotar, y qué tan rápido gira sobre sí mismo.
const LANTERN_BOB: f32 = 0.3;
const LANTERN_TURN: f32 = 0.4;
/// Pulsos de luz por segundo (en radianes) y tamaño del aura en múltiplos de su tamaño.
pub const LANTERN_PULSE_SPEED: f32 = 3.0;
const LANTERN_AURA: f32 = 2.4;
const LANTERN_SIZE: f32 = 0.3;
const LANTERN_GREEN: Vec3 = Vec3::new(0.15, 1.0, 0.35);

// --- Estela ---
/// Cuántas posiciones anteriores forman la estela.
const TRAIL_POINTS: usize = 20;
/// Qué tan arriba del centro de una figura parada quedan sus pies (en múltiplos de su tamaño).
const FEET: f32 = 0.95;

/// Un personaje: su modelo, su shader y cómo se mueve en el marco de su mundo.
pub struct Creature {
    pub name: &'static str,
    /// Índice del mundo donde vive (en scene.bodies).
    pub home: usize,
    pub lods: Vec<Mesh>,
    pub shader: Arc<dyn FragmentShader>,
    /// Posición y orientación en el marco del planeta en el tiempo t (sin escala).
    pub path: fn(t: f32) -> Mat4,
    /// Lugar fijo en el marco del planeta (identidad si el personaje se mueve con `path`).
    pub place: Mat4,
    /// Escala del modelo.
    pub size: f32,
    /// Color de la estela (None = sin estela) y cada cuántos segundos se toma un punto.
    pub trail: Option<Vec3>,
    pub trail_step: f32,
    /// Aura de luz alrededor (aditiva) con su color y su esfera, o None.
    pub aura: Option<(Vec3, Mesh)>,
}

impl Creature {
    /// Matriz de modelo en el mundo: marco del planeta · lugar · trayectoria · escala.
    pub fn model(&self, scene: &Scene, t: f32) -> Mat4 {
        scene.body_frame(self.home, t) * self.place * (self.path)(t) * Mat4::scale(Vec3::splat(self.size))
    }

    /// Dónde está en el mundo en el tiempo t.
    pub fn position(&self, scene: &Scene, t: f32) -> Vec3 {
        self.model(scene, t).transform_point(Vec3::ZERO)
    }
}

/// Trayectoria de quien está quieto: no se mueve en el marco de su planeta.
fn still(_t: f32) -> Mat4 {
    Mat4::identity()
}

/// El punto más alto de un planeta: el vértice de su malla más detallada más lejos del centro
/// (la deformación ya subió montañas o edificios). Devuelve (dirección, distancia al centro).
fn highest_point(scene: &Scene, i: usize) -> (Vec3, f32) {
    let b = &scene.bodies[i];
    let top = b.lods[0].vertices.iter().map(|v| v.position).fold(Vec3::ZERO, |best, p| {
        if p.length() > best.length() { p } else { best }
    });
    (top.normalized(), top.length() * b.radius)
}

/// Lugar fijo de una figura PARADA sobre la superficie: pies en `dir · radio`, "arriba" = `dir`.
fn standing_on(dir: Vec3, radius: f32, size: f32) -> Mat4 {
    // Cualquier dirección perpendicular a "arriba" sirve como "adelante".
    let side = if dir.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
    oriented(dir * (radius + size * FEET), dir, dir.cross(side))
}

/// Matriz que pone a alguien mirando hacia `forward` con su "arriba" hacia `up` en `pos`.
/// `forward` se corrige para que sea perpendicular a `up`.
fn oriented(pos: Vec3, up: Vec3, forward: Vec3) -> Mat4 {
    let up = up.normalized();
    let forward = (forward - up * forward.dot(up)).normalized();
    let right = forward.cross(up).normalized();
    Mat4::from_basis(right, up, forward, pos)
}

/// Agrega una caja (centro `c`, medias medidas `h`) a la malla, con normales por cara.
/// Cada cara usa dos ejes (u, v) con u × v = normal, así sus triángulos quedan antihorarios
/// vistos desde afuera (que es lo que el back-face culling considera "de frente").
fn add_box(mesh: &mut Mesh, c: Vec3, h: Vec3) {
    let x = Vec3::new(1.0, 0.0, 0.0);
    let y = Vec3::new(0.0, 1.0, 0.0);
    let z = Vec3::new(0.0, 0.0, 1.0);
    // Cuánto mide la caja a lo largo de un eje (los ejes son x, y, z con signo).
    let half = |a: Vec3| a.x.abs() * h.x + a.y.abs() * h.y + a.z.abs() * h.z;
    for (n, u, v) in [(x, y, z), (-x, z, y), (y, z, x), (-y, x, z), (z, x, y), (-z, y, x)] {
        let base = mesh.vertices.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = c + n * half(n) + u * (su * half(u)) + v * (sv * half(v));
            mesh.vertices.push(Vertex { position: p, normal: n, uv: (0.0, 0.0), tangent: Vec3::ZERO });
        }
        mesh.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Figura PROVISIONAL de una persona parada (piernas, torso, brazos y cabeza con cajas),
/// mirando a -Z. Solo se usa mientras falta el .obj modelado en Blender.
fn placeholder_figure() -> Mesh {
    let mut m = Mesh::default();
    for side in [-1.0, 1.0] {
        add_box(&mut m, Vec3::new(0.15 * side, -0.55, 0.0), Vec3::new(0.09, 0.4, 0.09)); // pierna
        add_box(&mut m, Vec3::new(0.34 * side, 0.12, 0.0), Vec3::new(0.08, 0.3, 0.08)); // brazo
    }
    add_box(&mut m, Vec3::new(0.0, 0.15, 0.0), Vec3::new(0.25, 0.32, 0.14)); // torso
    add_box(&mut m, Vec3::new(0.0, 0.62, 0.0), Vec3::new(0.12, 0.13, 0.12)); // cabeza
    m.compute_bounds();
    m
}

/// Ballena PROVISIONAL: un cuerpo en forma de huevo alargado hacia +Z (la cola) con aletas.
fn placeholder_whale() -> Mesh {
    let mut m = Mesh::icosphere(2, &|_| 1.0);
    for v in &mut m.vertices {
        // Más ancha adelante (-Z, la cabeza) que atrás.
        let taper = 1.0 - 0.35 * (v.position.z * 0.5 + 0.5);
        v.position = Vec3::new(v.position.x * 0.42 * taper, v.position.y * 0.36 * taper, v.position.z);
    }
    add_box(&mut m, Vec3::new(0.0, 0.0, 1.05), Vec3::new(0.45, 0.04, 0.12)); // cola
    add_box(&mut m, Vec3::new(0.0, -0.15, -0.2), Vec3::new(0.55, 0.03, 0.1)); // aletas
    m.recompute_normals();
    m.compute_bounds();
    m
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
    // "Arriba" = hacia afuera del planeta.
    oriented(pos, pos, point(t + 0.01) - pos)
}

/// Trayectoria de Flash en el marco de Central City: corre por el ecuador, con los pies
/// sobre la superficie ("arriba" = hacia afuera de la luna) y mirando hacia donde corre.
fn flash_path(t: f32) -> Mat4 {
    let r = CENTRAL_CITY_RADIUS + FLASH_SIZE * FEET;
    let a = FLASH_SPEED * t;
    let pos = Vec3::new(a.cos() * r, 0.0, a.sin() * r);
    // Derivada de (cos a, 0, sen a): hacia donde avanza al crecer el ángulo.
    oriented(pos, pos, Vec3::new(-a.sin(), 0.0, a.cos()))
}

/// Trayectoria de la ballena en el marco de Atlantis: una vuelta lenta que sube y baja,
/// con el lomo hacia afuera del planeta y mirando hacia donde nada.
fn whale_path(t: f32) -> Mat4 {
    let r = WHALE_ORBIT_RADII * ATLANTIS_RADIUS;
    let point = |t: f32| {
        let a = WHALE_SPEED * t;
        Vec3::new(a.cos() * r, WHALE_BOB * (t * WHALE_BOB_FREQ).sin(), a.sin() * r)
    };
    let pos = point(t);
    oriented(pos, pos, point(t + 0.05) - pos)
}

/// Aquaman va parado en el lomo de la ballena: SU trayectoria es la de la ballena por una
/// traslación hacia "arriba" de la ballena (jerarquía: si la ballena se mueve, él también).
fn aquaman_path(t: f32) -> Mat4 {
    whale_path(t) * Mat4::translation(Vec3::new(0.0, WHALE_BACK * WHALE_SIZE + AQUAMAN_SIZE * FEET, 0.0))
}

/// Linterna Verde en el marco de Oa: flota sobre LANTERN_SPOT, sube y baja y gira despacio
/// sobre su propio eje (que apunta hacia afuera del planeta).
fn lantern_path(t: f32) -> Mat4 {
    let up = LANTERN_SPOT.normalized();
    let pos = up * (LANTERN_HOVER_RADII * OA_RADIUS + LANTERN_BOB * (t * 1.5).sin());
    let side = up.cross(Vec3::new(1.0, 0.0, 0.0)).normalized();
    let turn = LANTERN_TURN * t;
    let forward = side * turn.cos() + up.cross(side) * turn.sin();
    oriented(pos, up, forward)
}

/// Índice de un cuerpo por su nombre.
fn body(scene: &Scene, name: &str) -> Option<usize> {
    scene.bodies.iter().position(|b| b.name == name)
}

/// Todos los personajes de la Liga cuyo mundo ya existe en la escena.
pub fn create_creatures(scene: &Scene) -> Vec<Creature> {
    let mut list = Vec::new();
    if let Some(krypton) = body(scene, "Krypton") {
        list.push(Creature {
            name: "Superman",
            home: krypton,
            lods: load_hero("superman", Mesh::placeholder_ship),
            shader: Arc::new(HeroShader { main: SUIT_BLUE, back: CAPE_RED, back_z: CAPE_Z }),
            path: superman_path,
            place: Mat4::identity(),
            size: SUPERMAN_SIZE,
            trail: Some(SUPERMAN_TRAIL),
            trail_step: 0.06,
            aura: None,
        });
    }
    if let Some(central_city) = body(scene, "Central City") {
        list.push(Creature {
            name: "Flash",
            home: central_city,
            lods: load_hero("flash", placeholder_figure),
            shader: Arc::new(HeroShader::solid(FLASH_RED)),
            path: flash_path,
            place: Mat4::identity(),
            size: FLASH_SIZE,
            // Estela larga y amarilla: puntos muy seguidos porque corre muy rápido.
            trail: Some(FLASH_TRAIL),
            trail_step: 0.03,
            aura: None,
        });
    }
    if let Some(gotham) = body(scene, "Gotham") {
        let (dir, roof) = highest_point(scene, gotham);
        list.push(Creature {
            name: "Batman",
            home: gotham,
            lods: load_hero("batman", placeholder_figure),
            // Traje gris adelante y capa negra atrás.
            shader: Arc::new(HeroShader { main: BATMAN_SUIT, back: BATMAN_CAPE, back_z: 0.05 }),
            path: still,
            place: standing_on(dir, roof, BATMAN_SIZE),
            size: BATMAN_SIZE,
            trail: None,
            trail_step: 0.0,
            aura: None,
        });
    }
    if let Some(atlantis) = body(scene, "Atlantis") {
        list.push(Creature {
            name: "Ballena",
            home: atlantis,
            lods: load_hero("ballena", placeholder_whale),
            shader: Arc::new(HeroShader::solid(WHALE_COLOR)),
            path: whale_path,
            place: Mat4::identity(),
            size: WHALE_SIZE,
            trail: None,
            trail_step: 0.0,
            aura: None,
        });
        list.push(Creature {
            name: "Aquaman",
            home: atlantis,
            lods: load_hero("aquaman", placeholder_figure),
            // Camisa naranja adelante, verde atrás (la figura provisional no trae colores).
            shader: Arc::new(HeroShader { main: AQUAMAN_ORANGE, back: AQUAMAN_GREEN, back_z: 0.05 }),
            path: aquaman_path,
            place: Mat4::identity(),
            size: AQUAMAN_SIZE,
            trail: None,
            trail_step: 0.0,
            aura: None,
        });
    }
    if let Some(oa) = body(scene, "Oa") {
        list.push(Creature {
            name: "Linterna Verde",
            home: oa,
            lods: load_hero("linterna_verde", placeholder_figure),
            shader: Arc::new(LanternShader),
            path: lantern_path,
            place: Mat4::identity(),
            size: LANTERN_SIZE,
            trail: None,
            trail_step: 0.0,
            aura: Some((LANTERN_GREEN, Mesh::icosphere(2, &|_| 1.0))),
        });
    }
    list
}

/// Las auras de luz (aditivas): se dibujan DESPUÉS de todo lo opaco, como el cometa.
pub fn draw_creature_glows(r: &mut Renderer, scene: &Scene, t: f32, creatures: &[Creature]) {
    for c in creatures {
        if let Some((color, mesh)) = &c.aura {
            let model = Mat4::translation(c.position(scene, t)) * Mat4::scale(Vec3::splat(c.size * LANTERN_AURA));
            r.draw_mesh_additive(mesh, &model, Arc::new(AuraShader { color: *color }));
        }
    }
}

/// Brillo que "respira": va de 0.4 a 1 con el tiempo.
fn pulse(time: f32) -> f32 {
    0.7 + 0.3 * (time * LANTERN_PULSE_SPEED).sin()
}

/// Linterna Verde: traje verde oscuro con Lambert + brillo verde EMISIVO que pulsa
/// (se ve aunque esté del lado oscuro del planeta).
struct LanternShader;

impl FragmentShader for LanternShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let l = (u.light_pos - f.world_pos).normalized();
        let diff = f.normal.dot(l).max(0.0);
        Vec3::new(0.03, 0.18, 0.06) * (0.3 + 0.7 * diff) + LANTERN_GREEN * (0.45 * pulse(u.time))
    }
}

/// Aura: brilla en el centro (donde la esfera mira a la cámara) y se desvanece en la orilla.
struct AuraShader {
    color: Vec3,
}

impl FragmentShader for AuraShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let v = (u.camera_pos - f.world_pos).normalized();
        let facing = f.normal.dot(v).abs();
        self.color * (facing.powf(3.0) * 0.45 * pulse(u.time))
    }
}

/// Dibuja a todos los personajes (con draw_lods: el culling y el LOD los cuentan) y sus estelas.
pub fn draw_creatures(r: &mut Renderer, scene: &Scene, t: f32, creatures: &[Creature]) {
    for c in creatures {
        r.draw_lods(&c.lods, &c.model(scene, t), c.shader.clone());
        if let Some(color) = c.trail {
            // Estela: las últimas TRAIL_POINTS posiciones. Como la trayectoria es una función del
            // tiempo, basta con evaluarla en t, t - paso, t - 2·paso... (no hay que guardar nada).
            let points: Vec<Vec3> = (0..TRAIL_POINTS).map(|k| c.position(scene, t - k as f32 * c.trail_step)).collect();
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

impl HeroShader {
    /// Un solo color en todo el cuerpo.
    fn solid(color: Vec3) -> HeroShader {
        HeroShader { main: color, back: color, back_z: f32::MAX }
    }
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

    fn find<'a>(list: &'a [Creature], name: &str) -> &'a Creature {
        list.iter().find(|c| c.name == name).unwrap()
    }

    #[test]
    fn superman_vuela_alrededor_de_krypton_a_1_6_radios() {
        let scene = Scene::new();
        let list = create_creatures(&scene);
        let sup = find(&list, "Superman");
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

    #[test]
    fn flash_corre_pegado_a_la_superficie_de_central_city() {
        let scene = Scene::new();
        let list = create_creatures(&scene);
        let flash = find(&list, "Flash");
        let radius = scene.bodies[flash.home].radius;
        assert!((radius - CENTRAL_CITY_RADIUS).abs() < 1e-6, "CENTRAL_CITY_RADIUS no coincide con scene.rs");
        for k in 0..40 {
            let t = k as f32 * 0.21;
            let d = (flash.position(&scene, t) - scene.body_position(flash.home, t)).length();
            assert!((d - (radius + FLASH_SIZE * FEET)).abs() < 1e-3);
        }
    }

    #[test]
    fn batman_esta_parado_en_la_azotea_mas_alta_y_gira_con_gotham() {
        let scene = Scene::new();
        let list = create_creatures(&scene);
        let bat = find(&list, "Batman");
        let (_, roof) = highest_point(&scene, bat.home);
        // Ningún vértice de Gotham está más alto que la azotea elegida.
        let radius = scene.bodies[bat.home].radius;
        assert!(scene.bodies[bat.home].lods[0].vertices.iter().all(|v| v.position.length() * radius <= roof + 1e-4));
        for t in [0.0, 7.0, 30.0] {
            let d = (bat.position(&scene, t) - scene.body_position(bat.home, t)).length();
            assert!((d - (roof + BATMAN_SIZE * FEET)).abs() < 1e-3);
        }
        // Gira con el planeta: en otro tiempo está en otro lugar del mundo.
        assert!((bat.position(&scene, 0.0) - bat.position(&scene, 5.0)).length() > 0.1);
    }

    #[test]
    fn aquaman_va_sobre_la_ballena_alrededor_de_atlantis() {
        let scene = Scene::new();
        let list = create_creatures(&scene);
        let (whale, aqua) = (find(&list, "Ballena"), find(&list, "Aquaman"));
        assert!((scene.bodies[whale.home].radius - ATLANTIS_RADIUS).abs() < 1e-6, "ATLANTIS_RADIUS no coincide");
        let ride = WHALE_BACK * WHALE_SIZE + AQUAMAN_SIZE * FEET;
        for k in 0..30 {
            let t = k as f32 * 0.9;
            let center = scene.body_position(whale.home, t);
            let w = whale.position(&scene, t);
            assert!(((w - center).length() - WHALE_ORBIT_RADII * ATLANTIS_RADIUS).abs() < WHALE_BOB + 0.05);
            // Aquaman siempre a la misma distancia de la ballena: va montado en ella.
            assert!(((aqua.position(&scene, t) - w).length() - ride).abs() < 1e-3);
        }
    }

    #[test]
    fn linterna_verde_flota_sobre_oa_con_su_aura() {
        let scene = Scene::new();
        let list = create_creatures(&scene);
        let gl = find(&list, "Linterna Verde");
        assert!((scene.bodies[gl.home].radius - OA_RADIUS).abs() < 1e-6, "OA_RADIUS no coincide con scene.rs");
        assert!(gl.aura.is_some());
        for k in 0..30 {
            let t = k as f32 * 0.7;
            let d = (gl.position(&scene, t) - scene.body_position(gl.home, t)).length();
            assert!((d - LANTERN_HOVER_RADII * OA_RADIUS).abs() <= LANTERN_BOB + 1e-3);
        }
    }

    #[test]
    fn las_caras_de_la_figura_miran_hacia_afuera() {
        // Cada triángulo de una caja debe tener su normal geométrica igual a la normal guardada.
        let m = placeholder_figure();
        for tri in m.indices.chunks_exact(3) {
            let (a, b, c) = (m.vertices[tri[0] as usize], m.vertices[tri[1] as usize], m.vertices[tri[2] as usize]);
            let n = (b.position - a.position).cross(c.position - a.position).normalized();
            assert!(n.dot(a.normal) > 0.99);
        }
    }
}

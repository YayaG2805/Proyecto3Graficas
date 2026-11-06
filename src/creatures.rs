//! PERSONAJES de la Liga que viven en los mundos (Diego, D5 y D6).
//!
//! Cada personaje tiene una trayectoria `path(t)` medida en el MARCO de su mundo
//! (`Scene::body_frame`: posición + inclinación + giro del planeta, sin escala).
//! Su matriz de modelo final es:  Model = body_frame(mundo) · place · path(t) · Escala
//!   - `place`: dónde está parado si no se mueve (Batman en su azotea); si se mueve es la identidad.
//!   - `path(t)`: su movimiento (Superman dando vueltas); si está quieto es la identidad.
//! Así el personaje acompaña a su planeta mientras éste orbita al sol, sin cálculos extra.
//!
//! Los modelos se cargan de assets/heroes/<nombre>.obj (los arma en Blender el script
//! assets/heroes/modelar_heroes.py) y se colorean por partes con una paleta (ver palette_color).
//! Mientras no exista el archivo se usa una figura provisional hecha con cajas.

use crate::math::{Mat4, Vec3};
use crate::mesh::{Mesh, Vertex};
use crate::noise::fbm;
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
/// Paletas: un color por banda, en el MISMO orden que en assets/heroes/modelar_heroes.py.
const SKIN: Vec3 = Vec3::new(0.93, 0.72, 0.58);
const YELLOW: Vec3 = Vec3::new(1.0, 0.82, 0.15);
const BLACK: Vec3 = Vec3::new(0.05, 0.05, 0.06);
/// Superman: azul, rojo (capa, botas), piel, amarillo (escudo, cinturón), negro (pelo).
const SUPERMAN_PALETTE: [Vec3; 5] = [Vec3::new(0.10, 0.25, 0.80), Vec3::new(0.85, 0.08, 0.06), SKIN, YELLOW, BLACK];
const SUPERMAN_TRAIL: Vec3 = Vec3::new(0.95, 0.25, 0.15);

// --- Flash (corre por el ecuador de Central City) ---
/// Radio de Central City (debe coincidir con scene.rs).
const CENTRAL_CITY_RADIUS: f32 = 1.2;
/// Radianes por segundo: da una vuelta a la luna en ~2 segundos.
pub const FLASH_SPEED: f32 = 3.0;
/// Tamaño de Flash (radio de su esfera envolvente).
const FLASH_SIZE: f32 = 0.24;
/// Flash: rojo, amarillo (rayo, botas, alitas), piel.
const FLASH_PALETTE: [Vec3; 3] = [Vec3::new(0.80, 0.06, 0.04), YELLOW, SKIN];
const FLASH_TRAIL: Vec3 = Vec3::new(1.0, 0.85, 0.15);

// --- Batman (quieto en la azotea más alta de Gotham) ---
/// Tamaño de Batman. Es más grande que un edificio para que se vea desde la nave.
const BATMAN_SIZE: f32 = 0.45;
/// Batman: gris, negro (capa, máscara, botas), amarillo (cinturón, óvalo), piel.
const BATMAN_PALETTE: [Vec3; 4] = [Vec3::new(0.42, 0.44, 0.52), BATMAN_CAPE, YELLOW, SKIN];
/// Capa azul muy oscuro (no negro puro): así la luz de borde dibuja su silueta.
const BATMAN_CAPE: Vec3 = Vec3::new(0.07, 0.08, 0.14);

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
const AQUAMAN_SIZE: f32 = 0.30;
/// Qué tan arriba del centro de la ballena va parado Aquaman (su lomo), en múltiplos de su
/// tamaño: el lomo del modelo está a ~0.21 de su radio envolvente (lo imprime modelar_heroes.py).
const WHALE_BACK: f32 = 0.21;
/// Ballena: azul oscuro, vientre claro, negro (ojos), cian (manchas del lomo).
const WHALE_PALETTE: [Vec3; 4] =
    [Vec3::new(0.20, 0.28, 0.45), Vec3::new(0.70, 0.76, 0.82), BLACK, Vec3::new(0.3, 0.95, 1.0)];
/// Aquaman: naranja, verde, piel, rubio, dorado (cinturón y tridente).
const AQUAMAN_PALETTE: [Vec3; 5] =
    [Vec3::new(0.95, 0.55, 0.10), Vec3::new(0.10, 0.55, 0.25), SKIN, Vec3::new(0.95, 0.80, 0.35), Vec3::new(1.0, 0.75, 0.2)];

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
const LANTERN_SIZE: f32 = 0.45;
const LANTERN_GREEN: Vec3 = Vec3::new(0.15, 1.0, 0.35);
/// Linterna Verde: verde (brilla), negro, piel, blanco (guantes, ojos, emblema).
const LANTERN_PALETTE: [Vec3; 4] = [Vec3::new(0.05, 0.45, 0.15), BLACK, SKIN, Vec3::new(0.95, 0.95, 0.95)];

// --- Mujer Maravilla (estatua gigante en Themyscira) ---
/// Tamaño de la estatua: mucho más grande que los demás, es un monumento.
const WONDER_WOMAN_SIZE: f32 = 1.2;
/// La estatua tiene pedestal: sus pies (la base) están a 0.80 de su radio bajo el centro.
const WONDER_WOMAN_FEET: f32 = 0.80;
const STATUE_STONE: Vec3 = Vec3::new(0.90, 0.88, 0.82);

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
/// `feet` = qué tan abajo del centro del modelo están sus pies, en múltiplos de su tamaño.
fn standing_on(dir: Vec3, radius: f32, size: f32, feet: f32) -> Mat4 {
    // Cualquier dirección perpendicular a "arriba" sirve como "adelante".
    let side = if dir.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
    oriented(dir * (radius + size * feet), dir, dir.cross(side))
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
            shader: Arc::new(HeroShader { palette: SUPERMAN_PALETTE.to_vec() }),
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
            shader: Arc::new(HeroShader { palette: FLASH_PALETTE.to_vec() }),
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
            // Colores por parte (paleta): traje gris, capa negra, cinturón amarillo, piel.
            shader: Arc::new(HeroShader { palette: BATMAN_PALETTE.to_vec() }),
            path: still,
            place: standing_on(dir, roof, BATMAN_SIZE, FEET),
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
            shader: Arc::new(HeroShader { palette: WHALE_PALETTE.to_vec() }),
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
            // Colores por parte (paleta): camisa naranja, pantalón verde, piel, pelo, tridente.
            shader: Arc::new(HeroShader { palette: AQUAMAN_PALETTE.to_vec() }),
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
    if let Some(themyscira) = body(scene, "Themyscira") {
        // En la cima más alta de las islas (donde irá el templo de Sando).
        let (dir, top) = highest_point(scene, themyscira);
        list.push(Creature {
            name: "Mujer Maravilla",
            home: themyscira,
            lods: load_hero("mujer_maravilla", placeholder_figure),
            shader: Arc::new(StoneShader),
            path: still,
            place: standing_on(dir, top, WONDER_WOMAN_SIZE, WONDER_WOMAN_FEET),
            size: WONDER_WOMAN_SIZE,
            trail: None,
            trail_step: 0.0,
            aura: None,
        });
    }
    list
}

/// Estatua: piedra blanca con Lambert y vetas suaves de ruido (como mármol).
struct StoneShader;

impl FragmentShader for StoneShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let veins = 0.9 + 0.1 * fbm(f.local_pos * 6.0, 3, 123);
        hero_light(STATUE_STONE * veins, f, u)
    }
}

/// Las auras de luz (aditivas): se dibujan DESPUÉS de todo lo opaco, como el cometa.
pub fn draw_creature_glows(r: &mut Renderer, scene: &Scene, t: f32, creatures: &[Creature]) {
    for c in creatures {
        if let Some((color, mesh)) = &c.aura {
            let model = Mat4::translation(c.position(scene, t)) * Mat4::scale(Vec3::splat(c.size * LANTERN_AURA));
            r.draw_mesh_additive(mesh, &model, Arc::new(AuraShader { color: *color }));
        }
    }
    if let Some(batman) = creatures.iter().find(|c| c.name == "Batman") {
        draw_bat_signal(r, scene, t, batman);
    }
}

/// Brillo que "respira": va de 0.4 a 1 con el tiempo.
fn pulse(time: f32) -> f32 {
    0.7 + 0.3 * (time * LANTERN_PULSE_SPEED).sin()
}

/// Linterna Verde: colores de su paleta con Lambert + brillo verde EMISIVO que pulsa en las
/// partes verdes (banda 0), así se ve aunque esté del lado oscuro del planeta.
struct LanternShader;

impl FragmentShader for LanternShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let base = palette_color(&LANTERN_PALETTE, f.uv.0);
        let glow = if f.uv.0 < 1.0 / LANTERN_PALETTE.len() as f32 { 0.45 * pulse(u.time) } else { 0.0 };
        hero_light(base, f, u) + LANTERN_GREEN * glow
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

/// Color de PALETA: en los modelos de modelar_heroes.py cada parte (traje, capa, piel...) tiene
/// su propia coordenada u = (banda + 0.5) / bandas. Aquí se hace al revés: banda = piso(u · bandas).
/// Las figuras provisionales traen u = 0, así que salen del primer color.
fn palette_color(palette: &[Vec3], u: f32) -> Vec3 {
    let band = ((u * palette.len() as f32).floor() as usize).min(palette.len() - 1);
    palette[band]
}

// --- Iluminación de los personajes ("de cine") ---
// Con solo la luz del sol, un héroe del lado de noche de su planeta sale negro. Por eso, además
// del sol, tienen dos luces más, como en una película:
//   - luz de RELLENO que sale de la cámara: ilumina lo que mira hacia nosotros (n·v);
//   - luz de BORDE: un contorno brillante donde la superficie se ve de lado ((1 − n·v)³),
//     que recorta la silueta contra el fondo oscuro.
/// Luz mínima en todos lados.
const HERO_AMBIENT: f32 = 0.18;
/// Fuerza de la luz de relleno (desde la cámara).
const HERO_FILL: f32 = 0.45;
/// Fuerza y color de la luz de borde.
const HERO_RIM: f32 = 0.7;
const HERO_RIM_COLOR: Vec3 = Vec3::new(0.65, 0.75, 1.0);

/// Sol (difuso + especular) + relleno + borde, para un color base dado.
fn hero_light(base: Vec3, f: &Fragment, u: &Uniforms) -> Vec3 {
    let l = (u.light_pos - f.world_pos).normalized();
    let v = (u.camera_pos - f.world_pos).normalized();
    let diff = f.normal.dot(l).max(0.0);
    let facing = f.normal.dot(v).max(0.0);
    let spec = f.normal.dot((l + v).normalized()).max(0.0).powf(24.0);
    let rim = (1.0 - facing).powf(3.0);
    base * (HERO_AMBIENT + 0.8 * diff + HERO_FILL * facing) + Vec3::splat(spec * 0.35) + HERO_RIM_COLOR * (rim * HERO_RIM)
}

/// Personaje con varios colores (paleta) con la iluminación de cine de arriba.
struct HeroShader {
    /// Un color por banda, en el mismo orden que en modelar_heroes.py.
    palette: Vec<Vec3>,
}

impl FragmentShader for HeroShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        hero_light(palette_color(&self.palette, f.uv.0), f, u)
    }
}

// --- BATISEÑAL: un haz de luz que sale de la azotea de Batman y termina en el círculo con el
// murciélago. Sirve para encontrarlo desde lejos. Se dibuja ADITIVO (es luz, se suma).
/// Largo del haz y radio del círculo del final (unidades del mundo).
const BAT_SIGNAL_LENGTH: f32 = 7.0;
const BAT_SIGNAL_RADIUS: f32 = 1.2;
/// Radio del haz donde sale, en fracción del radio del círculo.
const BAT_SIGNAL_BASE: f32 = 0.08;
/// Inclinación del haz respecto a la vertical de la azotea (radianes).
const BAT_SIGNAL_TILT: f32 = 0.35;
/// Color y brillo del haz y del círculo.
const BAT_SIGNAL_COLOR: Vec3 = Vec3::new(1.0, 0.86, 0.45);
const BAT_SIGNAL_BEAM: f32 = 0.30;
const BAT_SIGNAL_DISC: f32 = 0.85;

/// Cono abierto a lo largo de +Y: radio BAT_SIGNAL_BASE en y = 0 y radio 1 en y = 1.
fn beam_mesh(segments: usize) -> Mesh {
    let mut m = Mesh::default();
    for (y, r) in [(0.0, BAT_SIGNAL_BASE), (1.0, 1.0)] {
        for j in 0..segments {
            let a = j as f32 / segments as f32 * std::f32::consts::TAU;
            let position = Vec3::new(a.cos() * r, y, a.sin() * r);
            m.vertices.push(Vertex { position, normal: Vec3::ZERO, uv: (0.0, y), tangent: Vec3::ZERO });
        }
    }
    for j in 0..segments {
        let k = (j + 1) % segments;
        let (a, b, c, d) = (j as u32, k as u32, (segments + j) as u32, (segments + k) as u32);
        // Antihorario visto desde afuera del cono (lo que el back-face culling deja pasar).
        m.indices.extend([a, c, d, a, d, b]);
    }
    m.recompute_normals();
    m.compute_bounds();
    m
}

/// Disco de radio 1 en el plano XY (mira hacia +Z) hecho como un abanico de triángulos.
fn disc_mesh(segments: usize) -> Mesh {
    let mut m = Mesh::default();
    let n = Vec3::new(0.0, 0.0, 1.0);
    m.vertices.push(Vertex { position: Vec3::ZERO, normal: n, uv: (0.0, 0.0), tangent: Vec3::ZERO });
    for j in 0..segments {
        let a = j as f32 / segments as f32 * std::f32::consts::TAU;
        m.vertices.push(Vertex { position: Vec3::new(a.cos(), a.sin(), 0.0), normal: n, uv: (0.0, 0.0), tangent: Vec3::ZERO });
    }
    for j in 0..segments {
        m.indices.extend([0, 1 + j as u32, 1 + ((j + 1) % segments) as u32]);
    }
    m.compute_bounds();
    m
}

/// Dibuja la batiseñal de Batman: el haz (cono) y, en la punta, el círculo mirando a la cámara.
fn draw_bat_signal(r: &mut Renderer, scene: &Scene, t: f32, batman: &Creature) {
    // Las mallas se arman una sola vez y se guardan (OnceLock = "inicializar la primera vez").
    static MESHES: std::sync::OnceLock<(Mesh, Mesh)> = std::sync::OnceLock::new();
    let (beam, disc) = MESHES.get_or_init(|| (beam_mesh(24), disc_mesh(40)));

    // "Arriba" y "a un lado" de Batman salen de su propia matriz: así giran con Gotham.
    let model = batman.model(scene, t);
    let up = model.transform_dir(Vec3::UP).normalized();
    let side = model.transform_dir(Vec3::new(1.0, 0.0, 0.0)).normalized();
    let origin = batman.position(scene, t) + side * BATMAN_SIZE;
    let dir = (up * BAT_SIGNAL_TILT.cos() + side * BAT_SIGNAL_TILT.sin()).normalized();

    // Haz: el cono escalado (ancho = radio del círculo, alto = largo del haz).
    let beam_model = oriented(origin, dir, side) * Mat4::scale(Vec3::new(BAT_SIGNAL_RADIUS, BAT_SIGNAL_LENGTH, BAT_SIGNAL_RADIUS));
    r.draw_mesh_additive(beam, &beam_model, Arc::new(BeamShader { origin, dir }));

    // Círculo: en la punta del haz, de frente a la cámara (un "billboard"), con las orejas
    // del murciélago apuntando a lo largo del haz.
    let end = origin + dir * BAT_SIGNAL_LENGTH;
    let to_cam = r.uniforms.camera_pos - end;
    let disc_model = oriented(end, dir, -to_cam) * Mat4::scale(Vec3::new(BAT_SIGNAL_RADIUS, BAT_SIGNAL_RADIUS, 1.0));
    r.draw_mesh_additive(disc, &disc_model, Arc::new(BatDiscShader));
}

/// Haz de luz: más brillante al centro (donde lo vemos de frente) y se apaga hacia la punta.
/// La normal se reconstruye desde el eje del haz (la escala no uniforme deforma la de la malla).
struct BeamShader {
    origin: Vec3,
    dir: Vec3,
}

impl FragmentShader for BeamShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        let rel = f.world_pos - self.origin;
        let along = (rel.dot(self.dir) / BAT_SIGNAL_LENGTH).clamp(0.0, 1.0);
        let normal = (rel - self.dir * rel.dot(self.dir)).normalized();
        let v = (u.camera_pos - f.world_pos).normalized();
        let facing = normal.dot(v).abs();
        BAT_SIGNAL_COLOR * (BAT_SIGNAL_BEAM * facing * facing * (1.0 - 0.6 * along))
    }
}

/// ¿El punto (x, y) del círculo (radio 1, y hacia las orejas) cae dentro del murciélago?
/// Silueta hecha con desigualdades: cuerpo (elipse), cabeza con orejas y alas con el borde de
/// abajo ondulado (las "puntas" de las alas).
fn inside_bat(x: f32, y: f32) -> bool {
    let ax = x.abs();
    let body = (x / 0.16).powi(2) + (y / 0.32).powi(2) < 1.0;
    let head = ax < 0.12 && y > 0.1 && y < 0.36;
    let ears = ax > 0.05 && ax < 0.12 && y >= 0.36 && y < 0.46 - (ax - 0.08).abs();
    let wing_top = 0.12 + 0.35 * ax * ax; // sube hacia las puntas
    let wing_bottom = -0.05 - 0.12 * (ax * std::f32::consts::PI * 3.0).cos().abs() + 0.25 * ax;
    let wings = ax < 0.82 && y < wing_top && y > wing_bottom;
    body || head || ears || wings
}

/// Círculo de la batiseñal: luz amarilla con el borde suave y el murciélago en sombra.
struct BatDiscShader;

impl FragmentShader for BatDiscShader {
    fn shade(&self, f: &Fragment, _u: &Uniforms) -> Vec3 {
        let (x, y) = (f.local_pos.x, f.local_pos.y);
        let d = (x * x + y * y).sqrt();
        if inside_bat(x / 0.85, y / 0.85) {
            return Vec3::ZERO; // aditivo: cero = no agrega luz → se ve la silueta oscura
        }
        let edge = 1.0 - smoothstep(0.85, 1.0, d);
        BAT_SIGNAL_COLOR * (BAT_SIGNAL_DISC * edge)
    }
}

/// Transición suave de 0 a 1 entre a y b.
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    fn la_estatua_esta_en_la_cima_de_themyscira() {
        let scene = Scene::new();
        let list = create_creatures(&scene);
        let ww = find(&list, "Mujer Maravilla");
        let (_, top) = highest_point(&scene, ww.home);
        // La cima está sobre el nivel del mar (radio 4) y la estatua parada encima.
        assert!(top > scene.bodies[ww.home].radius);
        let d = (ww.position(&scene, 3.0) - scene.body_position(ww.home, 3.0)).length();
        assert!((d - (top + WONDER_WOMAN_SIZE * WONDER_WOMAN_FEET)).abs() < 1e-3);
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

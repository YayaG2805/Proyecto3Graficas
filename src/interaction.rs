//! todo lo q pasa entre la nave y los mundos: cual esta mas cerca, escanear, elegir destino...
//!
//! main.rs llama a `update` una vez por cuadro y el hud solo lee lo q queda en `GameState`.

use crate::math::{Mat4, Vec3};
use crate::mesh::{flat_mesh, Mesh};
use crate::noise::hash01;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use crate::shaders::{HazardShader, MarkerShader, RaceGateShader};
use crate::ship::{Ship, ShipInput};
use std::sync::Arc;

// se puede escanear un mundo si estas a menos de esta cantidad de radios de su superficie
const SCAN_RANGE_RADII: f32 = 3.0;
// se puede enganchar la orbita si estas a menos de esta cantidad de radios de la superficie
const ORBIT_RANGE_RADII: f32 = 2.5;
// el primer mundo q se elige como destino (bodies[0] es el sol)
const FIRST_WORLD: usize = 1;

// --- cristales coleccionables ---
// misma semilla = mismos lugares, se puede cambiar en vivo
const COLLECTIBLE_SEED: u32 = 71;
const COLLECTIBLES_PER_WORLD: usize = 2;
// se recoge al pasar a menos de esta distancia de la nave
const PICKUP_RADIUS: f32 = 1.5;
// a q distancia del centro del mundo flotan: radio * esto + lo otro
// asi siempre quedan afuera de la colision y se alcanzan volando o en modo orbita
const RING_RADII: f32 = 1.5;
const RING_EXTRA: f32 = 1.5;
// cuanto se pueden salir del ecuador, en radianes, para q no queden todos en la misma linea
const RING_MAX_TILT: f32 = 0.9;
// tamaño y giro de cada cristal
const CRYSTAL_SIZE: f32 = 0.7;
const CRYSTAL_SPIN: f32 = 1.5;
// colores y la banda de luz q sube por el cristal
const CRYSTAL_COLOR: Vec3 = Vec3 { x: 1.0, y: 0.8, z: 0.25 };
const CRYSTAL_RIM_COLOR: Vec3 = Vec3 { x: 1.0, y: 1.0, z: 0.85 };
const CRYSTAL_BAND_FREQ: f32 = 6.0;
const CRYSTAL_BAND_SPEED: f32 = 4.0;

// --- minijuegos (tecla R): hay q estar en orbita (F) de un mundo pa poder jugarlos ---
// cada mundo tiene un reto DISTINTO, no el mismo juego repintado: Themyscira y Oa cruzan aros
// en orden (Oa con reloj por puerta), Krypton hay q sobrevivir esquivando peligros, y Gotham /
// Atlantis hay q tocar todos los objetivos sueltos (Atlantis los tiene en movimiento).
// misma semilla = mismo circuito/peligros/objetivos, se puede cambiar en vivo
const RACE_SEED: u32 = 83;
// cuantos aros tiene un mundo q no esta en `make_world_games` (no debería pasar, es el respaldo)
const RACE_GATE_COUNT: usize = 6;
// a q distancia del centro del mundo va todo esto: radio * esto + lo otro, bastante mas lejos
// q los cristales pa q quepa rodeando el planeta sin rozar la superficie
const RACE_RING_RADII: f32 = 2.2;
const RACE_RING_EXTRA: f32 = 1.5;
// q tan inclinado va el eje del circuito/anillo respecto a +y, pa q cada mundo se vea distinto
const RACE_AXIS_TILT: f32 = 0.5;
// tamaño del aro sin escalar: radio del circulo central y grosor del tubo
const RACE_GATE_MAJOR: f32 = 1.0;
const RACE_GATE_MINOR: f32 = 0.16;
// el aro mide esto * el radio del planeta
const RACE_GATE_SCALE_RADII: f32 = 0.5;
// cuanto margen de mas tiene el agujero de un aro pa contar como "pasada"
const RACE_PASS_LENIENCY: f32 = 1.5;
// q tan grandes son los peligros (Krypton) y los objetivos (Gotham/Atlantis), en radios del mundo
const HAZARD_RADIUS_RADII: f32 = 0.22;
const TARGET_RADIUS_RADII: f32 = 0.3;
// q tanto se mecen los objetivos moviles de Atlantis, en radios del mundo
const TARGET_MOVE_RADII: f32 = 0.5;
// cuanto dura el mensaje de resultado en el hud
const RACE_RESULT_SECONDS: f32 = 6.0;

// un vector cualquiera en angulo recto con v, pa armar una base ortonormal
fn perpendicular(v: Vec3) -> Vec3 {
    let axis = if v.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
    v.cross(axis).normalized()
}

// el eje (inclinado distinto en cada mundo) y la base ortonormal q usan los 3 minijuegos pa
// repartir cosas alrededor del planeta; sale de la semilla, asi q es reproducible
fn world_axis(body: usize, seed: u32) -> (Vec3, Vec3, Vec3) {
    let polar = hash01(body as i32, 20, 0, seed) * RACE_AXIS_TILT;
    let azimuth = hash01(body as i32, 21, 0, seed) * std::f32::consts::TAU;
    let axis = Vec3::new(polar.sin() * azimuth.cos(), polar.cos(), polar.sin() * azimuth.sin()).normalized();
    let t1 = perpendicular(axis);
    let t2 = axis.cross(t1);
    (axis, t1, t2)
}

// una puerta del circuito: offset fijo desde el centro del mundo (no rota con el giro del
// planeta, igual q los cristales) y como esta orientada: la nave la atraviesa por el agujero,
// en la direccion `forward` (tangente al circuito, por donde se vuela rodeando el planeta)
#[derive(Clone, Copy)]
struct RaceGate {
    offset: Vec3,
    right: Vec3,
    forward: Vec3,
    scale: f32,
    radius: f32,
    color: Vec3,
}

/// arma un circuito de aros: el radio varia con `sin(lobes * angulo)` (0 = circulo perfecto,
/// mas lobulos = silueta de estrella o de flor), asi cada mundo se ve distinto sin dejar de
/// ser 100% procedural
fn make_gate_ring(scene: &Scene, body: usize, seed: u32, gate_count: usize, lobes: f32, wobble: f32, color: Vec3) -> Vec<RaceGate> {
    let b = &scene.bodies[body];
    let (axis, t1, t2) = world_axis(body, seed);
    let base_ring = b.radius * RACE_RING_RADII + RACE_RING_EXTRA;
    let scale = b.radius * RACE_GATE_SCALE_RADII;
    (0..gate_count)
        .map(|k| {
            let a = k as f32 / gate_count as f32 * std::f32::consts::TAU;
            let shape = (lobes * a).sin();
            let shape_derivative = lobes * (lobes * a).cos();
            let r = base_ring * (1.0 + wobble * shape);
            let dr = base_ring * wobble * shape_derivative;
            let radial = t1 * a.cos() + t2 * a.sin();
            let radial_perp = t2 * a.cos() - t1 * a.sin();
            let offset = radial * r;
            // la tangente real del circuito (incluyendo q el radio tambien cambia), por donde
            // hay q volar pa atravesar la puerta de frente
            let forward = (radial_perp * r + radial * dr).normalized();
            let right = forward.cross(axis).normalized();
            RaceGate { offset, right, forward, scale, radius: scale * (RACE_GATE_MAJOR - RACE_GATE_MINOR) * RACE_PASS_LENIENCY, color }
        })
        .collect()
}

/// reparte peligros fijos alrededor del mundo, repartidos con hash (no parejo como los aros,
/// pa q la ruta mas facil no sea obvia)
fn make_hazards(scene: &Scene, body: usize, seed: u32, count: usize) -> Vec<Vec3> {
    let b = &scene.bodies[body];
    let ring = b.radius * RACE_RING_RADII + RACE_RING_EXTRA;
    (0..count)
        .map(|k| {
            let angle = hash01(body as i32, 30 + k as i32, 0, seed) * std::f32::consts::TAU;
            let tilt = (hash01(body as i32, 30 + k as i32, 1, seed) - 0.5) * 1.2;
            Vec3::new(angle.cos() * tilt.cos(), tilt.sin(), angle.sin() * tilt.cos()) * ring
        })
        .collect()
}

// un objetivo pa tocar (Gotham/Atlantis): si `move_amount` es 0 se queda quieto, si no se mece
// de un lado a otro con el tiempo (una corriente, una patrulla que no para)
#[derive(Clone, Copy)]
struct CollectTarget {
    base_offset: Vec3,
    move_perp: Vec3,
    move_amount: f32,
    move_speed: f32,
    move_phase: f32,
    radius: f32,
}

impl CollectTarget {
    fn offset(&self, t: f32) -> Vec3 {
        self.base_offset + self.move_perp * (t * self.move_speed + self.move_phase).sin() * self.move_amount
    }
}

/// reparte objetivos pa tocar; si `moving` cada uno se mece con su propia fase y velocidad
fn make_targets(scene: &Scene, body: usize, seed: u32, count: usize, moving: bool) -> Vec<CollectTarget> {
    let b = &scene.bodies[body];
    let ring = b.radius * RACE_RING_RADII + RACE_RING_EXTRA;
    (0..count)
        .map(|k| {
            let angle = hash01(body as i32, 40 + k as i32, 0, seed) * std::f32::consts::TAU;
            let tilt = (hash01(body as i32, 40 + k as i32, 1, seed) - 0.5) * 1.2;
            let dir = Vec3::new(angle.cos() * tilt.cos(), tilt.sin(), angle.sin() * tilt.cos());
            CollectTarget {
                base_offset: dir * ring,
                move_perp: if moving { perpendicular(dir) } else { Vec3::ZERO },
                move_amount: if moving { b.radius * TARGET_MOVE_RADII } else { 0.0 },
                move_speed: 0.5 + hash01(body as i32, 40 + k as i32, 2, seed) * 0.6,
                move_phase: hash01(body as i32, 40 + k as i32, 3, seed) * std::f32::consts::TAU,
                radius: b.radius * TARGET_RADIUS_RADII,
            }
        })
        .collect()
}

/// el reto de un mundo: la configuracion no cambia una vez armada (ver `make_world_games`)
struct WorldGame {
    /// como se llama el reto, pa el aviso del hud ("[R] <nombre>")
    name: &'static str,
    kind: WorldGameKind,
}

enum WorldGameKind {
    /// cruzar los aros en orden; si `gate_deadline` no es None hay q cruzar cada uno antes de
    /// q se acaben esos segundos desde q se volvio el siguiente, o se falla
    Gates { gates: Vec<RaceGate>, gate_deadline: Option<f32> },
    /// no tocar ninguno de los peligros durante `goal_seconds`
    Survive { hazards: Vec<Vec3>, hazard_radius: f32, goal_seconds: f32 },
    /// tocar todos los objetivos (en cualquier orden) antes de q se acabe `time_limit`
    Collect { targets: Vec<CollectTarget>, time_limit: f32, color: Vec3 },
}

/// arma el reto de cada mundo, pensado pa q se sienta distinto segun su héroe:
/// Themyscira es el mas facil (cruzar aros en círculo, sin reloj, para aprender), Krypton hay
/// q sobrevivir esquivando cristales inestables, Gotham hay q tocar las señales de la
/// patrulla, Atlantis las mismas pero a la deriva con la corriente, y Oa (el anillo de poder)
/// cruza aros contra el reloj, cada uno con su propia cuenta regresiva
fn make_world_games(scene: &Scene, seed: u32) -> Vec<Option<WorldGame>> {
    scene
        .bodies
        .iter()
        .enumerate()
        .map(|(i, b)| {
            // el sol y las lunas no tienen reto, igual q los cristales
            if i < FIRST_WORLD || b.parent.is_some() {
                return None;
            }
            Some(match b.name {
                "Themyscira" => WorldGame {
                    name: "circuito de entrenamiento",
                    kind: WorldGameKind::Gates { gates: make_gate_ring(scene, i, seed, 6, 0.0, 0.0, Vec3::new(0.55, 0.9, 0.5)), gate_deadline: None },
                },
                "Krypton" => WorldGame {
                    name: "esquiva de cristales",
                    kind: WorldGameKind::Survive { hazards: make_hazards(scene, i, seed, 10), hazard_radius: b.radius * HAZARD_RADIUS_RADII, goal_seconds: 18.0 },
                },
                "Gotham" => WorldGame {
                    name: "patrulla nocturna",
                    kind: WorldGameKind::Collect { targets: make_targets(scene, i, seed, 6, false), time_limit: 25.0, color: Vec3::new(1.0, 0.85, 0.25) },
                },
                "Atlantis" => WorldGame {
                    name: "corrientes de Atlantis",
                    kind: WorldGameKind::Collect { targets: make_targets(scene, i, seed, 6, true), time_limit: 28.0, color: Vec3::new(0.2, 0.75, 0.95) },
                },
                "Oa" => WorldGame {
                    name: "anillos de energia",
                    kind: WorldGameKind::Gates { gates: make_gate_ring(scene, i, seed, 8, 6.0, 0.22, Vec3::new(0.35, 1.0, 0.45)), gate_deadline: Some(4.0) },
                },
                _ => WorldGame {
                    name: "circuito de anillos",
                    kind: WorldGameKind::Gates { gates: make_gate_ring(scene, i, seed, RACE_GATE_COUNT, 0.0, 0.0, Vec3::new(1.0, 0.35, 0.65)), gate_deadline: None },
                },
            })
        })
        .collect()
}

// revisa si la nave cruzo el punto `target` en este cuadro: reviso todo el tramo q recorrio,
// no solo donde quedo, pa no saltarnos nada a mucha velocidad o con pocos fps (mismo truco de
// pick_up). Sirve pa "pasar por un aro", "tocar un objetivo" y "chocar con un peligro".
fn crossed_this_frame(ship: &Ship, dt: f32, target: Vec3, radius: f32) -> bool {
    let previous = ship.position - ship.velocity * dt;
    let path = ship.position - previous;
    let length_squared = path.dot(path).max(1e-6);
    let along = ((target - previous).dot(path) / length_squared).clamp(0.0, 1.0);
    let closest = previous + path * along;
    (target - closest).length() < radius
}

// el minijuego en curso: cada variante trae el estado q le hace falta a su mecanica
enum ActiveMinigame {
    Gates { body: usize, next: usize, elapsed: f32, gate_elapsed: f32 },
    Survive { body: usize, elapsed: f32 },
    Collect { body: usize, touched: Vec<bool>, elapsed: f32, remaining: f32 },
}

impl ActiveMinigame {
    fn body(&self) -> usize {
        match self {
            ActiveMinigame::Gates { body, .. } => *body,
            ActiveMinigame::Survive { body, .. } => *body,
            ActiveMinigame::Collect { body, .. } => *body,
        }
    }

    fn start(body: usize, game: &WorldGame) -> ActiveMinigame {
        match &game.kind {
            WorldGameKind::Gates { .. } => ActiveMinigame::Gates { body, next: 0, elapsed: 0.0, gate_elapsed: 0.0 },
            WorldGameKind::Survive { .. } => ActiveMinigame::Survive { body, elapsed: 0.0 },
            WorldGameKind::Collect { targets, time_limit, .. } => {
                ActiveMinigame::Collect { body, touched: vec![false; targets.len()], elapsed: 0.0, remaining: *time_limit }
            }
        }
    }
}

/// lo q paso al avanzar un cuadro: listo (sigue jugandose) o terminado (con si se gano y el
/// mensaje pal hud)
enum StepOutcome {
    Ongoing,
    Done { success: bool, text: String },
}

fn step_gates(ship: &Ship, scene: &Scene, body: usize, gates: &[RaceGate], deadline: Option<f32>, next: &mut usize, elapsed: &mut f32, gate_elapsed: &mut f32, t: f32, dt: f32) -> StepOutcome {
    *elapsed += dt;
    *gate_elapsed += dt;
    if let Some(limit) = deadline {
        if *gate_elapsed > limit {
            return StepOutcome::Done { success: false, text: format!("se apago el aro {}: intentalo de nuevo", *next + 1) };
        }
    }
    if let Some(gate) = gates.get(*next) {
        let target = scene.body_position(body, t) + gate.offset;
        if crossed_this_frame(ship, dt, target, gate.radius) {
            *next += 1;
            *gate_elapsed = 0.0;
        }
    }
    if *next >= gates.len() {
        StepOutcome::Done { success: true, text: format!("circuito completo en {:.1}s", *elapsed) }
    } else {
        StepOutcome::Ongoing
    }
}

fn step_survive(ship: &Ship, scene: &Scene, body: usize, hazards: &[Vec3], hazard_radius: f32, goal: f32, elapsed: &mut f32, t: f32, dt: f32) -> StepOutcome {
    *elapsed += dt;
    let center = scene.body_position(body, t);
    for h in hazards {
        if crossed_this_frame(ship, dt, center + *h, hazard_radius) {
            return StepOutcome::Done { success: false, text: "chocaste con un cristal inestable: intentalo de nuevo".to_string() };
        }
    }
    if *elapsed >= goal {
        StepOutcome::Done { success: true, text: format!("sobreviviste {goal:.0}s esquivando los cristales") }
    } else {
        StepOutcome::Ongoing
    }
}

fn step_collect(ship: &Ship, scene: &Scene, body: usize, targets: &[CollectTarget], touched: &mut [bool], elapsed: &mut f32, remaining: &mut f32, t: f32, dt: f32) -> StepOutcome {
    *elapsed += dt;
    *remaining -= dt;
    let center = scene.body_position(body, t);
    for (k, target) in targets.iter().enumerate() {
        if !touched[k] && crossed_this_frame(ship, dt, center + target.offset(t), target.radius) {
            touched[k] = true;
        }
    }
    if touched.iter().all(|&done| done) {
        StepOutcome::Done { success: true, text: format!("tocaste todos los objetivos en {:.1}s", *elapsed) }
    } else if *remaining <= 0.0 {
        StepOutcome::Done { success: false, text: "se acabo el tiempo: faltaron objetivos por tocar".to_string() }
    } else {
        StepOutcome::Ongoing
    }
}

/// el resultado del ultimo minijuego terminado, pa mostrarlo un rato en el hud
#[derive(Clone)]
pub struct MinigameResult {
    pub body: usize,
    pub success: bool,
    pub text: String,
}

/// lo q el hud necesita pa mostrar el progreso del minijuego en curso
pub enum MinigameHud {
    Gates { next: usize, total: usize, elapsed: f32, gate_left: Option<f32> },
    Survive { elapsed: f32, goal: f32 },
    Collect { touched: usize, total: usize, remaining: f32 },
}

// un cristal flotando cerca de un mundo
struct Collectible {
    /// indice del mundo con el q se mueve
    body: usize,
    /// donde esta respecto al centro de ese mundo
    offset: Vec3,
    taken: bool,
}

// reparte los cristales alrededor de cada mundo, sin nada escrito a mano
fn make_collectibles(scene: &Scene, seed: u32) -> Vec<Collectible> {
    let mut crystals = Vec::new();
    for (i, b) in scene.bodies.iter().enumerate() {
        // el sol y las lunas no llevan cristales
        if i < FIRST_WORLD || b.parent.is_some() {
            continue;
        }
        let ring = b.radius * RING_RADII + RING_EXTRA;

        // paso 1: el hash da un angulo fijo pa cada mundo segun la semilla
        let start = hash01(i as i32, 0, 0, seed) * std::f32::consts::TAU;
        for k in 0..COLLECTIBLES_PER_WORLD {
            // paso 2: los cristales del mismo mundo quedan repartidos parejo en el anillo
            let angle = start + k as f32 / COLLECTIBLES_PER_WORLD as f32 * std::f32::consts::TAU;

            // paso 3: cada uno se sale un poco del ecuador, tambien con hash
            let tilt = (hash01(i as i32, k as i32 + 1, 0, seed) - 0.5) * RING_MAX_TILT;

            // paso 4: de angulos a un punto en la esfera, igual q en un mapa con longitud y latitud
            let dir = Vec3::new(angle.cos() * tilt.cos(), tilt.sin(), angle.sin() * tilt.cos());
            crystals.push(Collectible { body: i, offset: dir * ring, taken: false });
        }
    }
    crystals
}

// la forma del cristal: un octaedro, dos piramides pegadas por la base
fn crystal_mesh() -> Mesh {
    // los 4 de en medio dan la vuelta y luego van la punta de arriba y la de abajo
    let points = [
        Vec3::new(0.6, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.6),
        Vec3::new(-0.6, 0.0, 0.0),
        Vec3::new(0.0, 0.0, -0.6),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
    ];
    let mut triangles = Vec::new();
    for a in 0..4 {
        let b = (a + 1) % 4;
        // el orden de los puntos decide cual cara se ve, tiene q ser antihorario visto desde afuera
        triangles.push([4, b, a]);
        triangles.push([5, a, b]);
    }
    flat_mesh(&points, &triangles)
}

// el shader de los cristales: brillan solos con una banda de luz q les va subiendo
struct CrystalShader;

impl FragmentShader for CrystalShader {
    fn shade(&self, f: &Fragment, u: &Uniforms) -> Vec3 {
        // paso 1: la banda, local_pos.y va de abajo a arriba del cristal y time la hace avanzar
        let band = (f.local_pos.y * CRYSTAL_BAND_FREQ - u.time * CRYSTAL_BAND_SPEED).sin() * 0.5 + 0.5;

        // paso 2: las caras q miran a la camara se ven mas llenas y los bordes mas claritos
        let to_camera = (u.camera_pos - f.world_pos).normalized();
        let facing = f.normal.dot(to_camera).abs();
        let rim = (1.0 - facing).powf(2.0);

        // paso 3: no uso la luz del sol, asi se ven tambien del lado oscuro de un planeta
        CRYSTAL_COLOR * ((0.35 + 0.65 * band) * (0.5 + 0.5 * facing)) + CRYSTAL_RIM_COLOR * rim
    }
}

// el allow se quita cuando el hud (d3) y las siguientes tareas lean los campos q faltan
#[allow(dead_code)]
pub struct GameState {
    /// cuerpo mas cercano a la nave
    pub nearest: Option<usize>,
    /// distancia de la nave a la superficie de ese cuerpo
    pub nearest_dist: f32,
    /// la ficha q se muestra en el hud
    pub scanned: Option<usize>,
    /// a q mundo se va a saltar con el warp
    pub warp_target: usize,
    /// intensidad del efecto warp, de 0 a 1
    pub warp: f32,
    /// el cuerpo al q estamos enganchados en modo orbita
    pub orbit_lock: Option<usize>,
    pub collected: u32,
    pub total: u32,
    pub photo_mode: bool,
    // si shift venia apretado el cuadro pasado, pa saber cuando se aprieta de nuevo
    boost_was_down: bool,
    // los cristales y lo q comparten todos pa dibujarse
    collectibles: Vec<Collectible>,
    crystal_lods: Vec<Mesh>,
    crystal_shader: Arc<dyn FragmentShader>,

    // --- minijuegos por mundo ---
    /// el minijuego en curso, None = no se esta jugando ninguno
    active: Option<ActiveMinigame>,
    /// el reto de cada mundo (None en el sol y las lunas)
    world_games: Vec<Option<WorldGame>>,
    /// mejor marca lograda en cada mundo, None = todavia no se completa ninguna
    pub race_best: Vec<Option<f32>>,
    /// el resultado del ultimo minijuego terminado, pa mostrarlo un rato en el hud
    pub race_result: Option<MinigameResult>,
    race_result_timer: f32,
    // mallas q comparten todos los minijuegos pa dibujarse: el aro (Gates) y el marcador
    // generico (Survive/Collect), distinguidos solo por su shader y su color
    gate_mesh: Mesh,
    marker_mesh: Mesh,
}

impl GameState {
    pub fn new(scene: &Scene) -> GameState {
        let collectibles = make_collectibles(scene, COLLECTIBLE_SEED);
        GameState {
            nearest: None,
            nearest_dist: f32::MAX,
            scanned: None,
            warp_target: FIRST_WORLD,
            warp: 0.0,
            orbit_lock: None,
            collected: 0,
            total: collectibles.len() as u32,
            photo_mode: false,
            boost_was_down: false,
            collectibles,
            crystal_lods: vec![crystal_mesh()],
            crystal_shader: Arc::new(CrystalShader),
            active: None,
            world_games: make_world_games(scene, RACE_SEED),
            race_best: vec![None; scene.bodies.len()],
            race_result: None,
            race_result_timer: 0.0,
            gate_mesh: Mesh::torus(RACE_GATE_MAJOR, RACE_GATE_MINOR, 24, 10),
            marker_mesh: crystal_mesh(),
        }
    }

    /// si el mundo i tiene un reto (el sol y las lunas no), y como se llama, pa el aviso del hud
    pub fn minigame_name(&self, body: usize) -> Option<&'static str> {
        self.world_games[body].as_ref().map(|g| g.name)
    }

    /// lo q el hud necesita pa mostrar el minijuego en curso
    pub fn minigame_hud(&self) -> Option<(usize, MinigameHud)> {
        let active = self.active.as_ref()?;
        let body = active.body();
        match (active, &self.world_games[body].as_ref()?.kind) {
            (ActiveMinigame::Gates { next, elapsed, gate_elapsed, .. }, WorldGameKind::Gates { gates, gate_deadline }) => {
                Some((body, MinigameHud::Gates { next: *next, total: gates.len(), elapsed: *elapsed, gate_left: gate_deadline.map(|d| (d - gate_elapsed).max(0.0)) }))
            }
            (ActiveMinigame::Survive { elapsed, .. }, WorldGameKind::Survive { goal_seconds, .. }) => Some((body, MinigameHud::Survive { elapsed: *elapsed, goal: *goal_seconds })),
            (ActiveMinigame::Collect { touched, remaining, .. }, WorldGameKind::Collect { .. }) => {
                Some((body, MinigameHud::Collect { touched: touched.iter().filter(|&&x| x).count(), total: touched.len(), remaining: (*remaining).max(0.0) }))
            }
            _ => None,
        }
    }

    // cuando el minijuego termina, guarda la mejor marca: tiempo mas bajo en Gates/Collect,
    // mas tiempo sobrevivido en Survive (cuenta aunque se falle, asi anima a volver a intentar)
    fn update_best(&mut self, body: usize, active: &ActiveMinigame, success: bool) {
        let metric = match active {
            ActiveMinigame::Gates { elapsed, .. } if success => Some((*elapsed, true)),
            ActiveMinigame::Collect { elapsed, .. } if success => Some((*elapsed, true)),
            ActiveMinigame::Survive { elapsed, .. } => Some((*elapsed, false)),
            _ => None,
        };
        let Some((value, lower_is_better)) = metric else { return };
        let better = match self.race_best[body] {
            None => true,
            Some(best) => {
                if lower_is_better {
                    value < best
                } else {
                    value > best
                }
            }
        };
        if better {
            self.race_best[body] = Some(value);
        }
    }

    /// cambia el minijuego segun la tecla R y el estado de la orbita, y avanza el q este en curso
    fn update_minigame(&mut self, ship: &Ship, scene: &Scene, toggle: bool, t: f32, dt: f32) {
        if self.race_result_timer > 0.0 {
            self.race_result_timer -= dt;
            if self.race_result_timer <= 0.0 {
                self.race_result = None;
            }
        }

        // si se solto la orbita de ese mundo (F, warp, explosion...) el minijuego se cancela solo
        if let Some(active) = &self.active {
            if ship.orbit_body() != Some(active.body()) {
                self.active = None;
            }
        }

        // R: si ya estamos jugando lo cancela; si no, lo empieza (solo estando en orbita de un
        // mundo con reto, el sol y las lunas no tienen)
        if toggle {
            self.active = if self.active.is_some() {
                None
            } else {
                ship.orbit_body().and_then(|body| self.world_games[body].as_ref().map(|game| ActiveMinigame::start(body, game)))
            };
        }

        let Some(mut active) = self.active.take() else { return };
        let body = active.body();
        let outcome = match (&mut active, &self.world_games[body].as_ref().expect("un minijuego activo siempre tiene su reto").kind) {
            (ActiveMinigame::Gates { next, elapsed, gate_elapsed, .. }, WorldGameKind::Gates { gates, gate_deadline }) => {
                step_gates(ship, scene, body, gates, *gate_deadline, next, elapsed, gate_elapsed, t, dt)
            }
            (ActiveMinigame::Survive { elapsed, .. }, WorldGameKind::Survive { hazards, hazard_radius, goal_seconds }) => {
                step_survive(ship, scene, body, hazards, *hazard_radius, *goal_seconds, elapsed, t, dt)
            }
            (ActiveMinigame::Collect { touched, elapsed, remaining, .. }, WorldGameKind::Collect { targets, .. }) => step_collect(ship, scene, body, targets, touched, elapsed, remaining, t, dt),
            _ => StepOutcome::Ongoing,
        };

        match outcome {
            StepOutcome::Done { success, text } => {
                self.update_best(body, &active, success);
                self.race_result = Some(MinigameResult { body, success, text });
                self.race_result_timer = RACE_RESULT_SECONDS;
                self.active = None;
            }
            StepOutcome::Ongoing => self.active = Some(active),
        }
    }

    /// dibuja lo q falte del minijuego en curso, va en Game::draw_geometry
    pub fn draw_race(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let Some(active) = &self.active else { return };
        let body = active.body();
        let Some(game) = &self.world_games[body] else { return };
        let center = scene.body_position(body, t);
        match (active, &game.kind) {
            (ActiveMinigame::Gates { next, .. }, WorldGameKind::Gates { gates, .. }) => {
                for (k, gate) in gates.iter().enumerate().skip(*next) {
                    let up = gate.right.cross(gate.forward);
                    let model = Mat4::from_basis(gate.right, up, gate.forward, center + gate.offset) * Mat4::scale(Vec3::splat(gate.scale));
                    let shader: Arc<dyn FragmentShader> = Arc::new(RaceGateShader { color: gate.color, next: k == *next });
                    r.draw_mesh_additive(&self.gate_mesh, &model, shader);
                }
            }
            (ActiveMinigame::Survive { .. }, WorldGameKind::Survive { hazards, hazard_radius, .. }) => {
                for h in hazards {
                    let model = Mat4::translation(center + *h) * Mat4::rotation_y(t * 1.5) * Mat4::scale(Vec3::splat(*hazard_radius));
                    r.draw_mesh_additive(&self.marker_mesh, &model, Arc::new(HazardShader));
                }
            }
            (ActiveMinigame::Collect { touched, .. }, WorldGameKind::Collect { targets, color, .. }) => {
                for (k, target) in targets.iter().enumerate() {
                    if touched[k] {
                        continue;
                    }
                    let model = Mat4::translation(center + target.offset(t)) * Mat4::rotation_y(t * 2.0) * Mat4::scale(Vec3::splat(target.radius * 0.6));
                    r.draw_mesh_additive(&self.marker_mesh, &model, Arc::new(MarkerShader { color: *color }));
                }
            }
            _ => {}
        }
    }

    /// donde estan ahorita los cristales q faltan, se mueven junto con su mundo
    pub fn remaining_positions(&self, scene: &Scene, t: f32) -> Vec<Vec3> {
        self.collectibles.iter().filter(|c| !c.taken).map(|c| scene.body_position(c.body, t) + c.offset).collect()
    }

    /// dibuja los cristales q faltan, va en Game::render
    pub fn draw_collectibles(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        for position in self.remaining_positions(scene, t) {
            let model = Mat4::translation(position) * Mat4::rotation_y(t * CRYSTAL_SPIN) * Mat4::scale(Vec3::splat(CRYSTAL_SIZE));
            // con draw_lods el culling y el contador tambien los toman en cuenta
            r.draw_lods(&self.crystal_lods, &model, self.crystal_shader.clone());
        }
    }

    // recoge lo q la nave haya tocado en este cuadro
    fn pick_up(&mut self, ship: &Ship, scene: &Scene, t: f32, dt: f32) {
        // no basta con ver donde quedo la nave: a mucha velocidad o pocos fps un cuadro avanza
        // mas q el cristal y se lo saltaria, por eso reviso todo el tramo q recorrio
        let previous = ship.position - ship.velocity * dt;
        let path = ship.position - previous;
        for c in self.collectibles.iter_mut().filter(|c| !c.taken) {
            let crystal = scene.body_position(c.body, t) + c.offset;
            // el punto del tramo mas cercano al cristal, 0 = donde empezo y 1 = donde quedo
            let length_squared = path.dot(path).max(1e-6);
            let along = ((crystal - previous).dot(path) / length_squared).clamp(0.0, 1.0);
            let closest = previous + path * along;
            if (crystal - closest).length() < PICKUP_RADIUS {
                c.taken = true;
                self.collected += 1;
            }
        }
    }
}

// busca el cuerpo cuya superficie esta mas cerca de la nave
// si hay cuerpos dentro del rango de escaneo gana el mas cercano MEDIDO EN RADIOS (como los rangos),
// asi una luna chiquita (central city) no le gana a su planeta solo por estar a su lado (diego, d4)
fn find_nearest(ship: &Ship, scene: &Scene, t: f32) -> (Option<usize>, f32) {
    let mut nearest = None;
    let mut nearest_dist = f32::MAX;
    let mut in_range = None;
    let mut in_range_dist = f32::MAX;
    let mut in_range_radii = f32::MAX;
    for (i, b) in scene.bodies.iter().enumerate() {
        // distancia al centro menos el radio = distancia a la superficie
        // sin restar el radio un planeta grande lejos podria perder contra uno chiquito
        let dist = (scene.body_position(i, t) - ship.position).length() - b.radius;
        if dist < nearest_dist {
            nearest = Some(i);
            nearest_dist = dist;
        }
        let radii = dist / b.radius;
        if radii < SCAN_RANGE_RADII && radii < in_range_radii {
            in_range = Some(i);
            in_range_dist = dist;
            in_range_radii = radii;
        }
    }
    if in_range.is_some() {
        (in_range, in_range_dist)
    } else {
        (nearest, nearest_dist)
    }
}

/// si el cuerpo mas cercano esta lo bastante cerca pa enganchar la orbita, el hud lo usa tambien
pub fn orbit_in_range(state: &GameState, scene: &Scene) -> bool {
    match state.nearest {
        Some(i) => state.nearest_dist < ORBIT_RANGE_RADII * scene.bodies[i].radius,
        None => false,
    }
}

/// se llama una vez por cuadro, antes de mover la camara
pub fn update(state: &mut GameState, ship: &mut Ship, scene: &Scene, input: &ShipInput, t: f32, dt: f32) {
    // paso 1: ver cual cuerpo tenemos mas cerca, lo demas depende de esto
    let (nearest, nearest_dist) = find_nearest(ship, scene, t);
    state.nearest = nearest;
    state.nearest_dist = nearest_dist;

    // paso 2: con E guardamos la ficha, pero solo si estamos lo bastante cerca
    if input.scan {
        if let Some(i) = nearest {
            if nearest_dist < SCAN_RANGE_RADII * scene.bodies[i].radius {
                state.scanned = Some(i);
            }
        }
    }

    // paso 3: las teclas 1 a 6 cambian el destino
    // la tecla n es el mundo numero n (sin el sol ni las lunas), si ese mundo no existe no hace nada
    if let Some(n) = input.select {
        if let Some(i) = n.checked_sub(1).and_then(|k| scene.worlds().get(k).copied()) {
            state.warp_target = i;
        }
    }

    // paso 4: la orbita, en modo foto no se toca porque la nave esta congelada
    // shift solo suelta cuando se aprieta de nuevo, si no al enganchar con shift abajo se soltaria solo
    let boost_pressed = input.boost && !state.boost_was_down;
    state.boost_was_down = input.boost;
    if !state.photo_mode && !ship.warping() {
        if ship.orbit_body().is_some() {
            if input.orbit_toggle || boost_pressed {
                ship.release_orbit();
            }
        } else if input.orbit_toggle && orbit_in_range(state, scene) {
            if let Some(i) = nearest {
                ship.engage_orbit(i, scene, t);
            }
        }
    }

    // paso 5: tab arranca el viaje al destino, y si ya vamos viajando lo cancela
    if input.warp && !state.photo_mode {
        if ship.warping() {
            ship.cancel_warp();
        } else {
            ship.start_warp(state.warp_target, scene, t);
        }
    }
    state.orbit_lock = ship.orbit_body();
    state.warp = ship.warp_effect();

    // paso 6: recoger cristales, con la simulacion congelada no se mueve nada asi q no hace falta
    if !state.photo_mode {
        state.pick_up(ship, scene, t, dt);
    }

    // paso 6.5: el minijuego del mundo, solo se puede jugar en orbita: primero F, despues R
    if !state.photo_mode {
        state.update_minigame(ship, scene, input.race_toggle, t, dt);
    }

    // paso 7: con P entramos o salimos del modo foto
    if input.photo {
        state.photo_mode = !state.photo_mode;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ship::collision_radius;

    // pongo la nave a cierta altura sobre la superficie de un mundo, en el segundo 0
    fn ship_above(scene: &Scene, i: usize, height: f32) -> Ship {
        let center = scene.body_position(i, 0.0);
        Ship::new(center + Vec3::new(0.0, scene.bodies[i].radius + height, 0.0))
    }

    #[test]
    fn encuentra_el_cuerpo_mas_cercano_y_mide_desde_la_superficie() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 2, 5.0);
        update(&mut state, &mut ship, &scene, &ShipInput::default(), 0.0, 0.016);
        assert_eq!(state.nearest, Some(2));
        assert!((state.nearest_dist - 5.0).abs() < 1e-3);
    }

    #[test]
    fn la_luna_no_le_gana_a_su_planeta_si_el_planeta_esta_en_rango() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let gotham = scene.bodies.iter().position(|b| b.name == "Gotham").unwrap();
        let moon = scene.bodies.iter().position(|b| b.name == "Central City").unwrap();
        // en t = 0 la luna esta 9 unidades en +x de gotham; la nave queda entre los dos:
        // la superficie de la luna esta mas cerca, pero en radios gotham esta mucho mas cerca
        let mut ship = Ship::new(scene.body_position(gotham, 0.0) + Vec3::new(9.0, 6.0, 0.0));
        let to_moon = (scene.body_position(moon, 0.0) - ship.position).length() - scene.bodies[moon].radius;
        let to_gotham = (scene.body_position(gotham, 0.0) - ship.position).length() - scene.bodies[gotham].radius;
        assert!(to_moon < to_gotham);
        update(&mut state, &mut ship, &scene, &ShipInput::default(), 0.0, 0.016);
        assert_eq!(state.nearest, Some(gotham));
        assert!((state.nearest_dist - to_gotham).abs() < 1e-3);
    }

    #[test]
    fn solo_escanea_si_esta_cerca_y_aprieta_la_tecla() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let scan = ShipInput { scan: true, ..Default::default() };
        let radius = scene.bodies[1].radius;

        // cerca pero sin apretar nada
        let mut ship = ship_above(&scene, 1, radius);
        update(&mut state, &mut ship, &scene, &ShipInput::default(), 0.0, 0.016);
        assert_eq!(state.scanned, None);

        // apretando pero lejos del rango, sigue siendo el mas cercano
        let mut ship = ship_above(&scene, 1, radius * 3.5);
        update(&mut state, &mut ship, &scene, &scan, 0.0, 0.016);
        assert_eq!(state.nearest, Some(1));
        assert_eq!(state.scanned, None);

        // apretando y cerca
        let mut ship = ship_above(&scene, 1, radius);
        update(&mut state, &mut ship, &scene, &scan, 0.0, 0.016);
        assert_eq!(state.scanned, Some(1));
    }

    #[test]
    fn el_destino_ignora_lo_q_no_es_un_mundo() {
        let mut scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, 5.0);
        let select = |i| ShipInput { select: Some(i), ..Default::default() };

        update(&mut state, &mut ship, &scene, &select(3), 0.0, 0.016);
        assert_eq!(state.warp_target, 3);

        // el sol no es destino
        update(&mut state, &mut ship, &scene, &select(0), 0.0, 0.016);
        assert_eq!(state.warp_target, 3);

        // un mundo q no existe tampoco
        update(&mut state, &mut ship, &scene, &select(99), 0.0, 0.016);
        assert_eq!(state.warp_target, 3);

        // las lunas no cuentan: si atlantis (4) fuera luna, la tecla 4 seria el siguiente mundo, oa (5)
        scene.bodies[4].parent = Some(3);
        update(&mut state, &mut ship, &scene, &select(4), 0.0, 0.016);
        assert_eq!(state.warp_target, 5);
        // y la tecla 5 ya no tiene mundo
        update(&mut state, &mut ship, &scene, &select(5), 0.0, 0.016);
        assert_eq!(state.warp_target, 5);
    }

    #[test]
    fn la_f_solo_engancha_cerca_y_la_f_o_shift_sueltan() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let press_f = ShipInput { orbit_toggle: true, ..Default::default() };
        let hold_shift = ShipInput { boost: true, ..Default::default() };
        let radius = scene.bodies[1].radius;
        let tick = |state: &mut GameState, ship: &mut Ship, input: &ShipInput| update(state, ship, &scene, input, 0.0, 0.016);

        // lejos no engancha
        let mut ship = ship_above(&scene, 1, radius * 2.6);
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, None);

        // cerca si, y otra f suelta
        let mut ship = ship_above(&scene, 1, radius * 2.0);
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, Some(1));
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, None);

        // shift tambien suelta
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, Some(1));
        tick(&mut state, &mut ship, &hold_shift);
        assert_eq!(state.orbit_lock, None);

        // si enganchamos con shift ya abajo no se suelta sola, tiene q apretarlo de nuevo
        let f_with_shift = ShipInput { orbit_toggle: true, boost: true, ..Default::default() };
        tick(&mut state, &mut ship, &f_with_shift);
        tick(&mut state, &mut ship, &hold_shift);
        assert_eq!(state.orbit_lock, Some(1));
        tick(&mut state, &mut ship, &ShipInput::default());
        tick(&mut state, &mut ship, &hold_shift);
        assert_eq!(state.orbit_lock, None);
    }

    #[test]
    fn tab_arranca_y_cancela_el_viaje() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let press_tab = ShipInput { warp: true, ..Default::default() };
        let press_f = ShipInput { orbit_toggle: true, ..Default::default() };
        let mut ship = ship_above(&scene, 3, 5.0);
        let tick = |state: &mut GameState, ship: &mut Ship, input: &ShipInput| update(state, ship, &scene, input, 0.0, 0.016);

        // el destino por defecto es themyscira y gotham esta lejos
        tick(&mut state, &mut ship, &press_tab);
        assert!(ship.warping());

        // viajando la f no engancha nada
        tick(&mut state, &mut ship, &press_f);
        assert_eq!(state.orbit_lock, None);

        tick(&mut state, &mut ship, &press_tab);
        assert!(!ship.warping());
    }

    #[test]
    fn despues_de_llegar_da_tiempo_de_apretar_la_f() {
        let scene = Scene::new();
        let dt = 1.0 / 60.0;
        let mut shortest = f32::MAX;

        for to in 1..=5 {
            let from = if to == 5 { 1 } else { 5 };
            let mut state = GameState::new(&scene);
            state.warp_target = to;
            let mut ship = ship_above(&scene, from, 5.0);
            let mut t = 0.0;
            update(&mut state, &mut ship, &scene, &ShipInput { warp: true, ..Default::default() }, t, dt);
            while ship.warping() {
                t += dt;
                ship.update(ShipInput::default(), dt, &scene, t);
            }

            // ya llegamos y no se toca nada: cuanto tiempo sigue disponible la orbita
            update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
            assert_eq!(state.nearest, Some(to));
            assert!(orbit_in_range(&state, &scene));
            let mut in_range = 0.0;
            while orbit_in_range(&state, &scene) && in_range < 30.0 {
                t += dt;
                in_range += dt;
                ship.update(ShipInput::default(), dt, &scene, t);
                update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
            }
            shortest = shortest.min(in_range);
        }
        println!("despues de llegar la f sigue disponible al menos {shortest:.1} s");
        assert!(shortest > 3.0);
    }

    #[test]
    fn tab_desde_una_orbita_la_suelta_y_viaja() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 3, 5.0);
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, 0.016);
        assert_eq!(state.orbit_lock, Some(3));

        update(&mut state, &mut ship, &scene, &ShipInput { warp: true, ..Default::default() }, 0.0, 0.016);
        assert_eq!(state.orbit_lock, None);
        assert!(ship.warping());
    }

    #[test]
    fn en_modo_foto_tab_no_hace_nada() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        state.photo_mode = true;
        let mut ship = ship_above(&scene, 3, 5.0);
        update(&mut state, &mut ship, &scene, &ShipInput { warp: true, ..Default::default() }, 0.0, 0.016);
        assert!(!ship.warping());
    }

    #[test]
    fn en_modo_foto_la_f_no_hace_nada() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        state.photo_mode = true;
        let mut ship = ship_above(&scene, 1, 2.0);
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, 0.016);
        assert_eq!(state.orbit_lock, None);
    }

    #[test]
    fn la_misma_semilla_da_los_mismos_lugares() {
        let scene = Scene::new();
        let a = make_collectibles(&scene, COLLECTIBLE_SEED);
        let b = make_collectibles(&scene, COLLECTIBLE_SEED);
        let other = make_collectibles(&scene, COLLECTIBLE_SEED + 1);
        assert_eq!(a.len(), 10);
        for k in 0..a.len() {
            assert_eq!((a[k].offset - b[k].offset).length(), 0.0);
        }
        let moved = (0..a.len()).filter(|&k| (a[k].offset - other[k].offset).length() > 0.5).count();
        assert!(moved >= 8, "otra semilla casi no los movio");
    }

    #[test]
    fn cada_mundo_tiene_dos_cristales_alcanzables() {
        let scene = Scene::new();
        let crystals = make_collectibles(&scene, COLLECTIBLE_SEED);
        for world in 1..=5 {
            let radius = scene.bodies[world].radius;
            let own: Vec<&Collectible> = crystals.iter().filter(|c| c.body == world).collect();
            assert_eq!(own.len(), 2);
            for c in &own {
                let distance = c.offset.length();
                // afuera de la colision, se puede llegar sin chocar
                assert!(distance > collision_radius(radius) + 0.5);
                // adentro del rango de la orbita, se puede agarrar rodeando el planeta
                assert!(distance - radius < ORBIT_RANGE_RADII * radius);
            }
            // y no quedan pegados uno del otro
            assert!((own[0].offset - own[1].offset).length() > radius);
        }
    }

    #[test]
    fn el_cristal_se_recoge_una_sola_vez_y_solo_de_cerca() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let t = 3.0;
        let crystal = state.remaining_positions(&scene, t)[0];
        assert_eq!(state.total, 10);

        // cerca pero afuera del radio no pasa nada
        let mut ship = Ship::new(crystal + Vec3::new(PICKUP_RADIUS + 0.2, 0.0, 0.0));
        update(&mut state, &mut ship, &scene, &ShipInput::default(), t, 0.016);
        assert_eq!(state.collected, 0);

        // adentro del radio lo agarra y ya no queda en la lista
        let mut ship = Ship::new(crystal + Vec3::new(PICKUP_RADIUS - 0.2, 0.0, 0.0));
        update(&mut state, &mut ship, &scene, &ShipInput::default(), t, 0.016);
        assert_eq!(state.collected, 1);
        assert_eq!(state.remaining_positions(&scene, t).len(), 9);

        // si la nave se queda ahi no lo cuenta otra vez
        update(&mut state, &mut ship, &scene, &ShipInput::default(), t, 0.016);
        assert_eq!(state.collected, 1);
    }

    #[test]
    fn a_mucha_velocidad_tampoco_se_salta_el_cristal() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let t = 3.0;
        let crystal = state.remaining_positions(&scene, t)[0];

        // a 90 de velocidad con 20 fps avanza 4.5 por cuadro: empieza antes del cristal y termina despues
        let mut ship = Ship::new(crystal + Vec3::new(2.25, 0.0, 0.0));
        ship.velocity = Vec3::new(90.0, 0.0, 0.0);
        update(&mut state, &mut ship, &scene, &ShipInput::default(), t, 0.05);
        assert_eq!(state.collected, 1);

        // el mismo cuadro pero pasando lejos no lo agarra
        let mut state = GameState::new(&scene);
        let mut ship = Ship::new(crystal + Vec3::new(2.25, 3.0, 0.0));
        ship.velocity = Vec3::new(90.0, 0.0, 0.0);
        update(&mut state, &mut ship, &scene, &ShipInput::default(), t, 0.05);
        assert_eq!(state.collected, 0);
    }

    #[test]
    fn los_cristales_se_mueven_con_su_mundo() {
        let scene = Scene::new();
        let state = GameState::new(&scene);
        let (first, later) = (state.remaining_positions(&scene, 0.0), state.remaining_positions(&scene, 50.0));
        let c = &state.collectibles[0];
        assert!((first[0] - later[0]).length() > 5.0);
        assert!((later[0] - scene.body_position(c.body, 50.0) - c.offset).length() < 1e-3);
    }

    #[test]
    fn las_caras_del_cristal_miran_hacia_afuera() {
        let mesh = crystal_mesh();
        assert_eq!(mesh.triangle_count(), 8);
        for tri in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (mesh.vertices[tri[0] as usize].position, mesh.vertices[tri[1] as usize].position, mesh.vertices[tri[2] as usize].position);
            // el producto cruz sale pa donde mira la cara, y el centro de la cara apunta hacia afuera
            let outward = (a + b + c) / 3.0;
            assert!((b - a).cross(c - a).dot(outward) > 0.0);
        }
    }

    #[test]
    fn la_p_prende_y_apaga_el_modo_foto() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, 5.0);
        let photo = ShipInput { photo: true, ..Default::default() };

        update(&mut state, &mut ship, &scene, &photo, 0.0, 0.016);
        assert!(state.photo_mode);
        update(&mut state, &mut ship, &scene, &photo, 0.0, 0.016);
        assert!(!state.photo_mode);
    }

    fn body_named(scene: &Scene, name: &str) -> usize {
        scene.bodies.iter().position(|b| b.name == name).unwrap_or_else(|| panic!("falta {name}"))
    }

    #[test]
    fn la_r_no_hace_nada_sin_estar_en_orbita() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, scene.bodies[1].radius * 10.0);
        let race = ShipInput { race_toggle: true, ..Default::default() };
        update(&mut state, &mut ship, &scene, &race, 0.0, 0.016);
        assert!(state.minigame_hud().is_none());
    }

    #[test]
    fn la_r_empieza_el_minijuego_en_orbita_y_otra_r_lo_cancela() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, scene.bodies[1].radius * 2.0);
        let orbit = ShipInput { orbit_toggle: true, ..Default::default() };
        let race = ShipInput { race_toggle: true, ..Default::default() };
        let tick = |state: &mut GameState, ship: &mut Ship, input: &ShipInput| update(state, ship, &scene, input, 0.0, 0.016);

        tick(&mut state, &mut ship, &orbit);
        assert_eq!(state.orbit_lock, Some(1));

        tick(&mut state, &mut ship, &race);
        let (body, hud) = state.minigame_hud().expect("deberia haber un minijuego en curso");
        assert_eq!(body, 1);
        match hud {
            MinigameHud::Gates { next, total, .. } => {
                assert_eq!(next, 0);
                assert_eq!(total, 6); // Themyscira: circuito de entrenamiento
            }
            _ => panic!("Themyscira deberia ser un circuito de aros"),
        }

        tick(&mut state, &mut ship, &race);
        assert!(state.minigame_hud().is_none());
    }

    #[test]
    fn soltar_la_orbita_cancela_el_minijuego_en_curso() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, scene.bodies[1].radius * 2.0);
        let orbit = ShipInput { orbit_toggle: true, ..Default::default() };
        let race = ShipInput { race_toggle: true, ..Default::default() };
        let tick = |state: &mut GameState, ship: &mut Ship, input: &ShipInput| update(state, ship, &scene, input, 0.0, 0.016);

        tick(&mut state, &mut ship, &orbit);
        tick(&mut state, &mut ship, &race);
        assert!(state.minigame_hud().is_some());

        tick(&mut state, &mut ship, &orbit); // F otra vez: suelta la orbita
        assert_eq!(ship.orbit_body(), None);
        tick(&mut state, &mut ship, &ShipInput::default());
        assert!(state.minigame_hud().is_none(), "sin orbita el minijuego se tiene q cancelar solo");
    }

    #[test]
    fn pasar_los_aros_en_orden_termina_el_circuito_y_guarda_el_mejor_tiempo() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, scene.bodies[1].radius * 2.0);
        let dt = 1.0 / 60.0;
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, dt);
        update(&mut state, &mut ship, &scene, &ShipInput { race_toggle: true, ..Default::default() }, 0.0, dt);
        assert!(state.minigame_hud().is_some());
        assert_eq!(state.race_best[1], None);

        let gates = make_gate_ring(&scene, 1, RACE_SEED, 6, 0.0, 0.0, Vec3::ZERO);
        let mut t = 0.0;
        for gate in &gates {
            let target = scene.body_position(1, t) + gate.offset;
            // la "teletransporto" justo cruzando el aro, pa probar q cada uno avanza el circuito
            ship.position = target - gate.forward * 0.1;
            ship.velocity = gate.forward * 50.0;
            t += dt;
            update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
        }
        assert!(state.minigame_hud().is_none(), "con todos los aros el circuito ya deberia haber terminado");
        let result = state.race_result.as_ref().expect("deberia quedar un resultado");
        assert_eq!(result.body, 1);
        assert!(result.success);
        assert!(state.race_best[1].is_some_and(|best| best > 0.0));
    }

    #[test]
    fn tocar_un_peligro_en_krypton_falla_la_esquiva() {
        let scene = Scene::new();
        let krypton = body_named(&scene, "Krypton");
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, krypton, scene.bodies[krypton].radius * 2.0);
        let dt = 1.0 / 60.0;
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, dt);
        update(&mut state, &mut ship, &scene, &ShipInput { race_toggle: true, ..Default::default() }, 0.0, dt);
        assert!(matches!(state.minigame_hud(), Some((_, MinigameHud::Survive { .. }))));

        let hazards = make_hazards(&scene, krypton, RACE_SEED, 10);
        let target = scene.body_position(krypton, 0.0) + hazards[0];
        ship.position = target - Vec3::new(1.0, 0.0, 0.0) * 0.1;
        ship.velocity = Vec3::new(1.0, 0.0, 0.0) * 50.0;
        update(&mut state, &mut ship, &scene, &ShipInput::default(), dt, dt);

        assert!(state.minigame_hud().is_none(), "al tocar un peligro la esquiva se tiene q cancelar");
        let result = state.race_result.as_ref().expect("deberia quedar un resultado de fallo");
        assert!(!result.success);
        // igual guarda cuanto duro, pa animar a intentarlo de nuevo
        assert!(state.race_best[krypton].is_some());
    }

    #[test]
    fn sobrevivir_el_tiempo_completo_en_krypton_termina_con_exito() {
        let scene = Scene::new();
        let krypton = body_named(&scene, "Krypton");
        let radius = scene.bodies[krypton].radius;
        let mut state = GameState::new(&scene);
        // bien lejos del anillo de peligros (2.2 radios + 1.5), pa no tocar ninguno sin querer
        let mut ship = ship_above(&scene, krypton, radius * 2.0);
        let dt = 1.0 / 60.0;
        let mut t = 0.0;
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, t, dt);
        t += dt;
        update(&mut state, &mut ship, &scene, &ShipInput { race_toggle: true, ..Default::default() }, t, dt);
        assert!(state.minigame_hud().is_some());

        for _ in 0..1300 {
            t += dt;
            update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
            if state.minigame_hud().is_none() {
                break;
            }
        }
        let result = state.race_result.as_ref().expect("la esquiva deberia haber terminado");
        assert!(result.success, "{}", result.text);
    }

    #[test]
    fn tocar_todos_los_objetivos_en_gotham_termina_la_patrulla() {
        let scene = Scene::new();
        let gotham = body_named(&scene, "Gotham");
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, gotham, scene.bodies[gotham].radius * 2.0);
        let dt = 1.0 / 60.0;
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, dt);
        update(&mut state, &mut ship, &scene, &ShipInput { race_toggle: true, ..Default::default() }, 0.0, dt);
        assert!(matches!(state.minigame_hud(), Some((_, MinigameHud::Collect { .. }))));

        let targets = make_targets(&scene, gotham, RACE_SEED, 6, false);
        let mut t = 0.0;
        for target in &targets {
            let world_pos = scene.body_position(gotham, t) + target.offset(t);
            ship.position = world_pos - Vec3::new(0.0, 1.0, 0.0) * 0.1;
            ship.velocity = Vec3::new(0.0, 1.0, 0.0) * 50.0;
            t += dt;
            update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
        }
        assert!(state.minigame_hud().is_none(), "con todos los objetivos tocados la patrulla ya deberia haber terminado");
        let result = state.race_result.as_ref().expect("deberia quedar un resultado");
        assert!(result.success, "{}", result.text);
    }

    #[test]
    fn se_le_acaba_el_tiempo_a_un_aro_en_oa_y_falla() {
        let scene = Scene::new();
        let oa = body_named(&scene, "Oa");
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, oa, scene.bodies[oa].radius * 2.0);
        let dt = 1.0 / 60.0;
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, dt);
        update(&mut state, &mut ship, &scene, &ShipInput { race_toggle: true, ..Default::default() }, 0.0, dt);
        assert!(matches!(state.minigame_hud(), Some((_, MinigameHud::Gates { gate_left: Some(_), .. }))), "Oa deberia tener reloj por aro");

        // dejo pasar mas de los 4s del primer aro sin acercarme
        let mut t = 0.0;
        for _ in 0..300 {
            t += dt;
            update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
            if state.minigame_hud().is_none() {
                break;
            }
        }
        let result = state.race_result.as_ref().expect("deberia haber fallado por tiempo");
        assert!(!result.success, "{}", result.text);
    }

    #[test]
    fn cada_mundo_tiene_un_reto_distinto() {
        let scene = Scene::new();
        let names: Vec<&str> = ["Themyscira", "Krypton", "Gotham", "Atlantis", "Oa"]
            .iter()
            .map(|&n| {
                let i = body_named(&scene, n);
                GameState::new(&scene).minigame_name(i).expect("todos los mundos tienen reto")
            })
            .collect();
        for a in 0..names.len() {
            for b in (a + 1)..names.len() {
                assert_ne!(names[a], names[b], "dos mundos comparten el nombre de su minijuego");
            }
        }
        // el sol y las lunas no tienen reto
        let state = GameState::new(&scene);
        assert!(state.minigame_name(0).is_none());
        let central_city = body_named(&scene, "Central City");
        assert!(state.minigame_name(central_city).is_none());
    }

    #[test]
    fn la_misma_semilla_da_el_mismo_reto() {
        let scene = Scene::new();
        let krypton = body_named(&scene, "Krypton");
        let gotham = body_named(&scene, "Gotham");

        let a = make_hazards(&scene, krypton, RACE_SEED, 10);
        let b = make_hazards(&scene, krypton, RACE_SEED, 10);
        let other = make_hazards(&scene, krypton, RACE_SEED + 1, 10);
        for k in 0..a.len() {
            assert_eq!((a[k] - b[k]).length(), 0.0);
        }
        assert!((0..a.len()).any(|k| (a[k] - other[k]).length() > 0.5), "otra semilla deberia mover los peligros");

        let ta = make_targets(&scene, gotham, RACE_SEED, 6, false);
        let tb = make_targets(&scene, gotham, RACE_SEED, 6, false);
        for k in 0..ta.len() {
            assert_eq!((ta[k].base_offset - tb[k].base_offset).length(), 0.0);
        }
    }

    #[test]
    fn todo_lo_generado_queda_lejos_de_la_colision() {
        let scene = Scene::new();
        for world in 1..=5 {
            let floor = collision_radius(scene.bodies[world].radius);
            let name = scene.bodies[world].name;
            for gate in make_gate_ring(&scene, world, RACE_SEED, 6, 5.0, 0.3, Vec3::ZERO) {
                assert!(gate.offset.length() > floor + 1.0, "{name} tiene un aro cerca de la colision");
            }
            for hazard in make_hazards(&scene, world, RACE_SEED, 10) {
                assert!(hazard.length() > floor + 1.0, "{name} tiene un peligro cerca de la colision");
            }
            for target in make_targets(&scene, world, RACE_SEED, 6, true) {
                assert!(target.base_offset.length() > floor + 1.0, "{name} tiene un objetivo cerca de la colision");
            }
        }
    }
}

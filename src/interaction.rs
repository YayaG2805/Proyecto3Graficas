//! todo lo q pasa entre la nave y los mundos: cual esta mas cerca, escanear, elegir destino...
//!
//! main.rs llama a `update` una vez por cuadro y el hud solo lee lo q queda en `GameState`.

use crate::math::{Mat4, Vec3};
use crate::mesh::{flat_mesh, Mesh};
use crate::noise::hash01;
use crate::renderer::{Fragment, FragmentShader, Renderer, Uniforms};
use crate::scene::Scene;
use crate::shaders::RaceGateShader;
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

// --- carrera de anillos (tecla R): hay q estar en orbita (F) de un mundo pa poder jugarla ---
// misma semilla = mismo circuito, se puede cambiar en vivo
const RACE_SEED: u32 = 83;
// cuantas puertas tiene un mundo q no esta en `race_theme` (no debería pasar, es el respaldo)
const RACE_GATE_COUNT: usize = 6;
// a q distancia del centro del mundo va el circuito: radio * esto + lo otro
// bastante mas lejos q los cristales: con el circuito ondulado (RaceTheme::wobble) la puerta
// mas cerrada todavia tiene q quedar bien afuera de la colision
const RACE_RING_RADII: f32 = 2.2;
const RACE_RING_EXTRA: f32 = 1.5;
// q tan inclinado va el eje del circuito respecto a +y, pa q cada mundo se vea distinto
const RACE_AXIS_TILT: f32 = 0.5;
// tamaño del aro sin escalar: radio del circulo central y grosor del tubo
const RACE_GATE_MAJOR: f32 = 1.0;
const RACE_GATE_MINOR: f32 = 0.16;
// el aro mide esto * el radio del planeta
const RACE_GATE_SCALE_RADII: f32 = 0.5;
// cuanto margen de mas tiene el agujero pa contar como "pasada", si no es imposible de verdad
const RACE_PASS_LENIENCY: f32 = 1.5;
// cuanto dura el mensaje de "carrera terminada" en el hud
const RACE_RESULT_SECONDS: f32 = 6.0;

/// la forma y el color del circuito de un mundo: cada héroe vuela distinto.
/// `lobes` = cuantos "pétalos" tiene el circuito (0 = círculo perfecto) y `wobble` qué tan
/// marcados son (0 a ~0.3): juntos hacen q el radio del circuito varíe con `sin(lobes * ángulo)`,
/// asi cada mundo queda con una silueta distinta en vez de ser siempre el mismo círculo.
struct RaceTheme {
    lobes: f32,
    wobble: f32,
    gate_count: usize,
    color: Vec3,
}

/// el circuito de cada mundo, pensado pa q se sienta distinto segun su héroe:
/// Themyscira es el mas facil (un círculo liso, para aprender), Krypton es una estrella de
/// cristal angulosa, Gotham zigzaguea esquivando "edificios", Atlantis ondula como las olas
/// y Oa (el anillo de poder) tiene mas puertas y mas seguido.
fn race_theme(name: &str) -> RaceTheme {
    match name {
        "Themyscira" => RaceTheme { lobes: 0.0, wobble: 0.0, gate_count: 6, color: Vec3::new(0.55, 0.9, 0.5) },
        "Krypton" => RaceTheme { lobes: 5.0, wobble: 0.25, gate_count: 6, color: Vec3::new(0.35, 0.65, 1.0) },
        "Gotham" => RaceTheme { lobes: 8.0, wobble: 0.18, gate_count: 7, color: Vec3::new(1.0, 0.85, 0.25) },
        "Atlantis" => RaceTheme { lobes: 3.0, wobble: 0.3, gate_count: 6, color: Vec3::new(0.2, 0.75, 0.95) },
        "Oa" => RaceTheme { lobes: 6.0, wobble: 0.22, gate_count: 8, color: Vec3::new(0.35, 1.0, 0.45) },
        _ => RaceTheme { lobes: 0.0, wobble: 0.0, gate_count: RACE_GATE_COUNT, color: Vec3::new(1.0, 0.35, 0.65) },
    }
}

// una puerta del circuito: offset fijo desde el centro del mundo (no rota con el giro del
// planeta, igual q los cristales) y como esta orientada: la nave la atraviesa por el agujero,
// en la direccion `forward` (tangente al circuito, por donde se vuela rodeando el planeta).
// `color` es el del mundo (RaceTheme), asi cada circuito se ve distinto.
#[derive(Clone, Copy)]
struct RaceGate {
    offset: Vec3,
    right: Vec3,
    forward: Vec3,
    scale: f32,
    radius: f32,
    color: Vec3,
}

// un vector cualquiera en angulo recto con v, pa armar una base ortonormal
fn perpendicular(v: Vec3) -> Vec3 {
    let axis = if v.y.abs() < 0.9 { Vec3::UP } else { Vec3::new(1.0, 0.0, 0.0) };
    v.cross(axis).normalized()
}

// arma el circuito de cada mundo: puertas repartidas parejo alrededor de un circulo cuyo
// radio varia con `sin(lobes * angulo)` (RaceTheme) y cuyo eje va inclinado distinto en cada
// mundo; todo sale de la semilla, asi q es reproducible
fn make_race_tracks(scene: &Scene, seed: u32) -> Vec<Vec<RaceGate>> {
    scene
        .bodies
        .iter()
        .enumerate()
        .map(|(i, b)| {
            // el sol y las lunas no tienen carrera, igual q los cristales
            if i < FIRST_WORLD || b.parent.is_some() {
                return Vec::new();
            }
            let theme = race_theme(b.name);
            let polar = hash01(i as i32, 20, 0, seed) * RACE_AXIS_TILT;
            let azimuth = hash01(i as i32, 21, 0, seed) * std::f32::consts::TAU;
            let axis = Vec3::new(polar.sin() * azimuth.cos(), polar.cos(), polar.sin() * azimuth.sin()).normalized();
            let t1 = perpendicular(axis);
            let t2 = axis.cross(t1);
            let base_ring = b.radius * RACE_RING_RADII + RACE_RING_EXTRA;
            let scale = b.radius * RACE_GATE_SCALE_RADII;
            (0..theme.gate_count)
                .map(|k| {
                    let a = k as f32 / theme.gate_count as f32 * std::f32::consts::TAU;
                    // radio del circuito en este angulo: circulo perfecto si wobble = 0, si no
                    // "respira" con sin(lobes * a) y da una silueta de flor o de estrella
                    let shape = (theme.lobes * a).sin();
                    let shape_derivative = theme.lobes * (theme.lobes * a).cos();
                    let r = base_ring * (1.0 + theme.wobble * shape);
                    let dr = base_ring * theme.wobble * shape_derivative;
                    let radial = t1 * a.cos() + t2 * a.sin();
                    // derivada de `radial` respecto al angulo: hacia donde se mueve al crecer a
                    let radial_perp = t2 * a.cos() - t1 * a.sin();
                    let offset = radial * r;
                    // la tangente real del circuito (incluyendo q el radio tambien cambia),
                    // por donde hay q volar pa atravesar la puerta de frente
                    let forward = (radial_perp * r + radial * dr).normalized();
                    let right = forward.cross(axis).normalized();
                    RaceGate {
                        offset,
                        right,
                        forward,
                        scale,
                        radius: scale * (RACE_GATE_MAJOR - RACE_GATE_MINOR) * RACE_PASS_LENIENCY,
                        color: theme.color,
                    }
                })
                .collect()
        })
        .collect()
}

// la carrera en curso
#[derive(Clone, Copy)]
struct ActiveRace {
    body: usize,
    next_gate: usize,
    elapsed: f32,
}

/// el resultado de la ultima carrera terminada, pa mostrarlo un rato en el hud
#[derive(Clone, Copy)]
pub struct RaceResult {
    pub body: usize,
    pub time: f32,
    pub best: bool,
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

    // --- carrera de anillos ---
    /// la carrera en curso, None = no se esta jugando
    race: Option<ActiveRace>,
    /// el circuito de cada mundo (vacio en el sol y las lunas)
    race_tracks: Vec<Vec<RaceGate>>,
    /// mejor tiempo logrado en cada mundo, None = todavia no se completa ninguna
    pub race_best: Vec<Option<f32>>,
    /// el resultado de la ultima carrera terminada, pa mostrarlo un rato en el hud
    pub race_result: Option<RaceResult>,
    race_result_timer: f32,
    race_gate_mesh: Mesh,
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
            race: None,
            race_tracks: make_race_tracks(scene, RACE_SEED),
            race_best: vec![None; scene.bodies.len()],
            race_result: None,
            race_result_timer: 0.0,
            race_gate_mesh: Mesh::torus(RACE_GATE_MAJOR, RACE_GATE_MINOR, 24, 10),
        }
    }

    /// si hay carrera: el mundo, cuantas puertas lleva, cuantas hay en total y cuanto tiempo lleva
    pub fn race_progress(&self) -> Option<(usize, usize, usize, f32)> {
        self.race.map(|a| (a.body, a.next_gate, self.race_tracks[a.body].len(), a.elapsed))
    }

    /// si el mundo i tiene circuito de carrera (el sol y las lunas no), pa el aviso del hud
    pub fn has_race_track(&self, body: usize) -> bool {
        !self.race_tracks[body].is_empty()
    }

    /// cambia la carrera segun la tecla R y el estado de la orbita
    fn update_race(&mut self, ship: &Ship, scene: &Scene, race_toggle: bool, t: f32, dt: f32) {
        if self.race_result_timer > 0.0 {
            self.race_result_timer -= dt;
            if self.race_result_timer <= 0.0 {
                self.race_result = None;
            }
        }

        // si se solto la orbita de ese mundo (F, warp, explosion...) la carrera se cancela sola
        if let Some(active) = self.race {
            if ship.orbit_body() != Some(active.body) {
                self.race = None;
            }
        }

        // R: si ya estamos corriendo la cancela; si no, la empieza (solo estando en orbita
        // de un mundo con circuito, el sol y las lunas no tienen)
        if race_toggle {
            self.race = if self.race.is_some() {
                None
            } else {
                ship.orbit_body().filter(|&i| !self.race_tracks[i].is_empty()).map(|i| ActiveRace { body: i, next_gate: 0, elapsed: 0.0 })
            };
        }

        let Some(mut active) = self.race else { return };
        active.elapsed += dt;
        let gates = &self.race_tracks[active.body];
        if let Some(gate) = gates.get(active.next_gate) {
            // mismo truco q pick_up: reviso todo el tramo q recorrio la nave este cuadro, no
            // solo donde quedo, pa no saltarnos la puerta a mucha velocidad o con pocos fps
            let target = scene.body_position(active.body, t) + gate.offset;
            let previous = ship.position - ship.velocity * dt;
            let path = ship.position - previous;
            let length_squared = path.dot(path).max(1e-6);
            let along = ((target - previous).dot(path) / length_squared).clamp(0.0, 1.0);
            let closest = previous + path * along;
            if (target - closest).length() < gate.radius {
                active.next_gate += 1;
            }
        }

        if active.next_gate >= gates.len() {
            let improved = match self.race_best[active.body] {
                Some(best) => active.elapsed < best,
                None => true,
            };
            if improved {
                self.race_best[active.body] = Some(active.elapsed);
            }
            self.race_result = Some(RaceResult { body: active.body, time: active.elapsed, best: improved });
            self.race_result_timer = RACE_RESULT_SECONDS;
            self.race = None;
        } else {
            self.race = Some(active);
        }
    }

    /// dibuja las puertas q faltan de la carrera en curso, va en Game::draw_geometry
    pub fn draw_race(&self, r: &mut Renderer, scene: &Scene, t: f32) {
        let Some(active) = self.race else { return };
        let gates = &self.race_tracks[active.body];
        let center = scene.body_position(active.body, t);
        for (k, gate) in gates.iter().enumerate().skip(active.next_gate) {
            let up = gate.right.cross(gate.forward);
            let model = Mat4::from_basis(gate.right, up, gate.forward, center + gate.offset) * Mat4::scale(Vec3::splat(gate.scale));
            let shader: Arc<dyn FragmentShader> = Arc::new(RaceGateShader { color: gate.color, next: k == active.next_gate });
            r.draw_mesh_additive(&self.race_gate_mesh, &model, shader);
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

    // paso 6.5: la carrera de anillos, solo se puede jugar en orbita: primero F, despues R
    if !state.photo_mode {
        state.update_race(ship, scene, input.race_toggle, t, dt);
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

    #[test]
    fn la_r_no_hace_nada_sin_estar_en_orbita() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, scene.bodies[1].radius * 10.0);
        let race = ShipInput { race_toggle: true, ..Default::default() };
        update(&mut state, &mut ship, &scene, &race, 0.0, 0.016);
        assert!(state.race_progress().is_none());
    }

    #[test]
    fn la_r_empieza_la_carrera_en_orbita_y_otra_r_la_cancela() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, scene.bodies[1].radius * 2.0);
        let orbit = ShipInput { orbit_toggle: true, ..Default::default() };
        let race = ShipInput { race_toggle: true, ..Default::default() };
        let tick = |state: &mut GameState, ship: &mut Ship, input: &ShipInput| update(state, ship, &scene, input, 0.0, 0.016);

        tick(&mut state, &mut ship, &orbit);
        assert_eq!(state.orbit_lock, Some(1));

        tick(&mut state, &mut ship, &race);
        let (body, next, total, _) = state.race_progress().expect("deberia haber carrera en curso");
        assert_eq!(body, 1);
        assert_eq!(next, 0);
        assert_eq!(total, RACE_GATE_COUNT);

        tick(&mut state, &mut ship, &race);
        assert!(state.race_progress().is_none());
    }

    #[test]
    fn soltar_la_orbita_cancela_la_carrera_en_curso() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, scene.bodies[1].radius * 2.0);
        let orbit = ShipInput { orbit_toggle: true, ..Default::default() };
        let race = ShipInput { race_toggle: true, ..Default::default() };
        let tick = |state: &mut GameState, ship: &mut Ship, input: &ShipInput| update(state, ship, &scene, input, 0.0, 0.016);

        tick(&mut state, &mut ship, &orbit);
        tick(&mut state, &mut ship, &race);
        assert!(state.race_progress().is_some());

        tick(&mut state, &mut ship, &orbit); // F otra vez: suelta la orbita
        assert_eq!(ship.orbit_body(), None);
        tick(&mut state, &mut ship, &ShipInput::default());
        assert!(state.race_progress().is_none(), "sin orbita la carrera se tiene q cancelar sola");
    }

    #[test]
    fn pasar_las_puertas_en_orden_termina_la_carrera_y_guarda_el_mejor_tiempo() {
        let scene = Scene::new();
        let mut state = GameState::new(&scene);
        let mut ship = ship_above(&scene, 1, scene.bodies[1].radius * 2.0);
        let dt = 1.0 / 60.0;
        update(&mut state, &mut ship, &scene, &ShipInput { orbit_toggle: true, ..Default::default() }, 0.0, dt);
        update(&mut state, &mut ship, &scene, &ShipInput { race_toggle: true, ..Default::default() }, 0.0, dt);
        assert!(state.race_progress().is_some());
        assert_eq!(state.race_best[1], None);

        let track = &make_race_tracks(&scene, RACE_SEED)[1];
        let mut t = 0.0;
        for gate in track {
            let target = scene.body_position(1, t) + gate.offset;
            // la "teletransporto" justo cruzando la puerta, pa probar q cada una avanza la carrera
            ship.position = target - gate.forward * 0.1;
            ship.velocity = gate.forward * 50.0;
            t += dt;
            update(&mut state, &mut ship, &scene, &ShipInput::default(), t, dt);
        }
        assert!(state.race_progress().is_none(), "con todas las puertas la carrera ya deberia haber terminado");
        let result = state.race_result.expect("deberia quedar un resultado");
        assert_eq!(result.body, 1);
        assert!(result.best);
        assert_eq!(state.race_best[1], Some(result.time));
    }

    #[test]
    fn la_misma_semilla_da_el_mismo_circuito() {
        let scene = Scene::new();
        let a = make_race_tracks(&scene, RACE_SEED);
        let b = make_race_tracks(&scene, RACE_SEED);
        let other = make_race_tracks(&scene, RACE_SEED + 1);

        assert_eq!(a[1].len(), RACE_GATE_COUNT);
        for k in 0..a[1].len() {
            assert_eq!((a[1][k].offset - b[1][k].offset).length(), 0.0);
        }
        let moved = (0..a[1].len()).filter(|&k| (a[1][k].offset - other[1][k].offset).length() > 0.5).count();
        assert!(moved > 0, "otra semilla deberia mover el circuito");

        // el sol y las lunas no tienen circuito
        assert!(a[0].is_empty());
        let central_city = scene.bodies.iter().position(|b| b.name == "Central City").unwrap();
        assert!(a[central_city].is_empty());
    }

    #[test]
    fn el_circuito_de_cada_mundo_queda_lejos_de_la_colision_y_se_ve_distinto() {
        let scene = Scene::new();
        let tracks = make_race_tracks(&scene, RACE_SEED);
        for world in 1..=5 {
            let floor = collision_radius(scene.bodies[world].radius);
            for gate in &tracks[world] {
                assert!(
                    gate.offset.length() > floor + 1.0,
                    "{} tiene una puerta a {:.2} de distancia, muy cerca de la colision en {:.2}",
                    scene.bodies[world].name,
                    gate.offset.length(),
                    floor
                );
            }
        }
        // cada mundo tiene su propio color de circuito (RaceTheme)
        let colors: Vec<Vec3> = (1..=5).map(|w| tracks[w][0].color).collect();
        for a in 0..colors.len() {
            for b in (a + 1)..colors.len() {
                assert!((colors[a] - colors[b]).length() > 0.05, "dos mundos comparten el color de su carrera");
            }
        }
    }
}
